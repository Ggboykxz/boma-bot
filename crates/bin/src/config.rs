//! Typed configuration, parsed from the environment once at startup.
//!
//! Rules live in the crates that own them (`boma-infra` for the pool, `boma-api` for the HTTP
//! bounds); this module only reads variables, supplies documented defaults and fails fast on a
//! value the deployment cannot use. Secrets never reach a log line: `Debug` and `Display` both
//! print the redacted form (REQ-054).

use std::{
    collections::BTreeMap,
    fmt,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    time::Duration,
};

use boma_api::{HttpConfig, HttpConfigError};
use boma_app::Capabilities;
use boma_infra::{ConfigError as DatabaseConfigError, DatabaseSettings};

/// A configuration variable that is missing or unusable.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required variable is absent.
    #[error("`{name}` is required")]
    Missing {
        /// Variable name.
        name: &'static str,
    },
    /// A variable is present but does not parse.
    #[error("`{name}` is not valid: {value}")]
    Invalid {
        /// Variable name.
        name: &'static str,
        /// Value the operator supplied, already trimmed.
        value: String,
    },
    /// The database settings were rejected.
    #[error(transparent)]
    Database(#[from] DatabaseConfigError),
    /// The HTTP settings were rejected.
    #[error(transparent)]
    Http(#[from] HttpConfigError),
}

/// How logs are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable lines, for a terminal.
    Pretty,
    /// One JSON object per line, for a collector.
    Json,
}

/// Logging settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetrySettings {
    /// `tracing` filter expression, for example `info,boma_api=debug`.
    pub filter: String,
    /// Rendering format.
    pub format: LogFormat,
}

/// Everything the binary needs, already validated.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP bounds and the listener address.
    pub http: HttpConfig,
    /// Postgres connection settings.
    pub database: DatabaseSettings,
    /// Logging.
    pub telemetry: TelemetrySettings,
    /// What `/ready` reports.
    pub capabilities: Capabilities,
}

impl Config {
    /// Reads the process environment.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a required variable is missing or any value fails validation.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_pairs(std::env::vars())
    }

    /// Reads configuration from an arbitrary set of pairs.
    ///
    /// This is the testable form of [`Config::from_env`]: a test supplies its own pairs and never
    /// mutates the process environment.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a required variable is missing or any value fails validation.
    pub fn from_pairs<I, K, V>(pairs: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let env: BTreeMap<String, String> = pairs
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();

        let database = DatabaseSettings::new(
            required(&env, "BOMA_DATABASE_URL")?,
            optional_parsed(&env, "BOMA_DB_MAX_CONNECTIONS", 10_u32)?,
            Duration::from_millis(optional_parsed(
                &env,
                "BOMA_DB_ACQUIRE_TIMEOUT_MS",
                5_000_u64,
            )?),
        )?;

        let http = HttpConfig::new(
            optional_parsed(
                &env,
                "BOMA_BIND_ADDR",
                SocketAddr::from(([127, 0, 0, 1], 3000)),
            )?,
            Duration::from_millis(optional_parsed(&env, "BOMA_REQUEST_TIMEOUT_MS", 30_000_u64)?),
            Duration::from_millis(optional_parsed(&env, "BOMA_READINESS_TIMEOUT_MS", 5_000_u64)?),
            optional_parsed(&env, "BOMA_MAX_BODY_BYTES", 64 * 1024_usize)?,
            parse_ip_list(&env, "BOMA_TRUSTED_PROXIES")?,
        )?;

        let format = match optional(&env, "BOMA_LOG_FORMAT").as_deref() {
            None | Some("pretty") => LogFormat::Pretty,
            Some("json") => LogFormat::Json,
            Some(other) => {
                return Err(ConfigError::Invalid {
                    name: "BOMA_LOG_FORMAT",
                    value: other.to_owned(),
                });
            }
        };

        let capabilities = Capabilities {
            version: optional(&env, "BOMA_VERSION")
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()),
            runtime: optional(&env, "BOMA_RUNTIME").unwrap_or_else(|| "sandboxed".to_owned()),
            sandbox: optional(&env, "BOMA_SANDBOX").unwrap_or_else(|| "none".to_owned()),
            integrations: parse_string_list(&env, "BOMA_INTEGRATIONS"),
            messaging: optional_parsed(&env, "BOMA_MESSAGING", false)?,
            email: optional(&env, "BOMA_EMAIL"),
            billing: optional(&env, "BOMA_BILLING"),
            jobs: optional(&env, "BOMA_JOBS").unwrap_or_else(|| "inline".to_owned()),
            realtime: optional(&env, "BOMA_REALTIME").unwrap_or_else(|| "postgres".to_owned()),
            degraded: optional_parsed(&env, "BOMA_DEGRADED", false)?,
            revision: optional(&env, "BOMA_REVISION"),
        };

        Ok(Self {
            http,
            database,
            telemetry: TelemetrySettings {
                filter: optional(&env, "BOMA_LOG_FILTER").unwrap_or_else(|| "info".to_owned()),
                format,
            },
            capabilities,
        })
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "http=[{}] database=[{:?}] logs=[filter={} format={:?}] version={}",
            self.http, self.database, self.telemetry.filter, self.telemetry.format,
            self.capabilities.version
        )
    }
}

