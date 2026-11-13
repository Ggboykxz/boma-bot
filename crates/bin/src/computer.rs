//! The computer selection, read once at startup (REQ-027).
//!
//! `SANDBOX_PROVIDER` picks the backend this deployment runs computers on. The default is
//! `none`: an API that serves a space without computers is a valid, working deployment, and a
//! misconfigured one refuses to start rather than degrading silently. Fail-closed means the
//! error names the variable the operator has to fix.

use std::{collections::BTreeMap, fmt, path::Path};

use boma_app::{AppError, ports::ComputerRuntime};
use boma_infra::{DockerComputerRuntime, NoneComputerRuntime};

/// Which backend this deployment runs computers on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputerProvider {
    /// Computers are not available here.
    None,
    /// The local Docker supervisor.
    Docker,
}

impl ComputerProvider {
    /// The name the `/ready` report and logs carry.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Docker => "docker",
        }
    }
}

/// A computer selection that cannot start the process.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComputerConfigError {
    /// A required variable is absent.
    #[error("`{name}` is required")]
    Missing {
        /// Variable name.
        name: &'static str,
    },
    /// A variable is present but not acceptable.
    #[error("`{name}` is not valid: {value}")]
    Invalid {
        /// Variable name.
        name: &'static str,
        /// The value the operator supplied, already trimmed.
        value: String,
    },
    /// The provider this build cannot be configured with was requested.
    #[error(
        "`SANDBOX_PROVIDER` is not supported in this build: `{value}` (supported: `docker`, `none`)"
    )]
    Unsupported {
        /// The requested value.
        value: String,
    },
}

/// The computer selection, already validated.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputerConfig {
    /// Which backend this deployment runs computers on.
    pub provider: ComputerProvider,
    /// Where the Docker client reaches the daemon (a `unix://` socket).
    pub docker_host: String,
    /// The image new computers boot.
    pub image: String,
    /// The command run inside the container to emit a frame; `None` means frames are refused,
    /// never faked.
    pub frame_command: Option<String>,
    /// The command the container boots with; `None` keeps it alive (`sleep infinity`).
    ///
    /// A browser computer sets this so its Chromium starts with `--remote-debugging-port`.
    pub command: Option<String>,
    /// The CDP port the computer's browser listens on inside the container, when it runs one.
    ///
    /// `None` means the computer has no browser and observe-and-act answers `Unavailable`.
    pub cdp_port: Option<u16>,
    /// How long a computer may sit idle before it is released.
    pub idle_minutes: u64,
    /// RAM cap for one computer, in megabytes.
    pub memory_limit_mb: u64,
    /// CPU cap for one computer, in CPUs.
    pub cpu_limit: f64,
}

impl ComputerConfig {
    /// The runtime this configuration selects, ready to attach.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the provider is configured but cannot be reached.
    pub async fn provision(&self) -> Result<std::sync::Arc<dyn ComputerRuntime>, AppError> {
        match self.provider {
            ComputerProvider::None => Ok(std::sync::Arc::new(NoneComputerRuntime)),
            ComputerProvider::Docker => {
                let mut runtime = DockerComputerRuntime::new(
                    &self.docker_host,
                    self.image.clone(),
                    self.frame_command.clone(),
                )?;
                if let Some(command) = &self.command {
                    runtime = runtime
                        .with_command(command.split_whitespace().map(str::to_owned).collect());
                }
                if let Some(port) = self.cdp_port {
                    runtime = runtime.with_published_cdp_port(port);
                }
                // A cold daemon can take a moment to answer the first ping. The retry is bounded:
                // a daemon that never comes up still fails closed, it just stops depending on luck.
                let mut last = None;
                for attempt in 1..=3 {
                    match runtime.ping().await {
                        Ok(()) => return Ok(std::sync::Arc::new(runtime)),
                        Err(error) => {
                            last = Some(error);
                            tracing::warn!(attempt, "the docker daemon did not answer the ping");
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                }
                Err(last.unwrap_or(boma_app::AppError::Unavailable))
            }
        }
    }
}

impl fmt::Display for ComputerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "provider={} docker_host=[{}] image={} frame_command={} command={} cdp_port={} idle={}m memory={}MB cpu={}",
            self.provider.name(),
            self.docker_host,
            self.image,
            self.frame_command.as_deref().unwrap_or("none"),
            self.command.as_deref().unwrap_or("default"),
            self.cdp_port
                .map_or_else(|| "none".to_owned(), |port| port.to_string()),
            self.idle_minutes,
            self.memory_limit_mb,
            self.cpu_limit,
        )
    }
}

