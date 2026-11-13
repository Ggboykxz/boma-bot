//! HTTP settings, validated once at startup.

use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

/// An HTTP setting that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HttpConfigError {
    /// A timeout was zero, which would disable the protection it exists for.
    #[error("`{name}` must be greater than zero")]
    ZeroDuration {
        /// Name of the setting, as it appears in the environment.
        name: &'static str,
    },
    /// The request body limit is outside the range the server accepts.
    #[error("`BOMA_MAX_BODY_BYTES` must be between {min} and {max}, got {value}")]
    BodyLimitOutOfRange {
        /// Value the operator supplied.
        value: usize,
        /// Smallest accepted value.
        min: usize,
        /// Largest accepted value.
        max: usize,
    },
    /// The readiness deadline is longer than the whole request deadline.
    #[error("the readiness timeout must not exceed the request timeout")]
    ReadinessTimeoutTooLong,
    /// `BOMA_SESSION_TTL_SECONDS` is outside the accepted bounds.
    #[error("`BOMA_SESSION_TTL_SECONDS` must be between {min} and {max}, got {value}")]
    SessionLifetimeOutOfRange {
        /// Value the operator supplied.
        value: i64,
        /// Smallest accepted value.
        min: i64,
        /// Largest accepted value.
        max: i64,
    },
}

/// How the HTTP server behaves.
///
/// Every value has a bound on purpose: an unbounded body is a memory exhaustion, and an unbounded
/// request is a stuck task (AGENTS.md "Rust standards", REQ-053).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpConfig {
    bind: SocketAddr,
    request_timeout: Duration,
    readiness_timeout: Duration,
    body_limit_bytes: usize,
    trusted_proxies: Vec<IpAddr>,
    session_ttl_seconds: i64,
    secure_cookies: bool,
}

impl HttpConfig {
    /// Smallest accepted request body limit: 4 KiB.
    pub const MIN_BODY_BYTES: usize = 4 * 1024;
    /// Largest accepted request body limit: 1 MiB, raised per route by an explicit decision.
    pub const MAX_BODY_BYTES: usize = 1024 * 1024;
    /// How long a session lives when the operator does not say: 30 days.
    pub const DEFAULT_SESSION_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;
    /// Smallest accepted session lifetime: five minutes.
    pub const MIN_SESSION_TTL_SECONDS: i64 = 300;
    /// Largest accepted session lifetime: sixty days.
    pub const MAX_SESSION_TTL_SECONDS: i64 = 60 * 24 * 60 * 60;
    /// How long an invitation stays valid when the operator does not say: 7 days.
    pub const DEFAULT_INVITATION_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

    /// Validates the parts and builds the configuration.
    ///
    /// # Errors
    ///
    /// Returns [`HttpConfigError::ZeroDuration`] for a zero timeout,
    /// [`HttpConfigError::BodyLimitOutOfRange`] for a body limit outside
    /// `4 KiB..=1 MiB`, and [`HttpConfigError::ReadinessTimeoutTooLong`] when the readiness
    /// deadline outlives the request deadline.
    pub fn new(
        bind: SocketAddr,
        request_timeout: Duration,
        readiness_timeout: Duration,
        body_limit_bytes: usize,
        trusted_proxies: Vec<IpAddr>,
    ) -> Result<Self, HttpConfigError> {
        if request_timeout.is_zero() {
            return Err(HttpConfigError::ZeroDuration {
                name: "BOMA_REQUEST_TIMEOUT_MS",
            });
        }
        if readiness_timeout.is_zero() {
            return Err(HttpConfigError::ZeroDuration {
                name: "BOMA_READINESS_TIMEOUT_MS",
            });
        }
        if readiness_timeout > request_timeout {
            return Err(HttpConfigError::ReadinessTimeoutTooLong);
        }
        if !(Self::MIN_BODY_BYTES..=Self::MAX_BODY_BYTES).contains(&body_limit_bytes) {
            return Err(HttpConfigError::BodyLimitOutOfRange {
                value: body_limit_bytes,
                min: Self::MIN_BODY_BYTES,
                max: Self::MAX_BODY_BYTES,
            });
        }
        Ok(Self {
            bind,
            request_timeout,
            readiness_timeout,
            body_limit_bytes,
            trusted_proxies,
            session_ttl_seconds: Self::DEFAULT_SESSION_TTL_SECONDS,
            secure_cookies: true,
        })
    }

    /// Sets how long a session lives.
    ///
    /// A session is a credential, so its lifetime is a security setting, not a default to be
    /// forgotten: the operator states it (D-08), and the accepted range is bounded rather than
    /// any number passing silently.
    ///
    /// # Errors
    ///
    /// Returns [`HttpConfigError::SessionLifetimeOutOfRange`] when `seconds` is outside the
    /// accepted bounds.
    pub fn with_session_ttl_seconds(mut self, seconds: i64) -> Result<Self, HttpConfigError> {
        if !(Self::MIN_SESSION_TTL_SECONDS..=Self::MAX_SESSION_TTL_SECONDS).contains(&seconds) {
            return Err(HttpConfigError::SessionLifetimeOutOfRange {
                value: seconds,
                min: Self::MIN_SESSION_TTL_SECONDS,
                max: Self::MAX_SESSION_TTL_SECONDS,
            });
        }
        self.session_ttl_seconds = seconds;
        Ok(self)
    }

