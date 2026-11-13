//! The computer backends a deployment may be configured with (REQ-027).
//!
//! Two providers exist today: `none` (computers are not available, the default) and `docker`
//! (the local supervisor). The selection refuses, naming the variable, anything else (REQ-027).

use std::collections::HashMap;

use bollard::models::{ContainerCreateBody, ExecConfig, HostConfig, VolumeCreateRequest};
use bollard::{API_DEFAULT_VERSION, Docker};

use crate::{Cdp, CdpError};
use boma_app::{
    AppError,
    ports::{ComputerHandle, ComputerRuntime, ComputerSpec},
};

/// The local Docker supervisor.
pub struct DockerComputerRuntime {
    docker: Docker,
    image: String,
    frame_command: Option<String>,
    command: Vec<String>,
    cdp_port: Option<u16>,
}

impl DockerComputerRuntime {
    /// Builds a runtime against `docker_host` (a unix socket path, the `unix://` scheme optional).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the client cannot be constructed for that address.
    pub fn new(
        docker_host: &str,
        image: impl Into<String>,
        frame_command: Option<String>,
    ) -> Result<Self, AppError> {
        let path = docker_host.trim_start_matches("unix://").to_owned();
        let docker =
            Docker::connect_with_socket(&path, 60, API_DEFAULT_VERSION).map_err(|error| {
                tracing::warn!(%error, %path, "the docker socket is not acceptable to the client");
                AppError::Unavailable
            })?;
        Ok(Self {
            docker,
            image: image.into(),
            frame_command,
            command: vec!["sleep".to_owned(), "infinity".to_owned()],
            cdp_port: None,
        })
    }

    /// The image new computers boot.
    #[must_use]
    pub fn image(&self) -> &str {
        &self.image
    }

    /// The command run inside the computer to emit a frame, when the image carries one.
    #[must_use]
    pub fn frame_command(&self) -> Option<&str> {
        self.frame_command.as_deref()
    }

    /// Overrides the command new computers boot with.
    #[must_use]
    pub fn with_command(mut self, command: Vec<String>) -> Self {
        if !command.is_empty() {
            self.command = command;
        }
        self
    }

    /// Publishes the CDP port `port` of new computers on a random host port, proxied to 127.0.0.1.
    #[must_use]
    pub fn with_published_cdp_port(mut self, port: u16) -> Self {
        self.cdp_port = Some(port);
        self
    }
}

