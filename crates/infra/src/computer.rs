//! The computer backends a deployment may be configured with (REQ-027).
//!
//! Two providers exist today: `none` (computers are not available, the default) and `docker`
//! (the local supervisor). The selection refuses, naming the variable, anything else (REQ-027).

use std::collections::HashMap;

use bollard::models::{ContainerCreateBody, ExecConfig, HostConfig, VolumeCreateRequest};
use bollard::{API_DEFAULT_VERSION, Docker};

use boma_app::{
    AppError,
    ports::{ComputerHandle, ComputerRuntime, ComputerSpec},
};

/// The local Docker supervisor.
pub struct DockerComputerRuntime {
    docker: Docker,
    image: String,
    frame_command: Option<String>,
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
}

#[async_trait::async_trait]
impl ComputerRuntime for DockerComputerRuntime {
    fn provider(&self) -> &'static str {
        "docker"
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

        let body = ContainerCreateBody {
            image: Some(self.image.clone()),
            cmd: Some(vec!["sleep".to_owned(), "infinity".to_owned()]),
            labels: Some(HashMap::from([(
                "boma.computer".to_owned(),
                spec.computer_id.clone(),
            )])),
            host_config: Some(HostConfig {
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

    async fn screenshot(&self, external_id: &str) -> Result<Vec<u8>, AppError> {
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
}
