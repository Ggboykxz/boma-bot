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
use serde::Serialize;

/// The machine-readable code of an error response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
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
}

impl ApiError {
    /// Builds a failure from a code and a static, safe message.
    #[must_use]
    pub const fn new(code: ErrorCode, message: &'static str) -> Self {
        Self { code, message }
    }

    /// Missing or expired credentials. The wording matches the golden fixture.
    #[must_use]
    pub const fn unauthorized() -> Self {
        Self::new(ErrorCode::Unauthorized, "Sign in to continue.")
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

    /// A dependency the request needs is down or too slow.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self::new(ErrorCode::Internal, "A dependency is unavailable.")
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
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Rule(_) => Self::bad_request(),
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
            self.code.status(),
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