#[async_trait::async_trait]
impl ComputerRuntime for DockerComputerRuntime {
    fn provider(&self) -> &'static str {
        "docker"
    }

    fn frame_command(&self) -> Option<&str> {
        self.frame_command.as_deref()
    }

    async fn observe(&self, external_id: &str) -> Result<Vec<u8>, AppError> {
        self.cdp(external_id)
            .await?
            .screenshot()
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the CDP screenshot failed");
                AppError::Unavailable
            })
    }

    async fn navigate(&self, external_id: &str, url: &str) -> Result<(), AppError> {
        self.cdp(external_id)
            .await?
            .navigate(url)
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the CDP navigation failed");
                AppError::Unavailable
            })
    }

    async fn evaluate(
        &self,
        external_id: &str,
        expression: &str,
    ) -> Result<serde_json::Value, AppError> {
        self.cdp(external_id)
            .await?
            .evaluate(expression)
            .await
            .map_err(|error| {
                if let CdpError::Exception(_) = &error {
                    tracing::debug!(%external_id, %error, "the evaluated expression raised");
                    AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the expression raised".to_owned(),
                    ))
                } else {
                    tracing::warn!(%external_id, %error, "the CDP evaluation failed");
                    AppError::Unavailable
                }
            })
    }

    async fn click(&self, external_id: &str, x: f64, y: f64) -> Result<(), AppError> {
        self.cdp(external_id)
            .await?
            .click(x, y)
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the CDP click failed");
                AppError::Unavailable
            })
    }

    async fn type_text(&self, external_id: &str, text: &str) -> Result<(), AppError> {
        self.cdp(external_id)
            .await?
            .type_text(text)
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the CDP typing failed");
                AppError::Unavailable
            })
    }

    async fn scroll(&self, external_id: &str, delta_y: f64) -> Result<(), AppError> {
        self.cdp(external_id)
            .await?
            .scroll(0.0, delta_y)
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the CDP scroll failed");
                AppError::Unavailable
            })
    }

    async fn dom_snapshot(&self, external_id: &str) -> Result<String, AppError> {
        let html = self
            .cdp(external_id)
            .await?
            .evaluate("document.documentElement.outerHTML")
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the CDP DOM snapshot failed");
                AppError::Unavailable
            })?;
        html.as_str().map(str::to_owned).ok_or_else(|| {
            tracing::warn!(%external_id, "the DOM snapshot returned no HTML");
            AppError::Unavailable
        })
    }

    async fn ping(&self) -> Result<(), AppError> {
        self.docker.ping().await.map(|_| ()).map_err(|error| {
            tracing::warn!(%error, "the docker daemon did not answer the ping");
            AppError::Unavailable
        })
    }

    #[allow(clippy::too_many_lines)] // the boot flow is linear; splitting hides the failure table
    async fn ensure(&self, spec: &ComputerSpec) -> Result<ComputerHandle, AppError> {
        let name = format!("boma-computer-{}", spec.computer_id);
        let home_id = format!("boma-home-{}", spec.computer_id);

        // A computer this deployment already booted is reused, never booted twice: the name is
        // stable per computer id.
        if let Ok(inspection) = self
            .docker
            .inspect_container(
                &name,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
        {
            let running = inspection.state.and_then(|state| state.running) == Some(true);
            let external_id = inspection.id.unwrap_or_else(|| name.clone());
            if running {
                return Ok(ComputerHandle {
                    external_id,
                    home_id,
                    started: false,
                });
            }
            self.start(&external_id, spec).await?;
            return Ok(ComputerHandle {
                external_id,
                home_id,
                started: true,
            });
        }

        // The home survives the container: a volume per computer, never recycled.
        let volume = VolumeCreateRequest {
            name: Some(home_id.clone()),
            driver: None,
            driver_opts: None,
            labels: Some(HashMap::from([(
                "boma.computer".to_owned(),
                spec.computer_id.clone(),
            )])),
            ..Default::default()
        };
        self.docker.create_volume(volume).await.map_err(|error| {
            tracing::warn!(%error, %home_id, "the computer's home volume could not be ensured");
            AppError::Unavailable
        })?;

        // A published binding only takes effect for a port the container also exposes: the CLI
        // sends both, and a daemon that insists on `ExposedPorts` (as older ones do) would
        // otherwise accept the container and publish nothing.
        let exposed_ports = self.cdp_port.map(|port| vec![format!("{port}/tcp")]);
        let port_bindings = self.cdp_port.map(|port| {
            let mut bindings = HashMap::new();
            bindings.insert(
                format!("{port}/tcp"),
                Some(vec![bollard::models::PortBinding {
                    host_ip: Some("127.0.0.1".to_owned()),
                    // An empty host port asks the daemon for a free one.
                    host_port: Some(String::new()),
                }]),
            );
            bindings
        });
        let body = ContainerCreateBody {
            image: Some(self.image.clone()),
            cmd: Some(self.command.clone()),
            exposed_ports,
            labels: Some(HashMap::from([(
                "boma.computer".to_owned(),
                spec.computer_id.clone(),
            )])),
            host_config: Some(HostConfig {
                port_bindings,
                memory: Some(i64::try_from(spec.memory_limit_mb).unwrap_or(i64::MAX) * 1024 * 1024),
                // cpu_limit is bounded to [0.25, 4], so this value is exact in f64 and i64.
                #[allow(clippy::cast_possible_truncation)]
                nano_cpus: Some((spec.cpu_limit * 1_000_000_000.0).round() as i64),
                binds: Some(vec![format!("{home_id}:/home/bot")]),
                shm_size: Some(512 * 1024 * 1024),
                ..Default::default()
            }),
            ..Default::default()
        };
        match self
            .docker
            .create_container(
                Some(bollard::query_parameters::CreateContainerOptions {
                    name: Some(name.clone()),
                    platform: String::new(),
                }),
                body,
            )
            .await
        {
            Ok(created) => {
                self.start(&created.id, spec).await?;
                Ok(ComputerHandle {
                    external_id: created.id,
                    home_id,
                    started: true,
                })
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 409, ..
            }) => {
                // Raced with another ensure: the container exists already. Re-inspect and reuse.
                let inspection = self
                    .docker
                    .inspect_container(
                        &name,
                        None::<bollard::query_parameters::InspectContainerOptions>,
                    )
                    .await
                    .map_err(|error| {
                        tracing::warn!(%error, %name, "the computer could not be re-inspected");
                        AppError::Unavailable
                    })?;
                let external_id = inspection.id.unwrap_or_else(|| name.clone());
                let running = inspection.state.and_then(|state| state.running) == Some(true);
                if !running {
                    self.start(&external_id, spec).await?;
                }
                Ok(ComputerHandle {
                    external_id,
                    home_id,
                    started: !running,
                })
            }
            Err(error) => {
                tracing::warn!(%error, %name, "the computer could not be created");
                Err(AppError::Unavailable)
            }
        }
    }

    async fn exec(
        &self,
        external_id: &str,
        command: &str,
        output_cap: usize,
        env: &[(String, String)],
    ) -> Result<String, AppError> {
        let exec = self
            .docker
            .create_exec(
                external_id,
                ExecConfig {
                    cmd: Some(vec!["sh".to_owned(), "-c".to_owned(), command.to_owned()]),
                    attach_stdout: Some(true),
                    attach_stdin: Some(false),
                    attach_stderr: Some(false),
                    env: Some(
                        env.iter()
                            .map(|(key, value)| format!("{key}={value}"))
                            .collect(),
                    ),
                    ..Default::default()
                },
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the command could not be prepared");
                AppError::Unavailable
            })?;
        let output = self
            .docker
            .start_exec(
                &exec.id,
                Some(bollard::exec::StartExecOptions {
                    detach: false,
                    tty: false,
                    output_capacity: None,
                }),
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the command could not run");
                AppError::Unavailable
            })?;
        let bollard::exec::StartExecResults::Attached { mut output, .. } = output else {
            tracing::warn!(%external_id, "the command detached before producing output");
            return Err(AppError::Unavailable);
        };
        let mut bytes = Vec::new();
        while let Some(item) = futures_util::StreamExt::next(&mut output).await {
            let item = item.map_err(|error| {
                tracing::warn!(%error, %external_id, "the output stream died");
                AppError::Unavailable
            })?;
            if let bollard::container::LogOutput::StdOut { message } = item {
                let remaining = output_cap.saturating_sub(bytes.len());
                if remaining > 0 {
                    bytes.extend_from_slice(&message[..message.len().min(remaining)]);
                }
            }
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn screenshot(&self, external_id: &str) -> Result<Vec<u8>, AppError> {
        // A browser computer carries no frame command: its CDP endpoint captures the same frame.
        if self.frame_command.is_none() && self.cdp_port.is_some() {
            return self.observe(external_id).await;
        }
        let Some(frame_command) = self.frame_command.as_deref() else {
            tracing::warn!("no frame command is configured for this image");
            return Err(AppError::Unavailable);
        };
        // The frame command prints the frame bytes on stdout; the image is trusted with its own
        // rendering, bytes are delivered verbatim.
        let exec = self
            .docker
            .create_exec(
                external_id,
                ExecConfig {
                    cmd: Some(vec![
                        "sh".to_owned(),
                        "-c".to_owned(),
                        frame_command.to_owned(),
                    ]),
                    attach_stdout: Some(true),
                    attach_stdin: Some(false),
                    attach_stderr: Some(false),
                    ..Default::default()
                },
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the frame command could not be prepared");
                AppError::Unavailable
            })?;
        let output = self
            .docker
            .start_exec(
                &exec.id,
                Some(bollard::exec::StartExecOptions {
                    detach: false,
                    tty: false,
                    output_capacity: None,
                }),
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the frame command could not run");
                AppError::Unavailable
            })?;
        let bollard::exec::StartExecResults::Attached { mut output, .. } = output else {
            tracing::warn!(%external_id, "the frame command detached before producing a frame");
            return Err(AppError::Unavailable);
        };
        let mut bytes = Vec::new();
        while let Some(item) = futures_util::StreamExt::next(&mut output).await {
            let message = item.map_err(|error| {
                tracing::warn!(%error, %external_id, "the frame stream died");
                AppError::Unavailable
            })?;
            if let bollard::container::LogOutput::StdOut { message } = message {
                bytes.extend_from_slice(&message);
            }
        }
        Ok(bytes)
    }

    async fn stop(&self, external_id: &str) -> Result<(), AppError> {
        match self
            .docker
            .stop_container(
                external_id,
                Some(bollard::query_parameters::StopContainerOptions {
                    signal: None,
                    t: Some(10),
                }),
            )
            .await
        {
            // A clean stop, and a container already gone: the home survives either way.
            Ok(())
            | Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(()),
            Err(error) => {
                tracing::warn!(%error, %external_id, "the computer could not be stopped");
                Err(AppError::Unavailable)
            }
        }
    }
}

