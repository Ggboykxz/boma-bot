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
    /// `BOMA_WEB_ROOT` does not point at a directory with an `index.html`.
    #[error("`{name}` {reason}")]
    Invalid {
        /// Name of the setting, as it appears in the environment.
        name: String,
        /// Why it was refused.
        reason: String,
    },
    /// `BOMA_LOCAL_FIRST` was requested on a bind that is not loopback.
    #[error(
        "`BOMA_LOCAL_FIRST` requires a loopback bind (127.0.0.1 or ::1), got {bind}; the local-first mode is the machine owner's bring-up, never a network-open deployment"
    )]
    LocalFirstNonLoopback {
        /// The bind address the operator supplied.
        bind: SocketAddr,
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
    /// The directory that carries the static web export, when this surface serves it
    /// (REQ-055, ADR-013). `None` means the surface answers only the API.
    web_root: Option<String>,
    /// REQ-066: the local bring-up mode. The operator of a loopback-only machine is the
    /// implicit local owner; no session is required. Never valid on a non-loopback bind.
    local_first: bool,
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
            web_root: None,
            local_first: false,
        })
    }

    /// The directory of the static web export the surface serves, when it serves it.
    #[must_use]
    pub fn web_root(&self) -> Option<&str> {
        self.web_root.as_deref()
    }

    /// Attaches the static web root the surface serves (REQ-055).
    #[must_use]
    pub fn with_web_root(mut self, web_root: Option<String>) -> Self {
        self.web_root = web_root;
        self
    }

    /// Refuses a root that does not hold an export; a deployment that points at a missing
    /// directory is a startup error, never a silent 404-only shell.
    ///
    /// # Errors
    ///
    /// Returns [`HttpConfigError::Invalid { name, .. }`] when the root does not exist, is not a
    /// directory, or does not contain an `index.html`.
    pub fn validate_web_root(self) -> Result<Self, HttpConfigError> {
        if let Some(root) = &self.web_root {
            let path = std::path::Path::new(root.as_str());
            if !path.is_dir() || !path.join("index.html").is_file() {
                return Err(HttpConfigError::Invalid {
                    name: "BOMA_WEB_ROOT".to_owned(),
                    reason: "must point at the static export directory (index.html present)"
                        .to_owned(),
                });
            }
        }
        Ok(self)
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

    /// Enables the local bring-up mode (REQ-066).
    ///
    /// The machine owner becomes the implicit local owner: a request that carries no session
    /// cookie resolves as that principal, and the sign-in round trip disappears. The mode is
    /// only meaningful on a loopback bind, where the network boundary is the machine itself.
    ///
    /// # Errors
    ///
    /// Returns [`HttpConfigError::LocalFirstNonLoopback`] when `local_first` is set and the
    /// configured bind is not loopback — the mode is refused, never silently weakened.
    pub fn with_local_first(mut self, local_first: bool) -> Result<Self, HttpConfigError> {
        if local_first && !self.bind.ip().is_loopback() {
            return Err(HttpConfigError::LocalFirstNonLoopback { bind: self.bind });
        }
        self.local_first = local_first;
        Ok(self)
    }

    /// Whether the local bring-up mode (REQ-066) is on.
    #[must_use]
    pub const fn local_first(&self) -> bool {
        self.local_first
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
            "bind={} request_timeout={:?} readiness_timeout={:?} body_limit={}B trusted_proxies={} local_first={}",
            self.bind,
            self.request_timeout,
            self.readiness_timeout,
            self.body_limit_bytes,
            self.trusted_proxies.len(),
            self.local_first
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

    #[test]
    fn req_066_local_first_defaults_to_off_and_loopback_accepts_it() {
        let config = HttpConfig::new(
            loopback(),
            Duration::from_secs(30),
            Duration::from_secs(5),
            8192,
            Vec::new(),
        )
        .expect("the fixture is valid");
        assert!(!config.local_first());
        let enabled = config
            .with_local_first(true)
            .expect("loopback bind is accepted");
        assert!(enabled.local_first());
        let v6 = HttpConfig::new(
            SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, 3000)),
            Duration::from_secs(30),
            Duration::from_secs(5),
            8192,
            Vec::new(),
        )
        .expect("the fixture is valid")
        .with_local_first(true)
        .expect("the ::1 bind is loopback too");
        assert!(v6.local_first());
        let disabled = enabled
            .with_local_first(false)
            .expect("turning the mode off is accepted");
        assert!(!disabled.local_first());
    }

    #[test]
    fn req_066_local_first_on_a_non_loopback_bind_is_a_boot_error() {
        let public = SocketAddr::from((Ipv4Addr::new(10, 0, 0, 7), 3000));
        let outcome = HttpConfig::new(
            public,
            Duration::from_secs(30),
            Duration::from_secs(5),
            8192,
            Vec::new(),
        )
        .expect("the fixture itself is valid")
        .with_local_first(true);
        assert_eq!(
            outcome,
            Err(HttpConfigError::LocalFirstNonLoopback { bind: public })
        );
    }
}
