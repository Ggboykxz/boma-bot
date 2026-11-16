//! The local automation surface (REQ-072).
//!
//! `/api/v1/automation/*` is how scripts and operators drive bots on this machine: send a
//! message (and wait for the answer if asked), fire a routine once, list the pending approvals
//! and answer them. The caller authenticates with `Authorization: Bearer ...` against
//! `BOMA_AUTOMATION_TOKEN`. Every call resolves as the deployment's local owner (the same
//! principal the loopback local-first mode uses) — the machine is the trust anchor. An agent
//! must be listed in `BOMA_AUTOMATION_AGENTS` (`*` for all); anything else is a uniform grant.
//!
//! The surface is off unless both variables are set; then it behaves exactly like a signed-in
//! member with the automation allow-list applied.

use std::time::Duration;

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use tower_http::{set_header::SetResponseHeaderLayer, timeout::TimeoutLayer};

use boma_app::usecases;

use crate::{ApiError, AppState, correlation};

type Response = axum::response::Response<axum::body::Body>;

/// 404 with a short body: the automation surface is absent — same shape every time.
fn surface_absent() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// Uniform grant-based refusal; never reveals which bots exist.
fn uniform_refusal() -> (StatusCode, Bytes) {
    (
        StatusCode::FORBIDDEN,
        Bytes::from(r#"{"error":"the token may not drive that agent"}"#),
    )
}

/// Checks the bearer against the configured token in constant time.
fn bearer_ok(state: &AppState, headers: &axum::http::HeaderMap) -> bool {
    let Some(expected) = state.app().services.automation_token.as_ref() else {
        return false;
    };
    let Some(provided) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };
    let tokens = state.app().services.tokens.as_ref();
    let Ok(provided_hash) = tokens.hash(provided) else {
        return false;
    };
    let Ok(expected_hash) = tokens.hash(secrecy::ExposeSecret::expose_secret(expected)) else {
        return false;
    };
    provided_hash.len() == expected_hash.len()
        && provided_hash
            .iter()
            .zip(expected_hash.iter())
            .fold(0_u8, |accumulator, (left, right)| {
                accumulator | (left ^ right)
            })
            == 0
}

/// The uniform principal of the automation surface: the local owner (like local-first).
async fn automation_context(
    state: &AppState,
) -> Result<(boma_domain::TenantContext, boma_domain::User), ApiError> {
    let owner = usecases::ensure_local_owner(state.app()).await?;
    let actor = usecases::resolve_actor(state.app(), &owner, None).await?;
    Ok((actor.context.clone(), owner))
}

/// Whether this bot may be driven with the automation token.
///
/// `automation_agents` is `None` when the operator configured `*` (all bots); `Some(list)`
/// restricts the token to the listed bot ids.
fn agent_enabled(state: &AppState, bot_id: &str) -> bool {
    match state.app().services.automation_agents.as_ref() {
        None => true,
        Some(agents) => agents.iter().any(|allowed| allowed == bot_id),
    }
}

