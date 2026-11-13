//! The client address for rate limiting, spoof-proof by default (D-01, D-04).
//!
//! A spoofable client address defeats per-IP throttles, so the forwarded headers are honored
//! only when the peer is an operator-listed proxy. Everything else gets its own peer address —
//! the header is treated as a stranger's claim, not a fact.

use std::net::{IpAddr, SocketAddr};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, request::Parts},
};

/// The peer address of a connection, when the server was listening with connection info.
///
/// Missing connection info is not an error: tests, in-process calls and listeners without the
/// peer attached all get `None`, which the resolver maps to loopback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientAddr(pub Option<SocketAddr>);

impl<S> FromRequestParts<S> for ClientAddr
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    // The trait method is `async`, but this body has nothing to await: the value is read from
    // the request's extensions, already parsed, no IO involved.
    #[allow(clippy::unused_async_trait_impl)]
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|info| info.0),
        ))
    }
}

/// Resolves the client address this request is rate-limited by.
///
/// Rules:
///
/// * the peer the server accepted the connection from is the only source of truth unless the
///   operator listed that peer in `BOMA_TRUSTED_PROXIES`;
/// * for a trusted peer, the left-most syntactically valid `x-forwarded-for` entry is the
///   client, because the trusted proxy forwarded the original caller's view of itself;
/// * anything else — no peer to speak of, a garbage header, an unlisted proxy — falls back to
///   the peer itself (or loopback for in-process callers);
/// * when no peer is present at all, loopback stands in: tests and in-process calls rate-limit
///   against one bucket, which is visible and honest.
#[must_use]
pub fn resolve_client_ip(
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    trusted: &[IpAddr],
) -> IpAddr {
    let peer = peer.map_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), |addr| addr.ip());
    if !trusted.contains(&peer) {
        return peer;
    }
    // A trusted proxy: the left-most valid address in its forwarded view is the caller.
    for value in &headers.get_all("x-forwarded-for") {
        let Some(list) = value.to_str().ok() else {
            continue;
        };
        for entry in list.split(',').map(str::trim) {
            if let Ok(ip) = entry.parse::<IpAddr>() {
                return ip;
            }
        }
    }
    peer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(entries: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for entry in entries {
            headers.append(
                "x-forwarded-for",
                axum::http::HeaderValue::from_str(entry).expect("valid header"),
            );
        }
        headers
    }

    #[test]
    fn d_04_an_unlisted_proxy_cannot_steer_the_resolved_address() {
        let peer: SocketAddr = "10.0.0.9:9000".parse().expect("an address");
        let resolved = resolve_client_ip(Some(peer), &headers(&["198.51.100.4"]), &[]);
        assert_eq!(resolved, "10.0.0.9".parse::<IpAddr>().expect("an ip"));
    }

    #[test]
    fn d_04_a_trusted_proxy_steers_by_the_left_most_valid_forwarded_address() {
        let peer: SocketAddr = "10.0.0.2:443".parse().expect("an address");
        let trusted = ["10.0.0.2".parse::<IpAddr>().expect("an ip")];
        let resolved =
            resolve_client_ip(Some(peer), &headers(&["198.51.100.4, 10.0.0.2"]), &trusted);
        assert_eq!(resolved, "198.51.100.4".parse::<IpAddr>().expect("an ip"));
    }

    #[test]
    fn d_04_a_garbage_forwarded_header_falls_back_to_the_peer() {
        let peer: SocketAddr = "10.0.0.2:443".parse().expect("an address");
        let trusted = ["10.0.0.2".parse::<IpAddr>().expect("an ip")];
        let resolved = resolve_client_ip(Some(peer), &headers(&["not-an-ip"]), &trusted);
        assert_eq!(resolved, "10.0.0.2".parse::<IpAddr>().expect("an ip"));
    }

    #[test]
    fn d_04_a_spoof_from_an_untrusted_peer_is_ignored() {
        let peer: SocketAddr = "10.0.0.9:9000".parse().expect("an address");
        let trusted = ["10.0.0.2".parse::<IpAddr>().expect("an ip")];
        let resolved = resolve_client_ip(Some(peer), &headers(&["198.51.100.4"]), &trusted);
        assert_eq!(resolved, "10.0.0.9".parse::<IpAddr>().expect("an ip"));
    }
}
