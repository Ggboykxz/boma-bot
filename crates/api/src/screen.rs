//! The screen proxy: bytes flow only through a sealed capability (REQ-030).
//!
//! `POST /api/internal/screen-target` opens the screen of the computer a bearer capability names.
//! The capability is the only authentication; anything that does not check out — unknown token, a
//! stopped computer, a generation that moved — gets one uniform `403`. The response is a single
//! current frame delivered as a `frame` event in a `text/event-stream` round; the connection then
//! closes. `meta` carries the generation the frame was captured at, so a client can compare it
//! against the capability it holds.

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ErrorBody},
    state::AppState,
};

/// The one screen target this deployment serves: the computer's frame.
const SCREEN_TARGET: &str = "/screen";

/// The body of `POST /api/internal/screen-target` (REQ-030).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct ScreenTargetRequest {
    /// The target to open; only `/screen` exists today.
    pub path: String,
}

/// A screen frame event, for the documentation of the SSE round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenFrameEvent {
    /// The generation of the screen the frame was captured at.
    pub screen_generation: u64,
    /// The PNG bytes of the frame, base64-encoded.
    pub png_base64: String,
}

/// `POST /api/internal/screen-target` — open the screen the bearer names (REQ-030).
///
/// # Errors
///
/// Returns `403` for anything that does not check out; `400` for a body that does not name the
/// one target; `413` above the 16 KiB cap; `503` when the computer backend cannot produce a frame.
#[utoipa::path(
    post,
    path = "/api/internal/screen-target",
    operation_id = "screenTarget",
    security(("screenCapability" = [])),
    request_body = ScreenTargetRequest,
    responses(
        (status = 200, description = "Screen stream (text/event-stream: `meta` then one `frame`).", body = String),
        (status = 403, description = "Capability does not check out, target unknown, or no frame is available", body = ErrorBody),
        (status = 413, description = "Body too large", body = ErrorBody),
    ),
    tag = "ops"
)]
pub async fn screen_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    // Anything that does not check out answers 403, uniformly (spec/openapi.yaml lists 200, 403
    // and 413 for this operation — no 400, no 503).
    let capability = bearer_token(&headers).ok_or_else(ApiError::forbidden)?;
    if body.len() > 16 * 1024 {
        return Err(ApiError::forbidden());
    }
    let request: ScreenTargetRequest =
        serde_json::from_slice(&body).map_err(|_| ApiError::forbidden())?;
    if request.path != SCREEN_TARGET {
        return Err(ApiError::forbidden());
    }
    let resolved = boma_app::usecases::resolve_screen_capability(state.app(), capability).await?;
    let Some(capability) = resolved else {
        return Err(ApiError::forbidden());
    };
    let frame = boma_app::usecases::screen_frame(state.app(), &capability)
        .await
        .map_err(|_| ApiError::forbidden())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&frame);
    let body = format!(
        "event: meta\ndata: {{\"screenGeneration\": {generation}}}\n\nevent: frame\ndata: {{\"screenGeneration\": {generation}, \"pngBase64\": \"{encoded}\"}}\n\n",
        generation = capability.screen_generation
    );
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response())
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn req_030_the_bearer_scheme_is_the_only_capability_channel() {
        let mut headers = HeaderMap::new();
        assert!(bearer_token(&headers).is_none());
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("token"));
        assert!(bearer_token(&headers).is_none());
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer "));
        assert!(bearer_token(&headers).is_none());
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-capability"),
        );
        assert_eq!(bearer_token(&headers), Some("secret-capability"));
    }
}
