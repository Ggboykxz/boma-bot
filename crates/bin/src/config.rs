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

use boma_api::{AuthCapabilities, HttpConfig, HttpConfigError};
use boma_app::Capabilities;
use boma_infra::{ConfigError as DatabaseConfigError, DatabaseSettings};

use crate::computer::{ComputerConfig, ComputerConfigError, from_pairs as computer_from_pairs};

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
    /// The computer selection was rejected.
    #[error(transparent)]
    Computer(#[from] ComputerConfigError),
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
    /// The computer backend this deployment is configured with (REQ-027).
    pub computer: ComputerConfig,
    /// Which sign-in methods the deployment offers.
    pub auth_capabilities: AuthCapabilities,
    /// The bring-your-own-key provider, when the deployment is configured with one (REQ-015).
    ///
    /// `None` means the scripted provider answers: the deployment is exercised end to end
    /// without a model, exactly the state an operator wants before wiring one.
    pub provider: Option<ProviderConfig>,
    /// The stdio allow-list for local MCP servers (`MCP_STDIO_ALLOWED_COMMANDS`).
    ///
    /// Empty by default: stdio MCP servers are refused everywhere in this build until the
    /// operator names their programs here.
    pub stdio_allowed: Vec<String>,
}

/// The bring-your-own-key OpenAI-compatible provider (REQ-015).
///
/// The three `BOMA_PROVIDER_*` variables come together or the provider is not configured; a
/// half-configured provider is a startup error, never a silent downgrade to the script.
#[derive(Clone)]
pub struct ProviderConfig {
    /// The base URL, for example `https://api.openai.com/v1`.
    pub endpoint: String,
    /// The model identifier the deployment calls.
    pub model: String,
    /// The operator's key. Never printed, never logged.
    pub api_key: secrecy::SecretString,
}

impl fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
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
            Duration::from_millis(optional_parsed(
                &env,
                "BOMA_REQUEST_TIMEOUT_MS",
                30_000_u64,
            )?),
            Duration::from_millis(optional_parsed(
                &env,
                "BOMA_READINESS_TIMEOUT_MS",
                5_000_u64,
            )?),
            optional_parsed(&env, "BOMA_MAX_BODY_BYTES", 64 * 1024_usize)?,
            parse_ip_list(&env, "BOMA_TRUSTED_PROXIES")?,
        )?
        .with_session_ttl_seconds(optional_parsed(
            &env,
            "BOMA_SESSION_TTL_SECONDS",
            HttpConfig::DEFAULT_SESSION_TTL_SECONDS,
        )?)?
        .with_secure_cookies(optional_parsed(&env, "BOMA_SECURE_COOKIES", true)?);

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

        let computer = computer_from_pairs(
            env.iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )?;

        let capabilities = Capabilities {
            version: optional(&env, "BOMA_VERSION")
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()),
            runtime: optional(&env, "BOMA_RUNTIME").unwrap_or_else(|| "sandboxed".to_owned()),
            sandbox: computer.provider.name().to_owned(),
            integrations: parse_string_list(&env, "BOMA_INTEGRATIONS"),
            messaging: optional_parsed(&env, "BOMA_MESSAGING", false)?,
            email: optional(&env, "BOMA_EMAIL"),
            billing: optional(&env, "BOMA_BILLING"),
            jobs: optional(&env, "BOMA_JOBS").unwrap_or_else(|| "inline".to_owned()),
            realtime: optional(&env, "BOMA_REALTIME").unwrap_or_else(|| "postgres".to_owned()),
            degraded: optional_parsed(&env, "BOMA_DEGRADED", false)?,
            revision: optional(&env, "BOMA_REVISION"),
        };

        let web_origin =
            optional(&env, "BOMA_WEB_ORIGIN").unwrap_or_else(|| "http://127.0.0.1:5173".to_owned());
        let auth_capabilities = AuthCapabilities::password_only(
            Some(format!("{web_origin}/reset-password")),
            passkey_rp_id(&web_origin),
        );

        let provider = parse_provider(&env)?;
        Ok(Self {
            http,
            database,
            telemetry: TelemetrySettings {
                filter: optional(&env, "BOMA_LOG_FILTER").unwrap_or_else(|| "info".to_owned()),
                format,
            },
            capabilities,
            computer,
            auth_capabilities,
            provider,
            stdio_allowed: parse_string_list(&env, "MCP_STDIO_ALLOWED_COMMANDS"),
        })
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "http=[{}] database=[{:?}] logs=[filter={} format={:?}] version={}",
            self.http,
            self.database,
            self.telemetry.filter,
            self.telemetry.format,
            self.capabilities.version
        )
    }
}

