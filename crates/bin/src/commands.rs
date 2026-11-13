//! The jobs the binary can run.

use std::{sync::Arc, time::Duration};

use anyhow::Context;
use boma_api::{AppState, router};
use boma_app::{App, Readiness, Services, Stores, Worker};
use boma_infra::{
    Argon2idHasher, BroadcastPublisher, IdGenerator, Postgres, PostgresAccountStore,
    PostgresAuditStore, PostgresBotStore, PostgresCardStore, PostgresComputerStore,
    PostgresConnectorStore, PostgresEffectStore, PostgresEventStore, PostgresInvitationStore,
    PostgresMcpServerStore, PostgresMemoryStore, PostgresMessageStore,
    PostgresPendingToolCallStore, PostgresPluginInstallStore, PostgresPluginSubmissionStore,
    PostgresRateLimitStore, PostgresReactionStore, PostgresReadStateStore, PostgresRosterStore,
    PostgresRoutineStore, PostgresRunStore, PostgresScreenCapabilityStore, PostgresSecretStore,
    PostgresSessionStore, PostgresSkillStore, PostgresSpaceStore, PostgresThreadStore,
    PostgresToolStore, PostgresUserStore, ScriptedProvider, SystemClock, TokenGenerator,
};
use tokio::net::TcpListener;

use boma_app::ports::ComputerRuntime;

use crate::config::Config;

/// How long the worker waits between two polls of the queue.
const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Serves the HTTP API until a shutdown signal arrives.
///
/// The pool is built lazily so a database that is not up yet produces `503` on `/ready` instead of
/// a crash loop: the orchestrator restarts the process, but it can also simply start later.
///
/// # Errors
///
/// Returns an error when the pool cannot be built, the address cannot be bound, or the server stops
/// with an error.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let database =
        Postgres::connect_lazy(&config.database).context("the database pool cannot be built")?;
    let readiness = Readiness::new(vec![database.readiness(config.http.readiness_timeout())]);
    let computer = Some(config.computer.provision().await.with_context(|| {
        format!(
            "SANDBOX_PROVIDER={} is set but does not answer: {}",
            config.computer.provider.name(),
            config.computer,
        )
    })?);
    let app = build_app(&database, computer)?;
    let state = AppState::new(
        readiness,
        config.capabilities.clone(),
        config.auth_capabilities.clone(),
        config.http.clone(),
        app,
    );

    let listener = TcpListener::bind(config.http.bind())
        .await
        .with_context(|| format!("cannot bind {}", config.http.bind()))?;
    tracing::info!(
        bind = %config.http.bind(),
        version = %config.capabilities.version,
        "boma is listening"
    );

    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("the HTTP server stopped with an error")
}

/// The vault the deployment seals bot secrets with, read from `BOMA_MASTER_KEY` (REQ-046).
fn secrets_vault() -> anyhow::Result<boma_infra::SecretBox> {
    boma_infra::SecretBox::from_env(&std::env::vars().collect())
        .context("the secret vault could not be built")
}

