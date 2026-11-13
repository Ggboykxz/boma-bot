//! The jobs the binary can run.

use std::{sync::Arc, time::Duration};

use anyhow::Context;
use boma_api::{AppState, router};
use boma_app::{App, Readiness, Services, Stores, Worker};
use boma_infra::{
    Argon2idHasher, BroadcastPublisher, IdGenerator, Postgres, PostgresAccountStore,
    PostgresBotStore, PostgresCardStore, PostgresComputerStore, PostgresEffectStore,
    PostgresEventStore, PostgresInvitationStore, PostgresMemoryStore, PostgresMessageStore,
    PostgresPendingToolCallStore, PostgresRateLimitStore, PostgresReactionStore,
    PostgresReadStateStore, PostgresRosterStore, PostgresRoutineStore, PostgresRunStore,
    PostgresScreenCapabilityStore, PostgresSessionStore, PostgresSpaceStore, PostgresThreadStore,
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
    let app = build_app(&database, computer);
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

/// Builds the application from the Postgres pool, with the selected computer backend attached.
fn build_app(database: &Postgres, computer: Option<Arc<dyn ComputerRuntime>>) -> App {
    let pool = database.pool().clone();
    let publisher = Arc::new(BroadcastPublisher::default());
    App::new(
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
            );
            match computer {
                Some(runtime) => services.with_computers(runtime),
                None => services,
            }
        },
    )
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
    let app = Arc::new(build_app(&database, computer));
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