/// Reads the computer selection from an arbitrary set of pairs.
///
/// # Errors
///
/// Returns [`ComputerConfigError`] when the selection names a provider this build cannot be
/// configured with, or when a limit is outside what the deployment allows.
pub fn from_pairs<I, K, V>(pairs: I) -> Result<ComputerConfig, ComputerConfigError>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let env: BTreeMap<String, String> = pairs
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect();

    let provider = match optional(&env, "SANDBOX_PROVIDER").as_deref() {
        None | Some("none") => ComputerProvider::None,
        Some("docker") => ComputerProvider::Docker,
        Some("novita") => {
            if optional(&env, "NOVITA_API_KEY").is_none() {
                return Err(ComputerConfigError::Missing {
                    name: "NOVITA_API_KEY",
                });
            }
            return Err(ComputerConfigError::Unsupported {
                value: "novita".to_owned(),
            });
        }
        Some(other) => {
            return Err(ComputerConfigError::Unsupported {
                value: other.to_owned(),
            });
        }
    };

    let docker_host =
        optional(&env, "DOCKER_HOST").unwrap_or_else(|| "unix:///var/run/docker.sock".to_owned());
    let socket = docker_host.trim_start_matches("unix://");
    if !Path::new(socket).is_absolute() {
        return Err(ComputerConfigError::Invalid {
            name: "DOCKER_HOST",
            value: docker_host,
        });
    }

    let image = optional(&env, "BOMA_COMPUTER_IMAGE").unwrap_or_else(|| "alpine:3.21".to_owned());
    let frame_command = optional(&env, "BOMA_COMPUTER_FRAME_CMD");
    // The command is a whitespace-separated argv; the flags a browser image needs carry no spaces.
    let command = optional(&env, "BOMA_COMPUTER_CMD");
    let cdp_port =
        optional(&env, "BOMA_COMPUTER_CDP_PORT")
            .map(|value| {
                value.parse::<u16>().ok().filter(|port| *port > 0).ok_or(
                    ComputerConfigError::Invalid {
                        name: "BOMA_COMPUTER_CDP_PORT",
                        value: value.clone(),
                    },
                )
            })
            .transpose()?;

    let idle_minutes = optional_parsed(&env, "BOMA_COMPUTER_IDLE_MINUTES", 10_u64)?;
    if !(1..=1_440).contains(&idle_minutes) {
        return Err(ComputerConfigError::Invalid {
            name: "BOMA_COMPUTER_IDLE_MINUTES",
            value: idle_minutes.to_string(),
        });
    }

    let memory_limit_mb = optional_parsed(&env, "BOMA_COMPUTER_MEMORY_LIMIT_MB", 512_u64)?;
    if !(64..=4_096).contains(&memory_limit_mb) {
        return Err(ComputerConfigError::Invalid {
            name: "BOMA_COMPUTER_MEMORY_LIMIT_MB",
            value: memory_limit_mb.to_string(),
        });
    }

    let cpu_limit = optional_parsed(&env, "BOMA_COMPUTER_CPU_LIMIT", 1.0_f64)?;
    if !(0.25..=4.0).contains(&cpu_limit) {
        return Err(ComputerConfigError::Invalid {
            name: "BOMA_COMPUTER_CPU_LIMIT",
            value: cpu_limit.to_string(),
        });
    }

    Ok(ComputerConfig {
        provider,
        docker_host,
        image,
        frame_command,
        command,
        cdp_port,
        idle_minutes,
        memory_limit_mb,
        cpu_limit,
    })
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
) -> Result<T, ComputerConfigError>
where
    T: std::str::FromStr,
{
    optional(env, name).map_or(Ok(default), |value| {
        value
            .parse::<T>()
            .map_err(|_| ComputerConfigError::Invalid { name, value })
    })
}

#[cfg(test)]
mod tests {
    use super::{ComputerConfigError, ComputerProvider, from_pairs};