fn build_app(
    database: &Postgres,
    computer: Option<Arc<dyn ComputerRuntime>>,
) -> anyhow::Result<App> {
    let pool = database.pool().clone();
    let publisher = Arc::new(BroadcastPublisher::default());
    let mcp_transport = Arc::new(
        boma_infra::HttpMcpTransport::new().context("the MCP transport could not be built")?,
    );
    let connector_runner = Arc::new(
        boma_infra::HttpConnectorRunner::new()
            .context("the connector runner could not be built")?,
    );
    let plugin_fetcher = Arc::new(
        boma_infra::GitHubPluginFetcher::new().context("the plugin fetcher could not be built")?,
    );
    Ok(App::new(
        Stores::new(
            Arc::new(PostgresAccountStore::new(pool.clone())),
            Arc::new(PostgresUserStore::new(pool.clone())),
            Arc::new(PostgresSessionStore::new(pool.clone())),
            Arc::new(PostgresSpaceStore::new(pool.clone())),
            Arc::new(PostgresBotStore::new(pool.clone())),
            Arc::new(PostgresThreadStore::new(pool.clone())),
            Arc::new(PostgresMessageStore::new(pool.clone())),
            Arc::new(PostgresRunStore::new(pool.clone())),
            Arc::new(PostgresEventStore::new(pool.clone())),
            Arc::new(PostgresInvitationStore::new(pool.clone())),
            Arc::new(PostgresCardStore::new(pool.clone())),
            Arc::new(PostgresPendingToolCallStore::new(pool.clone())),
            Arc::new(PostgresEffectStore::new(pool.clone())),
            Arc::new(PostgresToolStore::new(pool.clone())),
            Arc::new(PostgresRateLimitStore::new(pool.clone())),
            Arc::new(PostgresSecretStore::new(pool.clone(), secrets_vault()?)),
            Arc::new(PostgresPluginSubmissionStore::new(pool.clone())),
            Arc::new(PostgresSkillStore::new(pool.clone())),
            Arc::new(PostgresMcpServerStore::new(pool.clone(), secrets_vault()?)),
            Arc::new(PostgresAuditStore::new(pool.clone())),
            Arc::new(PostgresConnectorStore::new(pool.clone(), secrets_vault()?)),
            Arc::new(PostgresPluginInstallStore::new(pool.clone())),
            Arc::new(PostgresReactionStore::new(pool.clone())),
            Arc::new(PostgresReadStateStore::new(pool.clone())),
            Arc::new(PostgresRosterStore::new(pool.clone())),
            Arc::new(PostgresMemoryStore::new(pool.clone())),
            Arc::new(PostgresRoutineStore::new(pool.clone())),
            Arc::new(PostgresComputerStore::new(pool.clone())),
            Arc::new(PostgresScreenCapabilityStore::new(pool)),
        ),
        {
            let services = Services::new(
                publisher,
                Arc::new(Argon2idHasher::default()),
                Arc::new(TokenGenerator),
                Arc::new(IdGenerator),
                Arc::new(SystemClock),
                boma_domain::model::DeploymentModels {
                    reachable: vec![
                        "openai/gpt-5".to_owned(),
                        "anthropic/claude-sonnet".to_owned(),
                    ],
                    default: "anthropic/claude-sonnet".to_owned(),
                },
                mcp_transport,
                plugin_fetcher,
            )
            .with_connectors(connector_runner);
            match computer {
                Some(runtime) => services.with_computers(runtime),
                None => services,
            }
        },
    ))
}

/// Drains the job queue until the process is asked to stop.
///
/// The worker claims one queued run at a time and answers it with the scripted provider: the M1
/// agent is a script, so a deployment can be exercised end to end before a model is connected.
///
/// # Errors
///
/// Returns an error when the database cannot be reached or the pool cannot be built.
pub async fn worker(config: Config) -> anyhow::Result<()> {
    let database =
        Postgres::connect_lazy(&config.database).context("the database pool cannot be built")?;
    let computer = Some(config.computer.provision().await.with_context(|| {
        format!(
            "SANDBOX_PROVIDER={} is set but does not answer: {}",
            config.computer.provider.name(),
            config.computer,
        )
    })?);
    let app = Arc::new(build_app(&database, computer)?);
    let dispatcher_app = Arc::clone(&app);
    // The BYOK provider, when the deployment is configured with one (REQ-015);
    // otherwise the scripted provider keeps the deployment exercisable end to end.
    let provider: Arc<dyn boma_app::ports::ModelProvider> = if let Some(provider) = &config.provider
    {
        let adapter = boma_infra::OpenAiCompatibleAdapter::new(boma_infra::OpenAiConfig {
            endpoint: provider.endpoint.clone(),
            model: provider.model.clone(),
            api_key: provider.api_key.clone(),
        })
        .context("the BYOK provider could not be built")?;
        tracing::info!(endpoint = %provider.endpoint, model = %provider.model, "the deployment answers with the provider's model");
        Arc::new(adapter)
    } else {
        tracing::info!(
            "BOMA_PROVIDER_ENDPOINT/MODEL/API_KEY not set: the scripted provider answers"
        );
        Arc::new(ScriptedProvider::default())
    };
    let worker = Worker::new(app, provider, worker_owner());

    tracing::info!(owner = %worker_owner(), "the worker is draining the queue");
    loop {
        // Sweep the routines before claiming work: a due routine is real work the space
        // asked for, and the queue order is by wake time.
        let dispatched = boma_app::usecases::dispatch_routines(&dispatcher_app)
            .await
            .context("the worker could not sweep the routines")?;
        if dispatched > 0 {
            tracing::info!(dispatched, "the worker enqueued routine wakes");
            continue;
        }
        // Release computers nobody has touched: capacity is finite.
        let released = boma_app::usecases::sweep_idle_computers(&dispatcher_app)
            .await
            .context("the worker could not release idle computers")?;
        if released > 0 {
            tracing::info!(released, "the worker released idle computers");
        }
        let stepped = worker
            .step()
            .await
            .context("the worker could not claim a run")?;
        if !stepped {
            tokio::time::sleep(WORKER_POLL_INTERVAL).await;
        }
    }
}