    /// Sets whether the session cookie carries the `Secure` attribute.
    ///
    /// It is on unless the deployment is explicitly served over plain HTTP in development.
    #[must_use]
    pub const fn with_secure_cookies(mut self, secure: bool) -> Self {
        self.secure_cookies = secure;
        self
    }

    /// How long a session lives, in seconds.
    #[must_use]
    pub const fn session_ttl_seconds(&self) -> i64 {
        self.session_ttl_seconds
    }

    /// Whether the session cookie carries the `Secure` attribute.
    #[must_use]
    pub const fn secure_cookies(&self) -> bool {
        self.secure_cookies
    }

    /// Address the listener binds to; loopback by default.
    #[must_use]
    pub const fn bind(&self) -> SocketAddr {
        self.bind
    }

    /// Deadline for one request.
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Deadline for the whole readiness pass.
    #[must_use]
    pub const fn readiness_timeout(&self) -> Duration {
        self.readiness_timeout
    }

    /// Maximum request body size, in bytes.
    #[must_use]
    pub const fn body_limit_bytes(&self) -> usize {
        self.body_limit_bytes
    }

    /// Peers whose forwarded headers are trusted. Empty means nothing is trusted, so a spoofed
    /// `x-forwarded-for` cannot change the client address (D-04).
    #[must_use]
    pub fn trusted_proxies(&self) -> &[IpAddr] {
        &self.trusted_proxies
    }
}

impl fmt::Display for HttpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "bind={} request_timeout={:?} readiness_timeout={:?} body_limit={}B trusted_proxies={}",
            self.bind,
            self.request_timeout,
            self.readiness_timeout,
            self.body_limit_bytes,
            self.trusted_proxies.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        time::Duration,
    };

    use super::{HttpConfig, HttpConfigError};

    fn loopback() -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, 3000))
    }

    #[test]
    fn req_053_http_config_rejects_a_zero_timeout() {
        let outcome = HttpConfig::new(loopback(), Duration::ZERO, Duration::ZERO, 8192, Vec::new());
        assert!(matches!(outcome, Err(HttpConfigError::ZeroDuration { .. })));
    }

    #[test]
    fn req_053_http_config_bounds_the_body_limit() {
        for limit in [0, HttpConfig::MAX_BODY_BYTES + 1] {
            let outcome = HttpConfig::new(
                loopback(),
                Duration::from_secs(30),
                Duration::from_secs(5),
                limit,
                Vec::new(),
            );
            assert!(matches!(
                outcome,
                Err(HttpConfigError::BodyLimitOutOfRange { .. })
            ));
        }
    }

    #[test]
    fn req_053_http_config_refuses_a_readiness_deadline_longer_than_the_request() {
        let outcome = HttpConfig::new(
            loopback(),
            Duration::from_secs(2),
            Duration::from_secs(5),
            8192,
            Vec::new(),
        );
        assert_eq!(outcome, Err(HttpConfigError::ReadinessTimeoutTooLong));
    }

    #[test]
    fn d_08_the_session_lifetime_is_bounded() {
        for seconds in [
            0,
            HttpConfig::MIN_SESSION_TTL_SECONDS - 1,
            HttpConfig::MAX_SESSION_TTL_SECONDS + 1,
        ] {
            let outcome = HttpConfig::new(
                loopback(),
                Duration::from_secs(30),
                Duration::from_secs(5),
                8192,
                Vec::new(),
            )
            .expect("the fixture is valid")
            .with_session_ttl_seconds(seconds);
            assert!(matches!(
                outcome,
                Err(HttpConfigError::SessionLifetimeOutOfRange { .. })
            ));
        }
        let config = HttpConfig::new(
            loopback(),
            Duration::from_secs(30),
            Duration::from_secs(5),
            8192,
            Vec::new(),
        )
        .expect("the fixture is valid")
        .with_session_ttl_seconds(86_400)
        .expect("a day is accepted");
        assert_eq!(config.session_ttl_seconds(), 86_400);
        assert_eq!(HttpConfig::DEFAULT_SESSION_TTL_SECONDS, 30 * 24 * 60 * 60);
    }

    #[test]
    fn d_08_the_secure_cookies_default_to_true() {
        let config = HttpConfig::new(
            loopback(),
            Duration::from_secs(30),
            Duration::from_secs(5),
            8192,
            Vec::new(),
        )
        .expect("the fixture is valid");
        assert!(config.secure_cookies());
    }

    #[test]
    fn req_053_http_config_defaults_to_trusting_no_proxy() {
        let config = HttpConfig::new(
            loopback(),
            Duration::from_secs(30),
            Duration::from_secs(5),
            8192,
            Vec::new(),
        )
        .expect("the fixture is valid");
        assert_eq!(config.trusted_proxies(), &[] as &[IpAddr]);
        assert_eq!(config.bind(), loopback());
        let config = HttpConfig::new(
            loopback(),
            Duration::from_secs(30),
            Duration::from_secs(5),
            8192,
            vec![IpAddr::from(Ipv4Addr::new(10, 0, 0, 2))],
        )
        .expect("the fixture is valid");
        assert_eq!(config.trusted_proxies().len(), 1);
    }
}