impl DockerComputerRuntime {
    /// Opens a CDP session to the computer, resolving the published host port.
    async fn cdp(&self, external_id: &str) -> Result<Cdp, AppError> {
        let Some(cdp_port) = self.cdp_port else {
            return Err(AppError::Unavailable);
        };
        let inspection = self
            .docker
            .inspect_container(
                external_id,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the computer inspection failed");
                AppError::Unavailable
            })?;
        let key = format!("{cdp_port}/tcp");
        let host_port = inspection
            .network_settings
            .and_then(|settings| settings.ports)
            .and_then(|ports| ports.get(&key).cloned())
            .flatten()
            .and_then(|bindings| bindings.into_iter().next())
            .and_then(|binding| binding.host_port)
            .and_then(|port| port.parse::<u16>().ok())
            .ok_or_else(|| {
                tracing::warn!(%external_id, %cdp_port, "no CDP port is published for the computer");
                AppError::Unavailable
            })?;
        Cdp::connect(host_port).await.map_err(|error| {
            tracing::warn!(%error, %external_id, "the CDP websocket did not open");
            AppError::Unavailable
        })
    }

    /// Starts an existing computer for `spec`'s memory mapping.
    async fn start(&self, external_id: &str, _spec: &ComputerSpec) -> Result<(), AppError> {
        self.docker
            .start_container(
                external_id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, %external_id, "the computer could not be started");
                AppError::Unavailable
            })
    }
}

