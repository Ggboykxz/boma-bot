//! The outbound-URL guard (spec/04-nonfunctional.md §4.5, defect D-05).
//!
//! Bots and MCP clients must never reach private, loopback or metadata addresses by accident or
//! by injection. This module is the one place that decides whether an outbound URL is safe to
//! call; every adapter that can make a network call goes through it. The rules, fail-closed:
//!
//! * the scheme is HTTPS, always — a private allow-list does not downgrade to plain HTTP;
//! * no credentials in the URL;
//! * port 443 only, unless the caller holds the per-space private allow-list;
//! * resolution happens here; if ANY resolved address is private, the URL is refused unless the
//!   caller holds the allow-list. The resolution is per-call by design, and redirects are the
//!   caller's to refuse (`reqwest::redirect::Policy::none()`), so a following request never be
//!   validated twice.

use std::{fmt, net::IpAddr};

/// An outbound URL that passed the guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedOutbound {
    /// The hostname the caller should connect to.
    pub host: String,
    /// The port.
    pub port: u16,
    /// Every address the host resolved to, validated. Kept for the audit trail (D-05).
    pub resolved: Vec<IpAddr>,
}

/// Why an outbound URL was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetSafetyError {
    /// The URL does not parse.
    Unparseable,
    /// The scheme is not HTTPS.
    NotHttps,
    /// The URL carries credentials.
    WithCredentials,
    /// The port is not 443 and the caller holds no allow-list.
    NonStandardPort,
    /// The host has no A/AAAA answer, or one of them is private/loopback/metadata.
    NotResolvableOrPrivate,
}

impl fmt::Display for NetSafetyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unparseable => write!(f, "the URL does not parse"),
            Self::NotHttps => write!(f, "the URL must use HTTPS"),
            Self::WithCredentials => write!(f, "the URL must not contain credentials"),
            Self::NonStandardPort => write!(f, "the URL must use port 443"),
            Self::NotResolvableOrPrivate => {
                write!(f, "the host does not resolve to a public address")
            }
        }
    }
}

impl std::error::Error for NetSafetyError {}

/// Whether an address is private, loopback, link-local, or a well-known metadata address.
#[must_use]
pub fn is_private_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            a == 10
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || a == 127
                || (a == 169 && b == 254)
                || (a == 100 && (64..=127).contains(&b))
                || a == 0
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_private_address(IpAddr::V4(mapped));
            }
            let segments = v6.segments();
            matches!(segments[0],
                0x0000 | 0xfe80..=0xfebf // ::/8 unspecified group (incl :: and ::1) and fe80::/10 link-local
                | 0xfc00..=0xfdff        // fc00::/7 unique-local
            )
        }
    }
}

/// The metadata endpoints that must never be reached, not even by the private allow-list.
const METADATA_HOSTS: &[&str] = &[
    "169.254.169.254",
    "metadata.google.internal",
    "metadata",
    "fd00:ec2::254",
];

