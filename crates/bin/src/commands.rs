//! The jobs the binary can run.

use anyhow::Context;
use boma_api::{AppState, router};
use boma_app::Readiness;
use boma_infra::Postgres;
use tokio::net::TcpListener;

use crate::config::Config;

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
    let state = AppState::new(readiness, config.capabilities.clone(), config.http.clone());

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

/// Drains the job queue.
///
/// # Errors
///
/// Always returns an error today: the queue lands in M1. A worker that exits `0` without working
/// would tell a supervisor that jobs are being processed.
pub fn worker(_config: Config) -> anyhow::Result<()> {
    anyhow::bail!("the worker loop arrives in milestone M1 (docs/PLAN.md); nothing is drained yet")
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
                let _ = stream.recv().await;
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
    tracing::info!("shutdown requested, draining in-flight requests");
}

#[cfg(test)]
mod tests {
    use super::{check_config, worker};
    use crate::config::Config;

    fn fixture_config() -> Config {
        Config::from_pairs([("BOMA_DATABASE_URL", "postgres://boma@127.0.0.1:5432/boma")])
            .expect("the fixture configuration is valid")
    }

    #[test]
    fn req_058_the_worker_refuses_to_pretend_it_processed_a_job() {
        let error = worker(fixture_config()).expect_err("the worker loop does not exist yet");
        assert!(
            error.to_string().contains("M1"),
            "the message must say when the worker arrives: {error}"
        );
    }

    #[test]
    fn req_054_the_configuration_is_logged_with_the_credentials_redacted() {
        // The rendering must not contain the password; `Config`'s own `Display` is what the log
        // line uses.
        let config = fixture_config();
        let rendered = format!("{config}");
        assert!(
            !rendered.contains("postgres://boma@127.0.0.1"),
            "{rendered}"
        );

        check_config(&config);
    }
}
