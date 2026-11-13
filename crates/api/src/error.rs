//! The stable error envelope (spec/03-api-contract.md §3.1).
//!
//! Every failure leaves this process as one shape: a machine code a client can branch on and a
//! message that is safe to show. Internal detail — a driver error, a stack trace, another tenant's
//! id — never crosses this boundary.

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use boma_app::AppError;
use serde::{Deserialize, Serialize};

/// The machine-readable code of an error response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// No valid session or token.
    Unauthorized,
    /// Authenticated, but this credential does not open the resource.
    Forbidden,
    /// Missing, or owned by another tenant: the two are indistinguishable on purpose.
    NotFound,
    /// The request body or parameters failed validation.
    BadRequest,
    /// The deployment has no computer backend configured (REQ-027).
    ComputersUnavailable,
    /// The request conflicts with the current state.
    Conflict,
    /// Rate limited.
    TooManyRequests,
    /// The server failed; nothing about the cause is disclosed.
    Internal,
}

impl ErrorCode {
    /// The status code this failure is reported with.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::ComputersUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Conflict => StatusCode::CONFLICT,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// The body of every error response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ErrorBody {
    /// Machine-readable code clients branch on.
    pub code: ErrorCode,
    /// Message that is safe to display to a person.
    pub error: String,
}

/// A failure the API is willing to describe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    code: ErrorCode,
    message: &'static str,
    status: StatusCode,
}

impl ApiError {
    /// Builds a failure from a code and a static, safe message.
    ///
    /// The status comes from the code, which is the right default for every failure a client can
    /// act on.
    #[must_use]
    pub const fn new(code: ErrorCode, message: &'static str) -> Self {
        Self {
            code,
            message,
            status: code.status(),
        }
    }

    /// Missing or expired credentials. The wording matches the golden fixture.
    #[must_use]
    pub const fn unauthorized() -> Self {
        Self::new(ErrorCode::Unauthorized, "Sign in to continue.")
    }

    /// The caller is authenticated but may not do this.
    #[must_use]
    pub const fn forbidden() -> Self {
        Self::new(ErrorCode::Forbidden, "No access.")
    }

    /// Credentials that do not match an account.
    ///
    /// The wording never says whether the address exists (REQ-040).
    #[must_use]
    pub const fn invalid_credentials() -> Self {
        Self::new(ErrorCode::Unauthorized, "Those credentials do not match.")
    }

    /// The request conflicts with what already exists.
    #[must_use]
    pub const fn conflict() -> Self {
        Self::new(ErrorCode::Conflict, "That already exists.")
    }

    /// Another member holds the screen of a computer (REQ-029).
    ///
    /// Same status and code as [`ApiError::conflict`], a different message: the caller is taking
    /// control of a screen a person is already driving, and the answer has to say so.
    #[must_use]
    pub const fn screen_taken() -> Self {
        Self::new(ErrorCode::Conflict, "Someone else has control.")
    }

    /// Absent, or owned by another tenant.
    #[must_use]
    pub const fn not_found() -> Self {
        Self::new(ErrorCode::NotFound, "Not found.")
    }

    /// The request did not validate.
    #[must_use]
    pub const fn bad_request() -> Self {
        Self::new(ErrorCode::BadRequest, "The request is not valid.")
    }