/// Waits until the run reaches a terminal status or the deployment's automation timeout
/// elapses, polling at 250ms.
///
/// Returns the terminal status name, or a ready-made response for every other outcome.
async fn await_terminal_run(
    state: &AppState,
    context: &boma_domain::TenantContext,
    run_id: &boma_domain::RunId,
    timeout_seconds: u64,
) -> Result<String, Box<Response>> {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(timeout_seconds.max(1));
    loop {
        let run = match state.app().stores.runs.find(context, run_id).await {
            Ok(Some(run)) => run,
            Ok(None) => {
                return Err(Box::new(
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        axum::Json(serde_json::json!({ "error": "the run vanished" })),
                    )
                        .into_response(),
                ));
            }
            Err(_) => {
                return Err(Box::new(StatusCode::SERVICE_UNAVAILABLE.into_response()));
            }
        };
        if run.status.is_terminal() {
            return Ok(run.status.as_str().to_owned());
        }
        if std::time::Instant::now() >= deadline {
            return Err(Box::new(
                (
                    StatusCode::REQUEST_TIMEOUT,
                    axum::Json(serde_json::json!({
                        "runId": run.id.as_str(),
                        "status": "waiting",
                        "error": "the run did not finish before the timeout",
                    })),
                )
                    .into_response(),
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

/// The body of `POST /api/v1/automation/agents/{bot_id}/messages`.
#[derive(Debug, serde::Deserialize)]
struct SendMessageRequest {
    /// The message text for the agent.
    text: String,
    /// Wait for a terminal run before answering; the default is `false`.
    #[serde(default)]
    wait: bool,
    /// How long to wait; bounded by the deployment's `BOMA_AUTOMATION_WAIT_TIMEOUT_SECONDS`.
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

/// Wire shape of one automation-enabled agent.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentView {
    /// The bot id.
    id: String,
    /// The bot's name.
    name: String,
    /// The bot's purpose, when set.
    description: Option<String>,
}

/// Wire shape of one pending approval card.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ApprovalCardView {
    /// The card id.
    id: String,
    /// The thread the card was parked in.
    thread_id: String,
    /// The run waiting on this card.
    run_id: Option<String>,
    /// The kind of card (approval, question, takeover).
    kind: String,
    /// When the card was created.
    created_at: String,
}

/// `GET /api/v1/automation/agents` — the agents the token may drive.
async fn agents_list(State(state): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    if state.app().services.automation_token.is_none() {
        return surface_absent().into_response();
    }
    if !bearer_ok(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let (context, _owner) = match automation_context(&state).await {
        Ok(pair) => pair,
        Err(error) => return error.into_response(),
    };
    let bots = match usecases::list_bots(state.app(), &context).await {
        Ok(bots) => bots,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let agents: Vec<AgentView> = bots
        .into_iter()
        .filter(|summary| agent_enabled(&state, summary.bot.id.as_str()))
        .map(|summary| AgentView {
            id: summary.bot.id.as_str().to_owned(),
            name: summary.bot.name.clone(),
            description: summary.bot.description.clone(),
        })
        .collect();
    axum::Json(serde_json::json!({ "agents": agents })).into_response()
}

/// `POST /api/v1/automation/agents/{bot_id}/messages` — send one message, optionally wait.
async fn send_message(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(bot_id): Path<String>,
    body: Bytes,
) -> Response {
    if state.app().services.automation_token.is_none() {
        return surface_absent().into_response();
    }
    if !bearer_ok(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !agent_enabled(&state, &bot_id) {
        return uniform_refusal().into_response();
    }
    let request: SendMessageRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": "body is `{\"text\": string, \"wait?\": bool, \"timeoutSeconds?\": u64}`"
            })),
        )
            .into_response(),
    };
    if request.text.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": "text must not be empty" })),
        )
            .into_response();
    }
    let (context, _owner) = match automation_context(&state).await {
        Ok(pair) => pair,
        Err(error) => return error.into_response(),
    };
    let outcome = match usecases::send_message(
        state.app(),
        &context,
        &boma_app::usecases::SendMessage {
            bot_id,
            text: request.text,
            client_nonce: None,
            reply_to_message_id: None,
            reply_quote: None,
        },
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let Some(run_id_value) = outcome.runs.first().map(|run| run.id.as_str().to_owned()) else {
        return axum::Json(serde_json::json!({ "error": "no run was created" })).into_response();
    };
    let run_id = boma_domain::RunId::new(run_id_value.clone()).map_err(|_| ApiError::unavailable());
    let run_id = match run_id {
        Ok(run_id) => run_id,
        Err(error) => return error.into_response(),
    };
    if !request.wait {
        return axum::Json(serde_json::json!({
            "messageId": outcome.message.id.as_str(),
            "runId": run_id_value,
            "status": "queued",
        }))
        .into_response();
    }
    let timeout = request
        .timeout_seconds
        .unwrap_or(state.app().services.automation_wait_timeout_seconds)
        .min(state.app().services.automation_wait_timeout_seconds);
    match await_terminal_run(&state, &context, &run_id, timeout).await {
        Ok(status) => axum::Json(serde_json::json!({
            "runId": run_id_value,
            "status": status,
        }))
        .into_response(),
        Err(response) => *response,
    }
}

/// The body of `POST /api/v1/automation/agents/{bot_id}/routines/{routine_id}/run`.
#[derive(Debug, serde::Deserialize)]
struct RoutineRunRequest {
    /// Whether to wait for the routine's run to finish; the default is `false`.
    #[serde(default)]
    wait: bool,
}

/// `POST /api/v1/automation/agents/{bot_id}/routines/{routine_id}/run` — fire a routine once.
async fn routine_run(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path((bot_id, routine_id)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    if state.app().services.automation_token.is_none() {
        return surface_absent().into_response();
    }
    if !bearer_ok(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !agent_enabled(&state, &bot_id) {
        return uniform_refusal().into_response();
    }
    let request: RoutineRunRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": "body is `{\"wait?\": bool}`" })),
            )
                .into_response();
        }
    };
    let (context, _owner) = match automation_context(&state).await {
        Ok(pair) => pair,
        Err(error) => return error.into_response(),
    };
    let outcome = match usecases::routines_run_now(state.app(), &context, &routine_id).await {
        Ok(outcome) => outcome,
        Err(error) => return ApiError::from(error).into_response(),
    };
    // The routine belongs to this agent or the refusal is uniform.
    if outcome.routine.bot_id.as_str() != bot_id {
        return uniform_refusal().into_response();
    }
    let run_id = outcome.run.id.as_str().to_owned();
    if !request.wait {
        return axum::Json(serde_json::json!({
            "runId": run_id,
            "status": "queued",
        }))
        .into_response();
    }
    match await_terminal_run(
        &state,
        &context,
        &outcome.run.id,
        state.app().services.automation_wait_timeout_seconds,
    )
    .await
    {
        Ok(status) => axum::Json(serde_json::json!({
            "runId": outcome.run.id.as_str(),
            "status": status,
        }))
        .into_response(),
        Err(response) => *response,
    }
}