/// The name this worker claims runs under.
///
/// It carries the process id, so two workers on the same box never share a lease.
fn worker_owner() -> String {
    format!("worker-{}", std::process::id())
}

/// Re-wraps every space's data key under a new master key (REQ-046, DEC-10 option (a)).
///
/// Reads `BOMA_MASTER_KEY` (the current key) and `BOMA_NEW_MASTER_KEY` (the new one). Secret
/// ciphertexts are never re-encrypted; only the wrapping rows change. The operator swaps
/// `BOMA_MASTER_KEY` to the new value in the deployment after this command exits 0.
///
/// # Errors
///
/// Returns an error when either key is absent or malformed, the database is unreachable, or a
/// row does not unwrap under the old key.
pub async fn rekey(config: Config) -> anyhow::Result<()> {
    let new_key = std::env::var("BOMA_NEW_MASTER_KEY")
        .context("BOMA_NEW_MASTER_KEY is required (the new 64-hex master key)")?;
    let old = secrets_vault()?;
    anyhow::ensure!(
        old.stable_key(),
        "BOMA_MASTER_KEY must be set: re-wrapping under an ephemeral key is a no-op masquerading as a rotation"
    );
    let new = boma_infra::SecretBox::from_env(&std::collections::BTreeMap::from([(
        "BOMA_MASTER_KEY".to_owned(),
        new_key,
    )]))
    .context("BOMA_NEW_MASTER_KEY is not a valid 64-hex master key")?;
    let database = Postgres::connect(&config.database)
        .await
        .context("cannot reach the database")?;
    let updated = boma_infra::rekey_space_keys(database.pool(), &old, &new)
        .await
        .context("the re-wrap failed")?;
    tracing::info!(
        updated,
        "space data keys re-wrapped under the new master key"
    );
    Ok(())
}

/// Applies every pending migration and exits.
///
/// # Errors
///
/// Returns an error when the database is unreachable or a migration fails.
pub async fn migrate(config: Config) -> anyhow::Result<()> {
    let database = Postgres::connect(&config.database)
        .await
        .context("cannot reach the database")?;
    database.migrate().await.context("a migration failed")?;
    tracing::info!("migrations applied");
    Ok(())
}

/// Logs the validated configuration with credentials redacted.
///
/// Output goes through the log subscriber rather than `println!`: `print_stdout` is denied in
/// non-test code, and an operator can redirect logs like any other process output.
pub fn check_config(config: &Config) {
    tracing::info!(config = %config, "configuration is valid");
}

/// Resolves when the process is asked to stop, so in-flight requests can finish.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for ctrl-c");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => tracing::error!(%error, "cannot listen for SIGTERM"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutting down");
}