/// The `none` backend: computers are not available in this deployment (REQ-027).
#[derive(Debug, Default)]
pub struct NoneComputerRuntime;

#[async_trait::async_trait]
impl ComputerRuntime for NoneComputerRuntime {
    fn provider(&self) -> &'static str {
        "none"
    }

    fn frame_command(&self) -> Option<&str> {
        None
    }

    async fn ping(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn ensure(&self, _spec: &ComputerSpec) -> Result<ComputerHandle, AppError> {
        // The wire maps this to COMPUTERS_UNAVAILABLE: the deployment answered honestly.
        Err(AppError::Unavailable)
    }

    async fn stop(&self, _external_id: &str) -> Result<(), AppError> {
        Ok(())
    }

    async fn screenshot(&self, _external_id: &str) -> Result<Vec<u8>, AppError> {
        // A deployment without computers answers honestly, not with fake pixels.
        Err(AppError::Unavailable)
    }

    async fn exec(
        &self,
        _external_id: &str,
        _command: &str,
        _output_cap: usize,
        _env: &[(String, String)],
    ) -> Result<String, AppError> {
        Err(AppError::Unavailable)
    }

    async fn observe(&self, _external_id: &str) -> Result<Vec<u8>, AppError> {
        Err(AppError::Unavailable)
    }

    async fn navigate(&self, _external_id: &str, _url: &str) -> Result<(), AppError> {
        Err(AppError::Unavailable)
    }

    async fn evaluate(
        &self,
        _external_id: &str,
        _expression: &str,
    ) -> Result<serde_json::Value, AppError> {
        Err(AppError::Unavailable)
    }

    async fn click(&self, _external_id: &str, _x: f64, _y: f64) -> Result<(), AppError> {
        Err(AppError::Unavailable)
    }

    async fn type_text(&self, _external_id: &str, _text: &str) -> Result<(), AppError> {
        Err(AppError::Unavailable)
    }

    async fn scroll(&self, _external_id: &str, _delta_y: f64) -> Result<(), AppError> {
        Err(AppError::Unavailable)
    }

    async fn dom_snapshot(&self, _external_id: &str) -> Result<String, AppError> {
        Err(AppError::Unavailable)
    }
}