fn required(env: &BTreeMap<String, String>, name: &'static str) -> Result<String, ConfigError> {
    optional(env, name).ok_or(ConfigError::Missing { name })
}

fn optional(env: &BTreeMap<String, String>, name: &str) -> Option<String> {
    env.get(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn optional_parsed<T>(
    env: &BTreeMap<String, String>,
    name: &'static str,
    default: T,
) -> Result<T, ConfigError>
where
    T: FromStr,
{
    optional(env, name).map_or(Ok(default), |value| {
        value.parse::<T>().map_err(|_| ConfigError::Invalid {
            name,
            value,
        })
    })
}

fn parse_ip_list(
    env: &BTreeMap<String, String>,
    name: &'static str,
) -> Result<Vec<IpAddr>, ConfigError> {
    let raw = optional(env, name).unwrap_or_default();
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<IpAddr>().map_err(|_| ConfigError::Invalid {
                name,
                value: part.to_owned(),
            })
        })
        .collect()
}

fn parse_string_list(env: &BTreeMap<String, String>, name: &str) -> Vec<String> {
    optional(env, name)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Config, ConfigError, LogFormat};

    fn minimum() -> Vec<(&'static str, &'static str)> {
        vec![(
            "BOMA_DATABASE_URL",
            "postgres://boma:boma@localhost:5432/boma",
        )]
    }

    #[test]
    fn req_053_config_requires_a_database_url() {
        let outcome = Config::from_pairs(Vec::<(&str, &str)>::new());
        assert!(matches!(outcome, Err(ConfigError::Missing { .. })));
        assert_eq!(
            Config::from_pairs([("BOMA_DATABASE_URL", "  ")]).err().map(|e| e.to_string()),
            Some("`BOMA_DATABASE_URL` is required".to_owned())
        );
    }

    #[test]
    fn req_053_config_defaults_to_loopback_with_bounded_limits() {
        let config = Config::from_pairs(minimum()).expect("the minimum configuration is valid");
        assert_eq!(config.http.bind().ip().to_string(), "127.0.0.1");
        assert_eq!(config.http.bind().port(), 3000);
        assert_eq!(config.http.body_limit_bytes(), 64 * 1024);
        assert!(config.http.trusted_proxies().is_empty(), "no proxy is trusted by default");
        assert_eq!(config.database.max_connections(), 10);
        assert_eq!(config.telemetry.format, LogFormat::Pretty);
        assert_eq!(config.capabilities.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn req_054_config_display_and_debug_redact_the_database_url() {
        let config = Config::from_pairs([(
            "BOMA_DATABASE_URL",
            "postgres://boma:s3cr3t-value@db.local:5432/boma", // secret-scan: allow (test fixture)
        )])
        .expect("the fixture is valid");
        for rendered in [format!("{config}"), format!("{config:?}")] {
            assert!(!rendered.contains("s3cr3t-value"), "{rendered}");
        }
    }

    #[test]
    fn req_053_config_rejects_values_it_cannot_use() {
        let mut pairs = minimum();
        pairs.push(("BOMA_LOG_FORMAT", "yaml"));
        assert!(matches!(
            Config::from_pairs(pairs),
            Err(ConfigError::Invalid {
                name: "BOMA_LOG_FORMAT",
                ..
            })
        ));

        let mut pairs = minimum();
        pairs.push(("BOMA_TRUSTED_PROXIES", "10.0.0.2, not-an-ip"));
        assert!(matches!(
            Config::from_pairs(pairs),
            Err(ConfigError::Invalid {
                name: "BOMA_TRUSTED_PROXIES",
                ..
            })
        ));
    }

    #[test]
    fn req_053_config_reads_the_capability_report_from_the_environment() {
        let mut pairs = minimum();
        pairs.extend([
            ("BOMA_VERSION", "9.9.9"),
            ("BOMA_RUNTIME", "disabled"),
            ("BOMA_SANDBOX", "docker"),
            ("BOMA_INTEGRATIONS", "github, linear"),
            ("BOMA_MESSAGING", "true"),
            ("BOMA_EMAIL", "bot@boma.test"),
            ("BOMA_BILLING", "none"),
            ("BOMA_JOBS", "postgres"),
            ("BOMA_REALTIME", "postgres"),
            ("BOMA_DEGRADED", "true"),
            ("BOMA_REVISION", "abc1234"),
            ("BOMA_LOG_FORMAT", "json"),
        ]);
        let config = Config::from_pairs(pairs).expect("the fixture is valid");
        assert_eq!(config.capabilities.version, "9.9.9");
        assert_eq!(config.capabilities.integrations, ["github", "linear"]);
        assert!(config.capabilities.messaging);
        assert_eq!(config.capabilities.email.as_deref(), Some("bot@boma.test"));
        assert_eq!(config.capabilities.revision.as_deref(), Some("abc1234"));
        assert!(config.capabilities.degraded);
        assert_eq!(config.telemetry.format, LogFormat::Json);
    }
}