/// The relying-party id passkeys are bound to: the host of the web origin.
fn passkey_rp_id(web_origin: &str) -> Option<String> {
    let without_scheme = web_origin
        .split_once("://")
        .map_or(web_origin, |(_, rest)| rest);
    let host = without_scheme.split('/').next().unwrap_or(without_scheme);
    let host = host.rsplit_once(':').map_or(host, |(name, _port)| name);
    (!host.is_empty()).then(|| host.to_owned())
}

/// Reads the bring-your-own-key provider, when it is configured.
fn parse_provider(env: &BTreeMap<String, String>) -> Result<Option<ProviderConfig>, ConfigError> {
    let endpoint = optional(env, "BOMA_PROVIDER_ENDPOINT");
    let model = optional(env, "BOMA_PROVIDER_MODEL");
    let api_key = optional(env, "BOMA_PROVIDER_API_KEY");
    match (endpoint, model, api_key) {
        (None, None, None) => Ok(None),
        (Some(endpoint), Some(model), Some(api_key)) => Ok(Some(ProviderConfig {
            endpoint,
            model,
            api_key: secrecy::SecretString::from(api_key),
        })),
        // The operator meant to configure one; refusing loudly beats answering with a script.
        _ => Err(ConfigError::Missing {
            name: "BOMA_PROVIDER_ENDPOINT/MODEL/API_KEY (set all three or none)",
        }),
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
        value
            .parse::<T>()
            .map_err(|_| ConfigError::Invalid { name, value })
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
            Config::from_pairs([("BOMA_DATABASE_URL", "  ")])
                .err()
                .map(|e| e.to_string()),
            Some("`BOMA_DATABASE_URL` is required".to_owned())
        );
    }

    #[test]
    fn req_015_the_provider_config_is_all_or_nothing() {
        // Nothing set: the deployment runs the scripted provider.
        let config = Config::from_pairs(minimum()).expect("the minimum configuration is valid");
        assert!(config.provider.is_none());

        // Half-set: the operator meant to configure a provider; the deployment must not start
        // on the scripted agent silently.
        let mut pairs = minimum();
        pairs.push(("BOMA_PROVIDER_ENDPOINT", "https://api.openai.com/v1"));
        pairs.push(("BOMA_PROVIDER_MODEL", "gpt-5"));
        assert!(matches!(
            Config::from_pairs(pairs),
            Err(ConfigError::Missing { .. })
        ));

        // All three: the provider is configured, and the key never prints.
        let mut pairs = minimum();
        pairs.push(("BOMA_PROVIDER_ENDPOINT", "https://api.openai.com/v1"));
        pairs.push(("BOMA_PROVIDER_MODEL", "gpt-5"));
        pairs.push(("BOMA_PROVIDER_API_KEY", "sk-test-key")); // secret-scan: allow (test fixture)
        let config = Config::from_pairs(pairs).expect("the provider configuration is valid");
        let provider = config
            .provider
            .as_ref()
            .expect("the provider is configured");
        assert_eq!(provider.endpoint, "https://api.openai.com/v1");
        assert_eq!(provider.model, "gpt-5");
        let rendered = format!("{provider:?}");
        assert!(!rendered.contains("sk-test-key"), "{rendered}");
    }

    #[test]
    fn req_053_config_defaults_to_loopback_with_bounded_limits() {
        let config = Config::from_pairs(minimum()).expect("the minimum configuration is valid");
        assert_eq!(config.http.bind().ip().to_string(), "127.0.0.1");
        assert_eq!(config.http.bind().port(), 3000);
        assert_eq!(config.http.body_limit_bytes(), 64 * 1024);
        assert!(
            config.http.trusted_proxies().is_empty(),
            "no proxy is trusted by default"
        );
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
