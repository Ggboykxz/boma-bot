//! Validated settings for the Postgres adapter.
//!
//! The binary parses the environment; this module owns the *rules* a setting must satisfy and the
//! redacted rendering used in logs. A connection string carries a password, so its `Debug` output
//! never contains it (REQ-054).

use std::{fmt, time::Duration};

/// A connection string or pool setting that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The connection string was empty or whitespace.
    #[error("the database URL is empty")]
    EmptyUrl,
    /// The connection string does not use the Postgres scheme.
    #[error("the database URL must start with postgres:// or postgresql://")]
    NotPostgresUrl,
    /// A numeric setting is outside the range the adapter accepts.
    #[error("`{name}` must be between {min} and {max}, got {value}")]
    OutOfRange {
        /// Name of the setting, as it appears in the environment.
        name: &'static str,
        /// Value the operator supplied.
        value: u64,
        /// Smallest accepted value.
        min: u64,
        /// Largest accepted value.
        max: u64,
    },
    /// A duration was zero, which would disable a timeout.
    #[error("`{name}` must be greater than zero")]
    ZeroDuration {
        /// Name of the setting, as it appears in the environment.
        name: &'static str,
    },
}

/// Connection settings for the Postgres adapter.
#[derive(Clone, PartialEq, Eq)]
pub struct DatabaseSettings {
    url: String,
    max_connections: u32,
    acquire_timeout: Duration,
}

impl DatabaseSettings {
    /// Validates the parts and builds the settings.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::EmptyUrl`] or [`ConfigError::NotPostgresUrl`] for an unusable
    /// connection string, [`ConfigError::OutOfRange`] for a pool size outside `1..=100`, and
    /// [`ConfigError::ZeroDuration`] for a zero acquire timeout.
    pub fn new(
        url: impl Into<String>,
        max_connections: u32,
        acquire_timeout: Duration,
    ) -> Result<Self, ConfigError> {
        let url = url.into();
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(ConfigError::EmptyUrl);
        }
        if !(trimmed.starts_with("postgres://") || trimmed.starts_with("postgresql://")) {
            return Err(ConfigError::NotPostgresUrl);
        }
        if !(1..=100).contains(&max_connections) {
            return Err(ConfigError::OutOfRange {
                name: "BOMA_DB_MAX_CONNECTIONS",
                value: u64::from(max_connections),
                min: 1,
                max: 100,
            });
        }
        if acquire_timeout.is_zero() {
            return Err(ConfigError::ZeroDuration {
                name: "BOMA_DB_ACQUIRE_TIMEOUT_MS",
            });
        }
        Ok(Self {
            url: trimmed.to_owned(),
            max_connections,
            acquire_timeout,
        })
    }

    /// The connection string, including its credentials.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Upper bound on pooled connections.
    #[must_use]
    pub const fn max_connections(&self) -> u32 {
        self.max_connections
    }

    /// How long acquiring a connection may wait.
    #[must_use]
    pub const fn acquire_timeout(&self) -> Duration {
        self.acquire_timeout
    }
}

impl fmt::Debug for DatabaseSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseSettings")
            .field("url", &"<redacted: contains credentials>")
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout", &self.acquire_timeout)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ConfigError, DatabaseSettings};

    fn settings(url: &str) -> Result<DatabaseSettings, ConfigError> {
        DatabaseSettings::new(url, 10, Duration::from_secs(5))
    }

    #[test]
    fn req_053_database_settings_reject_an_empty_url() {
        assert_eq!(settings("   "), Err(ConfigError::EmptyUrl));
    }

    #[test]
    fn req_053_database_settings_reject_a_non_postgres_url() {
        assert_eq!(
            settings("mysql://user:pw@localhost/boma"),
            Err(ConfigError::NotPostgresUrl)
        );
    }

    #[test]
    fn req_053_database_settings_reject_an_unbounded_pool() {
        for size in [0, 101] {
            let outcome = DatabaseSettings::new(
                "postgres://boma:boma@localhost/boma",
                size,
                Duration::from_secs(5),
            );
            assert!(matches!(outcome, Err(ConfigError::OutOfRange { .. })));
        }
        let outcome =
            DatabaseSettings::new("postgres://boma:boma@localhost/boma", 10, Duration::ZERO);
        assert!(matches!(outcome, Err(ConfigError::ZeroDuration { .. })));
    }

    #[test]
    fn req_054_database_settings_debug_never_prints_the_password() {
        let url = "postgres://boma:s3cr3t-value@db.local:5432/boma"; // secret-scan: allow (fixture)
        let settings = settings(url).expect("the fixture is a valid Postgres URL");
        let rendered = format!("{settings:?}");
        assert!(!rendered.contains("s3cr3t-value"), "{rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
        assert!(rendered.contains("max_connections: 10"), "{rendered}");
    }
}
