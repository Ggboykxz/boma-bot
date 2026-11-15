//! Web Push delivery for runs a person must hear about (REQ-064, improvement program).
//!
//! The delivery path is best-effort end to end: any failure — disabled configuration, a refusing
//! policy, an over-cap count, a wedged push service — logs and returns, and never fails a run.
//! Pushes never carry conversation content: the payload is exactly `{v, kind, title, tag, url}`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use base64::Engine as _;
use boma_domain::{BotId, RunId, SpaceId, TenantContext, UserId};

use crate::error::AppError;
use crate::service::App;

/// The signing material for the deployment's push service.
///
/// Operator-owned, following the same rule as `BOMA_MASTER_KEY`:
/// - `BOMA_VAPID_PRIVATE_KEY` — either a PEM (`-----BEGIN EC PRIVATE KEY-----` or `PRIVATE KEY`)
///   or the raw base64url-no-pad VAPID private key printed by common VAPID generators.
///   Absent or empty → pushes are off entirely; the code is built but inert.
///   Present-and-malformed → fail-fast at boot.
/// - `BOMA_VAPID_SUBJECT` — `mailto:…` or `https:…`; the VAPID `sub` claim.
#[derive(Clone)]
pub struct VapidConfig {
    private_key: String,
    subject: String,
}

impl std::fmt::Debug for VapidConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The private key is never printed: not in traces, not in failures, not by accident.
        f.debug_struct("VapidConfig")
            .field("subject", &self.subject)
            .finish_non_exhaustive()
    }
}

impl VapidConfig {
    /// Reads and validates the operator configuration. Empty or missing means off (`Ok(None)`);
    /// malformed means an error the boot fails on.
    ///
    /// # Errors
    ///
    /// Returns an error when the configuration cannot produce usable signing material.
    pub fn from_env(env: &BTreeMap<String, String>) -> Result<Option<Self>, AppError> {
        let Some(private_key) = env
            .get("BOMA_VAPID_PRIVATE_KEY")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        let _ = parse_builder(&private_key)?;
        let subject = env
            .get("BOMA_VAPID_SUBJECT")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "mailto:admin@localhost".to_owned());
        if !subject.starts_with("mailto:") && !subject.starts_with("https:") {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "BOMA_VAPID_SUBJECT must be a mailto: address or an https: URL".to_owned(),
            )));
        }
        Ok(Some(Self {
            private_key,
            subject,
        }))
    }

    /// The `applicationServerKey` the browser needs, base64url (no padding).
    ///
    /// # Errors
    ///
    /// Returns an error when the key does not parse.
    pub fn public_key_base64url(&self) -> Result<String, AppError> {
        let builder = parse_builder(&self.private_key)?;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(builder.get_public_key()))
    }

    fn signature(
        &self,
        info: &web_push::SubscriptionInfo,
    ) -> Result<web_push::VapidSignature, AppError> {
        let mut builder = parse_builder(&self.private_key)?.add_sub_info(info);
        builder.add_claim("sub", self.subject.clone());
        builder.build().map_err(|_| AppError::Unavailable)
    }
}

fn parse_builder(private_key: &str) -> Result<web_push::PartialVapidSignatureBuilder, AppError> {
    let trimmed = private_key.trim();
    let result = if trimmed.starts_with("-----BEGIN") {
        web_push::VapidSignatureBuilder::from_pem_no_sub(trimmed.as_bytes())
    } else {
        web_push::VapidSignatureBuilder::from_base64_no_sub(trimmed)
    };
    result.map_err(|_| AppError::Unavailable)
}

/// Where in the world a push is for (REQ-064).
#[derive(Debug, Clone)]
pub struct PushTarget {
    /// The space the run is in.
    pub space_id: SpaceId,
    /// The member who started the run; the push goes to them, and them only.
    pub user_id: UserId,
    /// The bot doing the work.
    pub bot_id: BotId,
    /// The run the push is about.
    pub run_id: RunId,
}

