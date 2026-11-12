//! The computer backends a deployment may be configured with (REQ-027).
//!
//! Two providers exist today: `none` (computers are not available, the default) and `docker`
//! (the local supervisor). The selection refuses, naming the variable, anything else (REQ-027).

use bollard::{API_DEFAULT_VERSION, Docker};

use boma_app::{AppError, ports::ComputerRuntime};

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
            Docker::connect_with_socket(&path, 10, API_DEFAULT_VERSION).map_err(|error| {
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
}