    /// Computers are not available in this deployment: `SANDBOX_PROVIDER=none` or no backend.
    #[must_use]
    pub const fn computers_unavailable() -> Self {
        Self {
            code: ErrorCode::ComputersUnavailable,
            message: "Computers are not available in this deployment.",
            status: StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// A dependency the request needs is down or too slow.
    ///
    /// The code stays `INTERNAL` — the failure is on our side and nothing internal may leak — but
    /// the status is `503`, which is what `spec/openapi.yaml` calls `Unavailable` and what tells a
    /// client the request is worth repeating.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            code: ErrorCode::Internal,
            message: "A dependency is unavailable.",
            status: StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// The machine code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// The safe message.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }

    /// The status this failure is reported with.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Rule(_) => Self::bad_request(),
            AppError::Conflict => Self::new(ErrorCode::Conflict, "That already exists."),
            AppError::StdioDisabled => Self::new(
                ErrorCode::BadRequest,
                "Local command servers are disabled in this deployment. An HTTPS server is the supported path.",
            ),
            AppError::RateLimited => Self::new(
                ErrorCode::TooManyRequests,
                "Too many requests; wait and try again.",
            ),
            AppError::ScreenTaken => Self::screen_taken(),
            AppError::InvalidCredentials => Self::invalid_credentials(),
            AppError::NotFound => Self::not_found(),
            AppError::Unavailable => Self::unavailable(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            code: self.code,
            error: self.message.to_owned(),
        };
        // A cached error is a cached authorization failure; the spec forbids storing either.
        (
            self.status,
            [(header::CACHE_CONTROL, "no-store")],
            Json(body),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        response::IntoResponse,
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::{ApiError, ErrorBody, ErrorCode};

    #[test]
    fn req_040_every_error_code_has_one_status() {
        let pairs = [
            (
                ErrorCode::ComputersUnavailable,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (ErrorCode::Unauthorized, StatusCode::UNAUTHORIZED),
            (ErrorCode::Forbidden, StatusCode::FORBIDDEN),
            (ErrorCode::NotFound, StatusCode::NOT_FOUND),
            (ErrorCode::BadRequest, StatusCode::BAD_REQUEST),
            (ErrorCode::Conflict, StatusCode::CONFLICT),
            (ErrorCode::TooManyRequests, StatusCode::TOO_MANY_REQUESTS),
            (ErrorCode::Internal, StatusCode::INTERNAL_SERVER_ERROR),
        ];
        for (code, status) in pairs {
            assert_eq!(code.status(), status);
        }
    }

    #[tokio::test]
    async fn req_040_the_error_envelope_matches_the_golden_shape() {
        let response = ApiError::unauthorized().into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("the test body is in memory")
            .to_bytes();
        let parsed: ErrorBody =
            serde_json::from_slice(&body).expect("the envelope is valid JSON with two fields");
        assert_eq!(parsed.code, ErrorCode::Unauthorized);
        assert_eq!(parsed.error, "Sign in to continue.");
        let raw: serde_json::Value =
            serde_json::from_slice(&body).expect("the envelope is valid JSON");
        assert_eq!(
            raw.as_object().map(serde_json::Map::len),
            Some(2),
            "the envelope must not grow internals"
        );
    }

    #[test]
    fn req_040_codes_serialise_as_the_documented_uppercase() {
        for (code, wire) in [
            (ErrorCode::Unauthorized, "\"UNAUTHORIZED\""),
            (ErrorCode::TooManyRequests, "\"TOO_MANY_REQUESTS\""),
            (ErrorCode::Internal, "\"INTERNAL\""),
        ] {
            assert_eq!(serde_json::to_string(&code).expect("codes serialise"), wire);
        }
    }

    #[test]
    fn req_053_an_unavailable_dependency_is_a_503_that_stays_internal() {
        let error = ApiError::unavailable();
        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.code(), ErrorCode::Internal);
        assert_eq!(error.message(), "A dependency is unavailable.");
        // A genuine internal failure keeps the 500 its code carries; only the dependency case is
        // a 503, so a client can tell "try again" from "we are broken".
        assert_eq!(
            ApiError::new(ErrorCode::Internal, "boom").status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn req_054_error_responses_are_never_cacheable() {
        let request = Request::builder()
            .uri("/missing")
            .body(Body::empty())
            .expect("a static request builds");
        let app = axum::Router::new().fallback(|| async { ApiError::not_found() });
        let response = app.oneshot(request).await.expect("the router answers");
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
    }
}