/// Checks an outbound URL and resolves it, refusing private destinations by default.
///
/// `allow_private` is the per-space allow-list (D-05): only a caller already authorized for a
/// specific server may pass it, and every use must write an audit event. It releases the
/// scheme, port and private-range checks — enough for an HTTP LAN service with no TLS — but
/// never the metadata endpoints, which no legitimate integration reads.
///
/// # Errors
///
/// Returns [`NetSafetyError`] with the rule that refused the URL.
pub async fn check_outbound_url(
    url: &str,
    allow_private: bool,
) -> Result<CheckedOutbound, NetSafetyError> {
    let parsed = url::Url::parse(url).map_err(|_| NetSafetyError::Unparseable)?;
    let scheme = parsed.scheme();
    if scheme != "https" && !allow_private {
        return Err(NetSafetyError::NotHttps);
    }
    if METADATA_HOSTS.contains(&parsed.host_str().unwrap_or_default()) {
        return Err(NetSafetyError::NotResolvableOrPrivate);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(NetSafetyError::WithCredentials);
    }
    let host = parsed
        .host_str()
        .map(str::to_owned)
        .ok_or(NetSafetyError::Unparseable)?;
    let port = parsed
        .port_or_known_default()
        .unwrap_or(if scheme == "http" { 80 } else { 443 });
    if port != 443 && port != 80 && !allow_private {
        return Err(NetSafetyError::NonStandardPort);
    }
    if !allow_private && (port != 443 || scheme != "https") {
        return Err(NetSafetyError::NonStandardPort);
    }
    let resolved: Vec<IpAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|_| NetSafetyError::NotResolvableOrPrivate)?
        .map(|address| address.ip())
        .collect();
    if resolved.is_empty() {
        return Err(NetSafetyError::NotResolvableOrPrivate);
    }
    if METADATA_HOSTS.iter().any(|metadata| {
        resolved
            .iter()
            .any(|address| address.to_string() == *metadata)
    }) {
        return Err(NetSafetyError::NotResolvableOrPrivate);
    }
    if !allow_private && resolved.iter().any(|address| is_private_address(*address)) {
        return Err(NetSafetyError::NotResolvableOrPrivate);
    }
    Ok(CheckedOutbound {
        host,
        port,
        resolved,
    })
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::{NetSafetyError, check_outbound_url, is_private_address};

    #[test]
    fn req_033_ssrf_private_addresses_are_refused() {
        for address in [
            std::net::IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3)),
            std::net::IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1)),
            std::net::IpAddr::V4(Ipv4Addr::new(172, 31, 255, 1)),
            std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
            std::net::IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
            std::net::IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1)),
            std::net::IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            std::net::IpAddr::V6(Ipv6Addr::LOCALHOST),
            std::net::IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        ] {
            assert!(is_private_address(address), "{address}");
        }
        assert!(is_private_address(
            "::ffff:10.0.0.1".parse().expect("a mapped address")
        ));
        assert!(is_private_address(
            "fe80::1".parse().expect("a link-local address")
        ));
        assert!(is_private_address(
            "fd12:3456::".parse().expect("a unique-local address")
        ));
        assert!(!is_private_address(
            "203.0.113.1".parse().expect("a public address")
        ));
    }

    #[tokio::test]
    async fn req_033_ssrf_a_non_https_url_is_refused() {
        assert_eq!(
            check_outbound_url("http://example.test/", false)
                .await
                .unwrap_err(),
            NetSafetyError::NotHttps
        );
    }

    #[tokio::test]
    async fn req_033_ssrf_http_to_loopback_is_allowed_with_the_allow_list() {
        let checked = check_outbound_url("http://127.0.0.1:8080/mcp", true)
            .await
            .expect("the allow-list releases the scheme and the port");
        assert_eq!(checked.port, 8080);
        check_outbound_url("https://127.0.0.1/mcp", true)
            .await
            .expect("the allow-list releases the ranges");
    }

    #[tokio::test]
    async fn req_033_ssrf_the_metadata_endpoint_is_never_allowed() {
        assert_eq!(
            check_outbound_url("http://169.254.169.254/latest/meta-data/", true)
                .await
                .unwrap_err(),
            NetSafetyError::NotResolvableOrPrivate
        );
        assert_eq!(
            check_outbound_url("https://169.254.169.254/latest/meta-data/", false)
                .await
                .unwrap_err(),
            NetSafetyError::NotResolvableOrPrivate
        );
    }

    #[tokio::test]
    async fn req_033_ssrf_https_to_a_private_range_is_refused_without_the_allow_list() {
        assert_eq!(
            check_outbound_url("https://192.168.0.1/mcp", false)
                .await
                .unwrap_err(),
            NetSafetyError::NotResolvableOrPrivate
        );
    }

    #[tokio::test]
    async fn req_033_ssrf_a_url_with_credentials_is_refused() {
        assert_eq!(
            check_outbound_url("https://user:pass@example.test/", false)
                .await
                .unwrap_err(),
            NetSafetyError::WithCredentials
        );
    }

    #[tokio::test]
    async fn req_033_ssrf_a_non_standard_port_is_refused() {
        assert_eq!(
            check_outbound_url("https://example.test:8443/", false)
                .await
                .unwrap_err(),
            NetSafetyError::NonStandardPort
        );
    }

    #[tokio::test]
    async fn req_033_ssrf_a_private_hostname_is_refused() {
        assert_eq!(
            check_outbound_url("https://metadata.google.internal/", false)
                .await
                .unwrap_err(),
            NetSafetyError::NotResolvableOrPrivate
        );
    }
}