/// How the run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// The run finished successfully.
    Completed,
    /// The run failed; opted-in subscribers are told either way.
    Failed,
}

/// The title catalog — three strings per locale, never a name, never a tool.
fn title_for(locale: &str, outcome: Option<PushOutcome>) -> &'static str {
    let language = locale.split('-').next().unwrap_or_default();
    match (language, outcome) {
        ("fr", None) => "Un bot attend votre réponse",
        ("fr", Some(PushOutcome::Completed)) => "Le bot a terminé son exécution",
        ("fr", Some(PushOutcome::Failed)) => "L'exécution du bot a échoué",
        ("he", None) => "בוט ממתין לתשובה",
        ("he", Some(PushOutcome::Completed)) => "הבוט סיים את ההרצה",
        ("he", Some(PushOutcome::Failed)) => "ההרצה של הבוט נכשלה",
        (_, None) => "A bot needs your answer",
        (_, Some(PushOutcome::Completed)) => "The bot finished its run",
        (_, Some(PushOutcome::Failed)) => "The bot run failed",
    }
}

const fn allows(
    policy: &boma_domain::notification::NotificationPolicy,
    kind: boma_domain::notification::NotificationKind,
) -> bool {
    match kind {
        boma_domain::notification::NotificationKind::Waiting => policy.notify_waiting,
        boma_domain::notification::NotificationKind::Completed => policy.notify_completed,
    }
}

/// Serialises the exact payload the spec pins down (REQ-064): five keys, content-free.
#[must_use]
pub fn build_push_payload(
    kind: boma_domain::notification::NotificationKind,
    outcome: Option<PushOutcome>,
    locale: &str,
    run_id: &str,
    bot_id: &str,
) -> String {
    let title = title_for(locale, outcome);
    let url = format!("/app/{bot_id}/");
    serde_json::json!({
        "v": 1,
        "kind": kind.as_str(),
        "title": title,
        "tag": format!("run:{run_id}"),
        "url": url,
    })
    .to_string()
}

/// Notify the initiator that their run waits on a person.
///
/// Silent when push is disabled, the policy forbids it, the cap is reached, or delivery fails.
/// Nothing here may fail an execution.
pub async fn notify_run_waiting(app: &App, target: PushTarget) {
    push_one(
        app,
        target,
        boma_domain::notification::NotificationKind::Waiting,
        None,
    )
    .await;
}

/// Notify the initiator that their run ended.
///
/// Silent in exactly the same ways as [`notify_run_waiting`].
pub async fn notify_run_outcome(app: &App, target: PushTarget, outcome: PushOutcome) {
    push_one(
        app,
        target,
        boma_domain::notification::NotificationKind::Completed,
        Some(outcome),
    )
    .await;
}

async fn push_one(
    app: &App,
    target: PushTarget,
    kind: boma_domain::notification::NotificationKind,
    outcome: Option<PushOutcome>,
) {
    let Some(vapid) = app.services.vapid.clone() else {
        return; // off by operator configuration; silently inert
    };
    let target_user = target.user_id.clone();
    let run_id = target.run_id;
    let bot_id = target.bot_id;
    let context = TenantContext::Member {
        space_id: target.space_id,
        user_id: target.user_id,
    };
    let policy = match app.stores.notifications.get_policy(&context, &bot_id).await {
        Ok(policy) => policy,
        Err(error) => {
            tracing::warn!(%error, "the member's push policy could not be read; skipping");
            return;
        }
    };
    if !allows(&policy, kind) {
        return;
    }
    match app
        .stores
        .notifications
        .count_today(&context, &bot_id)
        .await
    {
        Ok(today) if today >= u64::from(policy.daily_cap) => return,
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, "the push counter could not be read; skipping");
            return;
        }
    }
    let locale = app
        .stores
        .users
        .find(&target_user)
        .await
        .ok()
        .flatten()
        .map_or_else(|| "en-US".to_owned(), |user| user.locale);

    let subscriptions = match app.stores.notifications.list(&context).await {
        Ok(subscriptions) => subscriptions,
        Err(error) => {
            tracing::warn!(%error, "the member's push subscriptions could not be read; skipping");
            return;
        }
    };
    for subscription in subscriptions {
        deliver(
            app,
            &context,
            &vapid,
            &subscription,
            DeliveryPayload {
                kind,
                outcome,
                locale: &locale,
                run_id: &run_id,
                bot_id: &bot_id,
            },
        )
        .await;
    }
}

