//! The jobs the binary can run.

use std::{sync::Arc, time::Duration};

use anyhow::Context;
use boma_api::{AppState, router};
use boma_app::{App, Readiness, Services, Stores, Worker};
use boma_infra::{
    Argon2idHasher, BroadcastPublisher, IdGenerator, Postgres, PostgresAccountStore,
    PostgresBotStore, PostgresCardStore, PostgresEffectStore, PostgresEventStore,
    PostgresInvitationStore, PostgresMemoryStore, PostgresMessageStore, PostgresReactionStore,
    PostgresReadStateStore, PostgresRosterStore, PostgresRoutineStore, PostgresRunStore,
    PostgresSessionStore, PostgresSpaceStore, PostgresThreadStore, PostgresToolStore,
    PostgresUserStore, ScriptedProvider, SystemClock, TokenGenerator,
};
use tokio::net::TcpListener;

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
    let app = build_app(&database);
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

    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("the HTTP server stopped with an error")
}

/// Builds the application from the Postgres pool.
fn build_app(database: &Postgres) -> App {
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
            Arc::new(PostgresEffectStore::new(pool.clone())),
            Arc::new(PostgresToolStore::new(pool.clone())),
            Arc::new(PostgresReactionStore::new(pool.clone())),
            Arc::new(PostgresReadStateStore::new(pool.clone())),
            Arc::new(PostgresRosterStore::new(pool.clone())),
            Arc::new(PostgresMemoryStore::new(pool.clone())),
            Arc::new(PostgresRoutineStore::new(pool)),
        ),
        Services::new(
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
        ),
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
    let app = Arc::new(build_app(&database));
    let provider = Arc::new(ScriptedProvider::default());
    let dispatcher_app = Arc::clone(&app);
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
