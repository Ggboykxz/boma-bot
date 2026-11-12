//! The sign-in and sign-up surface (REQ-040).
//!
//! The session is a cookie: the browser never sees a token it has to store, and the answer is
//! never cached (REQ-041). The endpoints live under `/api/auth/*`, the path the spec's OpenAPI
//! document reserves for the authentication library.

use axum::{
    Json,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use boma_app::usecases;
use serde::{Deserialize, Serialize};

use crate::{
    auth::{cleared_session_cookie_header, session_cookie_header},
    error::ApiError,
    state::AppState,
};

/// The body of `POST /api/auth/sign-up`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignUpRequest {
    /// The address the person signs in with.
    pub email: String,
    /// The password, in clear: it is hashed here and never stored in clear.
    pub password: String,
    /// The name a person sees.
    #[serde(default)]
    pub name: Option<String>,
    /// The locale the interface is shown in.
    #[serde(default)]
    pub locale: Option<String>,
}

/// The body of `POST /api/auth/sign-in`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignInRequest {
    /// The address the person signs in with.
    pub email: String,
    /// The password, in clear.
    pub password: String,
}

/// The person, as the client needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserView {
    /// The person's id.
    pub id: String,
    /// The address they sign in with.
    pub email: String,
    /// The name a person sees.
    pub name: String,
}

/// The answer of a successful sign-in or sign-up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReply {
    /// The person.
    pub user: UserView,
}

/// `POST /api/auth/sign-up` — creates an account and signs the person in.
///
/// # Errors
///
/// Returns [`ApiError::BadRequest`] when the body does not match the schema, and the error the
/// use case raises when the address is refused or already taken.
pub async fn sign_up(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<Response, ApiError> {
    let request: SignUpRequest = parse(&body)?;
    let name = request
        .name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            request
                .email
                .split('@')
                .next()
                .unwrap_or("member")
                .to_owned()
        });

    let grant = usecases::sign_up(
        state.app(),
        state.http().session_ttl_seconds(),
        &usecases::SignUp {
            email: request.email,
            password: request.password,
            name,
            locale: request.locale.unwrap_or_else(|| "en".to_owned()),
        },
    )
    .await?;

    Ok(session_response(&state, grant))
}

/// `POST /api/auth/sign-in` — signs an existing person in.
///
/// # Errors
///
/// Returns [`ApiError::BadRequest`] when the body does not match the schema, and
/// [`ApiError::InvalidCredentials`] when the pair does not match an account.
pub async fn sign_in(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<Response, ApiError> {
    let request: SignInRequest = parse(&body)?;
    let grant = usecases::sign_in(
        state.app(),
        state.http().session_ttl_seconds(),
        &usecases::SignIn {
            email: request.email,
            password: request.password,
        },
    )
    .await?;
    Ok(session_response(&state, grant))
}

/// `POST /api/auth/sign-out` — revokes the session and clears the cookie.
pub async fn sign_out(State(state): State<AppState>) -> Response {
    let cookie = cleared_session_cookie_header(state.http().secure_cookies());
    let mut response = Json(serde_json::json!({ "ok": true })).into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// The answer of a sign-in or a sign-up, with the session cookie attached.
fn session_response(state: &AppState, grant: boma_app::SessionGrant) -> Response {
    let cookie = session_cookie_header(
        &grant.token,
        state.http().session_ttl_seconds(),
        state.http().secure_cookies(),
    );
    let body = SessionReply {
        user: UserView {
            id: grant.user.id.as_str().to_owned(),
            email: grant.user.email.as_str().to_owned(),
            name: grant.user.name,
        },
    };
    let mut response = (StatusCode::OK, Json(body)).into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// Parses a body into its request type.
fn parse<T: serde::de::DeserializeOwned>(body: &axum::body::Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|error| {
        tracing::debug!(%error, "an auth body did not match its schema");
        ApiError::bad_request()
    })
}

#[cfg(test)]
mod tests {
    use super::{SessionReply, SignInRequest, SignUpRequest, UserView};

    #[test]
    fn req_040_the_sign_up_body_carries_the_pair_of_secrets_and_a_name() {
        let body: SignUpRequest = serde_json::from_str(
            r#"{"email":"person@example.com","password":"correct horse battery","name":"Ada"}"#,
        )
        .expect("the body matches the schema");
        assert_eq!(body.email, "person@example.com");
        assert_eq!(body.name.as_deref(), Some("Ada"));
        assert!(body.locale.is_none());
    }

    #[test]
    fn req_040_an_unknown_field_in_a_sign_up_body_is_refused() {
        let outcome = serde_json::from_str::<SignUpRequest>(
            r#"{"email":"person@example.com","password":"correct horse battery","admin":true}"#,
        );
        assert!(outcome.is_err(), "an unknown field must not be ignored");
    }

    #[test]
    fn req_041_the_session_reply_carries_only_the_person() {
        let reply = SessionReply {
            user: UserView {
                id: "usr_1".to_owned(),
                email: "person@example.com".to_owned(),
                name: "Ada".to_owned(),
            },
        };
        let json = serde_json::to_value(reply).expect("the reply serialises");
        assert_eq!(json["user"]["id"], "usr_1");
        assert!(json.get("token").is_none(), "the token stays in the cookie");
    }

    #[test]
    fn req_041_the_sign_in_body_carries_only_the_pair_of_secrets() {
        let body: SignInRequest = serde_json::from_str(
            r#"{"email":"person@example.com","password":"correct horse battery"}"#,
        )
        .expect("the body matches the schema");
        assert_eq!(body.email, "person@example.com");
    }
}
