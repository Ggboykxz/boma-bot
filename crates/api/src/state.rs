//! The state every handler shares.

use std::{fmt, sync::Arc};

use boma_app::{App, Capabilities, Readiness};

use crate::{auth::AuthContext, authcap::AuthCapabilities, config::HttpConfig, error::ApiError};

/// Everything the HTTP layer needs, built once by the binary.
///
/// Cloning this is cheap and shares the probes: a handler cannot mutate the deployment's
/// capabilities or its deadlines.
#[derive(Clone)]
pub struct AppState {
    readiness: Arc<Readiness>,
    capabilities: Arc<Capabilities>,
    auth_capabilities: Arc<AuthCapabilities>,
    http: Arc<HttpConfig>,
    app: Arc<App>,
}

impl AppState {
    /// Assembles the state from already-validated parts.
    #[must_use]
    pub fn new(
        readiness: Readiness,
        capabilities: Capabilities,
        auth_capabilities: AuthCapabilities,
        http: HttpConfig,
        app: App,
    ) -> Self {
        Self {
            readiness: Arc::new(readiness),
            capabilities: Arc::new(capabilities),
            auth_capabilities: Arc::new(auth_capabilities),
            http: Arc::new(http),
            app: Arc::new(app),
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

    /// Which sign-in methods this deployment offers.
    #[must_use]
    pub fn auth_capabilities(&self) -> &AuthCapabilities {
        &self.auth_capabilities
    }

    /// The HTTP settings the router was built with.
    #[must_use]
    pub fn http(&self) -> &HttpConfig {
        &self.http
    }

    /// How long an invitation stays valid, in seconds.
    #[must_use]
    pub const fn invitation_ttl_seconds(&self) -> i64 {
        HttpConfig::DEFAULT_INVITATION_TTL_SECONDS
    }

    /// The application the procedures run against.
    #[must_use]
    pub fn app(&self) -> &App {
        &self.app
    }

    /// Resolves the caller of a request.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::Unauthorized`] when the request carries no valid session, and
    /// [`ApiError::NotFound`] when the caller belongs to no space.
    pub async fn authenticate(
        &self,
        headers: &axum::http::HeaderMap,
    ) -> Result<AuthContext, ApiError> {
        crate::auth::authenticate(&self.app, headers, self.http.session_ttl_seconds()).await
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