/// The delivery bundle, factored so `deliver` stays honest about what it needs.
struct DeliveryPayload<'a> {
    kind: boma_domain::notification::NotificationKind,
    outcome: Option<PushOutcome>,
    locale: &'a str,
    run_id: &'a RunId,
    bot_id: &'a BotId,
}

async fn deliver(
    app: &App,
    context: &TenantContext,
    vapid: &VapidConfig,
    subscription: &boma_domain::notification::NotificationSubscription,
    payload: DeliveryPayload<'_>,
) {
    let DeliveryPayload {
        kind,
        outcome,
        locale,
        run_id,
        bot_id,
    } = payload;
    let info = web_push::SubscriptionInfo::new(
        subscription.endpoint.clone(),
        subscription.p256dh.clone(),
        subscription.auth.clone(),
    );
    let Ok(signature) = vapid.signature(&info) else {
        note(app, context, &subscription.id, false).await;
        return;
    };
    let payload = build_push_payload(kind, outcome, locale, run_id.as_str(), bot_id.as_str());
    let mut builder = web_push::WebPushMessageBuilder::new(&info);
    builder.set_payload(web_push::ContentEncoding::Aes128Gcm, payload.as_bytes());
    builder.set_vapid_signature(signature);
    let Ok(message) = builder.build() else {
        note(app, context, &subscription.id, false).await;
        return;
    };
    let request = web_push::request_builder::build_request::<Vec<u8>>(message);
    let (parts, body) = request.into_parts();
    let Ok(method) = reqwest::Method::from_bytes(parts.method.as_str().as_bytes()) else {
        note(app, context, &subscription.id, false).await;
        return;
    };
    let mut request = http_client()
        .request(method, parts.uri.to_string())
        .body(body);
    for (name, value) in &parts.headers {
        request = request.header(name.as_str(), value.as_bytes());
    }
    let Ok(request) = request.build() else {
        note(app, context, &subscription.id, false).await;
        return;
    };
    match http_client().execute(request).await {
        Ok(response) => {
            let status = response.status().as_u16();
            if (200..300).contains(&status) {
                note_success(app, context, &subscription.id).await;
                let id = app.services.ids.new_id("log");
                let _ = app
                    .stores
                    .notifications
                    .record_send(context, &id, bot_id, kind)
                    .await;
            } else if matches!(status, 401 | 403 | 404 | 410) {
                note(app, context, &subscription.id, true).await;
            } else {
                note(app, context, &subscription.id, false).await;
            }
        }
        Err(_) => {
            note(app, context, &subscription.id, false).await;
        }
    }
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            // `reqwest` already carries TLS state from the workspace HTTP client; this cannot
            // fail for any configuration this codebase accepts.
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

async fn note(app: &App, context: &TenantContext, id: &str, permanent: bool) {
    if let Err(error) = app
        .stores
        .notifications
        .note_failure(context, id, permanent)
        .await
    {
        tracing::warn!(%error, permanent, "the push failure counter could not be recorded; ignoring");
    }
}

async fn note_success(app: &App, context: &TenantContext, id: &str) {
    if let Err(error) = app.stores.notifications.note_success(context, id).await {
        tracing::warn!(%error, "the push success could not be recorded; ignoring");
    }
}
