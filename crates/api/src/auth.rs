//! Sessions, spaces and the identity a request carries (REQ-040, REQ-041, REQ-043).
//!
//! A session is an opaque token in a cookie; the database only ever holds its hash. A request
//! without a valid session is unauthorized; a request that names a space the caller does not
//! belong to gets *not found*, never *forbidden*, because the difference would confirm that the
//! space exists.

use axum::http::{HeaderMap, header};
use boma_app::{Actor, usecases};

use crate::{error::ApiError, rpc::requested_space};

/// Who a request runs as, and in which space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    /// The signed-in member and the tenant they act in.
    pub actor: Actor,
}

/// Reads the session cookie out of the request headers.
#[must_use]
pub fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|cookie| {
        let (name, value) = cookie.split_once('=')?;
        (name.trim() == "session").then(|| value.trim())
    })
}

/// Builds the `Set-Cookie` header that carries a session.
///
/// The cookie is `httpOnly` (a script cannot read it), `SameSite=Lax` (a cross-site form cannot
/// carry it) and `Secure` when the deployment says the origin is secure. `Path=/` keeps it for
/// every route (D-08).
#[must_use]
pub fn session_cookie_header(token: &str, max_age_seconds: i64, secure: bool) -> String {
    let mut cookie =
        format!("session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_seconds}");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Builds the `Set-Cookie` header that clears a session.
#[must_use]
pub fn cleared_session_cookie_header(secure: bool) -> String {
    let mut cookie = "session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0".to_owned();
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Resolves the caller of a request.
///
/// # Errors
///
/// Returns [`ApiError::Unauthorized`] when there is no session, and [`ApiError::NotFound`] when
/// the caller belongs to no space at all.
pub async fn authenticate(
    app: &boma_app::App,
    headers: &HeaderMap,
    session_ttl_seconds: i64,
) -> Result<AuthContext, ApiError> {
    let Some(token) = session_cookie(headers) else {
        return Err(ApiError::unauthorized());
    };
    let Some((user, _expires_at)) = usecases::resolve_session(app, token).await? else {
        return Err(ApiError::unauthorized());
    };
    let requested = requested_space(headers);
    let actor = usecases::resolve_actor(app, &user, requested.as_deref()).await?;
    let _ = session_ttl_seconds;
    Ok(AuthContext { actor })
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;

    use super::{cleared_session_cookie_header, session_cookie, session_cookie_header};

    #[test]
    fn req_041_the_session_cookie_is_read_from_a_cookie_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            "theme=dark; session=abc123; other=1".parse().unwrap(),
        );
        assert_eq!(session_cookie(&headers), Some("abc123"));
    }

    #[test]
    fn req_041_a_request_without_a_session_cookie_has_none() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", "theme=dark".parse().unwrap());
        assert_eq!(session_cookie(&headers), None);
        assert_eq!(session_cookie(&HeaderMap::new()), None);
    }

    #[test]
    fn req_041_the_session_cookie_is_http_only_and_same_site_lax() {
        let header = session_cookie_header("token", 3600, false);
        assert!(header.contains("HttpOnly"), "{header}");
        assert!(header.contains("SameSite=Lax"), "{header}");
        assert!(!header.contains("Secure"), "{header}");
        let secure = session_cookie_header("token", 3600, true);
        assert!(secure.contains("Secure"), "{secure}");
        assert!(cleared_session_cookie_header(true).contains("Max-Age=0"));
    }
}
