//! The state every handler shares.

use std::{fmt, sync::Arc};

use boma_app::{Capabilities, Readiness};

use crate::config::HttpConfig;

/// Everything the HTTP layer needs, built once by the binary.
///
/// Cloning this is cheap and shares the probes: a handler cannot mutate the deployment's
/// capabilities or its deadlines.
#[derive(Clone)]
pub struct AppState {
    readiness: Arc<Readiness>,
    capabilities: Arc<Capabilities>,
    http: Arc<HttpConfig>,
}

impl AppState {
    /// Assembles the state from already-validated parts.
    #[must_use]
    pub fn new(readiness: Readiness, capabilities: Capabilities, http: HttpConfig) -> Self {
        Self {
            readiness: Arc::new(readiness),
            capabilities: Arc::new(capabilities),
            http: Arc::new(http),
        }
    }

    /// The readiness probes behind `/ready`.
    #[must_use]
    pub fn readiness(&self) -> &Readiness {
        &self.readiness
    }

    /// What this deployment reports it can do.
    #[must_use]
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// The HTTP settings the router was built with.
    #[must_use]
    pub fn http(&self) -> &HttpConfig {
        &self.http
    }
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState")
            .field("probes", &self.readiness.len())
            .field("version", &self.capabilities.version)
            .field("bind", &self.http.bind())
            .finish_non_exhaustive()
    }
}