/// `GET /api/v1/automation/approvals` — the pending approval cards of this deployment.
async fn approvals_list(State(state): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    if state.app().services.automation_token.is_none() {
        return surface_absent().into_response();
    }
    if !bearer_ok(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let (context, _owner) = match automation_context(&state).await {
        Ok(pair) => pair,
        Err(error) => return error.into_response(),
    };
    let cards = match state.app().stores.cards.list_pending(&context).await {
        Ok(cards) => cards,
        Err(error) => return ApiError::from(error).into_response(),
    };
    let cards: Vec<ApprovalCardView> = cards
        .into_iter()
        .map(|card| ApprovalCardView {
            id: card.id.as_str().to_owned(),
            thread_id: card.thread_id.as_str().to_owned(),
            run_id: card.run_id.map(|run_id| run_id.as_str().to_owned()),
            kind: serde_json::to_string(&card.kind)
                .unwrap_or_else(|_| "approval".to_owned())
                .trim_matches('"')
                .to_owned(),
            created_at: card.created_at,
        })
        .collect();
    axum::Json(serde_json::json!({ "approvals": cards })).into_response()
}

/// `GET /api/v1/automation/runs/{run_id}` — the state of one run (poll until it leaves `running`).
async fn runs_get(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(run_id): Path<String>,
) -> Response {
    if state.app().services.automation_token.is_none() {
        return surface_absent().into_response();
    }
    if !bearer_ok(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let (context, _owner) = match automation_context(&state).await {
        Ok(pair) => pair,
        Err(error) => return error.into_response(),
    };
    let Ok(run_id) = boma_domain::RunId::new(run_id) else {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "error": "no such run" })),
        )
            .into_response();
    };
    match state.app().stores.runs.find(&context, &run_id).await {
        Ok(Some(run)) => axum::Json(serde_json::json!({
            "runId": run.id.as_str(),
            "status": run.status.as_str(),
        }))
        .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "error": "no such run" })),
        )
            .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

/// The body of `POST /api/v1/automation/approvals/{card_id}/answer`.
#[derive(Debug, serde::Deserialize)]
struct ApprovalAnswerRequest {
    /// `approve` or `deny`.
    answer: String,
}

/// `POST /api/v1/automation/approvals/{card_id}/answer` — answer a pending approval card.
async fn approvals_answer(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(card_id): Path<String>,
    body: Bytes,
) -> Response {
    if state.app().services.automation_token.is_none() {
        return surface_absent().into_response();
    }
    if !bearer_ok(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let request: ApprovalAnswerRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": "body is `{\"answer\": string}`" })),
            )
                .into_response();
        }
    };
    let (context, _owner) = match automation_context(&state).await {
        Ok(pair) => pair,
        Err(error) => return error.into_response(),
    };
    let outcome =
        match usecases::answer_card(state.app(), &context, &card_id, &request.answer).await {
            Ok(outcome) => outcome,
            Err(error) => return ApiError::from(error).into_response(),
        };
    axum::Json(serde_json::json!({
        "id": outcome.card.id.as_str(),
        "answer": outcome.card.answer,
    }))
    .into_response()
}

/// Builds the automation surface; nested into the root router under `/api/v1/automation`,
/// *outside* the global request timeout: `wait=true` calls are long polls by design. The surface
/// carries the same response-header layers and request logging as the main router, a 64 KiB
/// body cap, and its own timeout sized to the configured automation wait plus a grace. The
/// returned router is final (it owns its state) so the root may nest it without supplying one.
pub fn router(state: AppState, wait_timeout_seconds: u64) -> Router {
    let correlation_state = state.clone();
    Router::new()
        .route("/agents", axum::routing::get(agents_list))
        .route(
            "/agents/{bot_id}/messages",
            axum::routing::post(send_message),
        )
        .route(
            "/agents/{bot_id}/routines/{routine_id}/run",
            axum::routing::post(routine_run),
        )
        .route("/approvals", axum::routing::get(approvals_list))
        .route(
            "/approvals/{card_id}/answer",
            axum::routing::post(approvals_answer),
        )
        .route("/runs/{run_id}", axum::routing::get(runs_get))
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(wait_timeout_seconds.saturating_add(5)),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(axum::middleware::from_fn_with_state(
            correlation_state,
            correlation::log_http,
        ))
        .with_state(state)
}