    #[test]
    fn req_027_no_provider_is_a_valid_none_deployment() {
        let config = from_pairs(Vec::<(&str, &str)>::new()).expect("the empty selection is valid");
        assert_eq!(config.provider, ComputerProvider::None);
        assert_eq!(config.provider.name(), "none");
        assert_eq!(config.docker_host, "unix:///var/run/docker.sock");
        assert_eq!(config.image, "alpine:3.21");
        assert_eq!(config.idle_minutes, 10);
        assert_eq!(config.memory_limit_mb, 512);
        assert!((config.cpu_limit - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn req_027_an_unknown_provider_refuses_at_startup() {
        let error = from_pairs([("SANDBOX_PROVIDER", "e2b")]).expect_err("not configured");
        assert_eq!(
            error.to_string(),
            "`SANDBOX_PROVIDER` is not supported in this build: `e2b` (supported: `docker`, `none`)"
        );
    }

    #[test]
    fn req_027_novita_without_its_key_is_refused_naming_the_variable() {
        let error = from_pairs([("SANDBOX_PROVIDER", "novita")]).expect_err("the key is required");
        assert_eq!(error.to_string(), "`NOVITA_API_KEY` is required");
        assert!(matches!(
            error,
            ComputerConfigError::Missing {
                name: "NOVITA_API_KEY"
            }
        ));
    }

    #[test]
    fn req_027_novita_with_its_key_is_still_refused_by_this_build() {
        let error = from_pairs([
            ("SANDBOX_PROVIDER", "novita"),
            ("NOVITA_API_KEY", "sk-test"),
        ])
        .expect_err("the build does not support it");
        assert!(matches!(error, ComputerConfigError::Unsupported { .. }));
    }

    #[test]
    fn req_027_the_docker_host_accepts_a_socket_path_and_nothing_else() {
        let error = from_pairs([
            ("SANDBOX_PROVIDER", "docker"),
            ("DOCKER_HOST", "http://docker.local:2375"),
        ])
        .expect_err("http is not a socket path");
        assert!(matches!(
            error,
            ComputerConfigError::Invalid {
                name: "DOCKER_HOST",
                ..
            }
        ));
        let config = from_pairs([
            ("SANDBOX_PROVIDER", "docker"),
            ("DOCKER_HOST", "unix:///custom.sock"),
        ])
        .expect("a unix socket path is fine");
        assert_eq!(config.docker_host, "unix:///custom.sock");
    }

    #[test]
    fn req_027_limits_are_bounded() {
        for (name, value) in [
            ("BOMA_COMPUTER_IDLE_MINUTES", "0"),
            ("BOMA_COMPUTER_IDLE_MINUTES", "1441"),
            ("BOMA_COMPUTER_MEMORY_LIMIT_MB", "32"),
            ("BOMA_COMPUTER_MEMORY_LIMIT_MB", "8192"),
            ("BOMA_COMPUTER_CPU_LIMIT", "9"),
            ("BOMA_COMPUTER_CPU_LIMIT", "0.1"),
        ] {
            let error = from_pairs([(name, value)]).expect_err("the limit is refused");
            assert!(matches!(error, ComputerConfigError::Invalid { name: n, .. } if n == name));
        }
    }

    #[test]
    fn req_028_a_browser_computer_configures_its_command_and_cdp_port() {
        let config = from_pairs([
            ("SANDBOX_PROVIDER", "docker"),
            ("BOMA_COMPUTER_IMAGE", "zenika/alpine-chrome:latest"),
            (
                "BOMA_COMPUTER_CMD",
                "--no-sandbox --remote-debugging-port=9222 --remote-debugging-address=0.0.0.0 about:blank",
            ),
            ("BOMA_COMPUTER_CDP_PORT", "9222"),
        ])
        .expect("the browser selection parses");
        assert_eq!(config.image, "zenika/alpine-chrome:latest");
        assert_eq!(
            config.command.as_deref(),
            Some(
                "--no-sandbox --remote-debugging-port=9222 --remote-debugging-address=0.0.0.0 about:blank"
            )
        );
        assert_eq!(config.cdp_port, Some(9222));
    }

    #[test]
    fn req_028_a_cdp_port_outside_the_range_is_refused() {
        for value in ["0", "65536", "nine"] {
            let error =
                from_pairs([("BOMA_COMPUTER_CDP_PORT", value)]).expect_err("the port is refused");
            assert!(matches!(
                error,
                ComputerConfigError::Invalid {
                    name: "BOMA_COMPUTER_CDP_PORT",
                    ..
                }
            ));
        }
    }

    #[tokio::test]
    async fn req_027_a_docker_selection_pays_for_itself_against_an_absent_socket() {
        let config = from_pairs([
            ("SANDBOX_PROVIDER", "docker"),
            ("DOCKER_HOST", "unix:///boma-no-such-docker.sock"),
        ])
        .expect("the selection parses");
        let Err(error) = config.provision().await else {
            panic!("the daemon must answer");
        };
        assert!(matches!(error, boma_app::AppError::Unavailable));
    }

    #[tokio::test]
    async fn req_027_a_docker_selection_provisions_against_the_daemon() {
        let host = std::env::var("DOCKER_HOST")
            .unwrap_or_else(|_| "unix:///var/run/docker.sock".to_owned());
        let config = from_pairs([
            ("SANDBOX_PROVIDER", "docker"),
            ("DOCKER_HOST", host.as_str()),
        ])
        .expect("the selection parses");
        let runtime = config.provision().await.expect("the daemon answers");
        assert_eq!(runtime.provider(), "docker");
    }
}
