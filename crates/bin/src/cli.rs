//! Command-line surface.
//!
//! One binary, several jobs (ADR-017): `serve` runs the API, `worker` drains the job queue,
//! `migrate` is a deploy step, `admin` is operator-only and `probe` is what a container health
//! check calls. `probe` deliberately does not need a database URL: it is used from outside the
//! process it checks.

use std::net::SocketAddr;

use anyhow::Context;
use clap::{Parser, Subcommand};

use crate::{commands, config::Config, probe, telemetry};

/// `boma` — server, worker and operator tools for Boma Bot.
#[derive(Debug, Parser)]
#[command(name = "boma", version, about = "Boma Bot server, worker and operator tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the HTTP API until asked to stop.
    Serve,
    /// Run the background worker.
    Worker,
    /// Apply database migrations in order and exit.
    Migrate,
    /// Operator tools; never reachable over HTTP by a member.
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
    /// Check a running instance's readiness over TCP.
    Probe {
        /// Address to connect to.
        #[arg(long, default_value = "127.0.0.1:3000")]
        addr: SocketAddr,
        /// Path to request.
        #[arg(long, default_value = "/ready")]
        path: String,
    },
}

#[derive(Debug, Subcommand)]
enum AdminCommand {
    /// Validate the configuration and log it with credentials redacted.
    CheckConfig,
}

/// Parses the command line and runs the requested job.
///
/// # Errors
///
/// Returns an error when the configuration is unusable, a dependency is unreachable, a migration
/// fails, or the requested job is not implemented yet.
pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Probe { addr, path } => probe::probe(addr, &path).await,
        Command::Admin { command } => {
            let config = load_config()?;
            match command {
                AdminCommand::CheckConfig => commands::check_config(&config),
            }
            Ok(())
        }
        Command::Serve => commands::serve(load_config()?).await,
        Command::Worker => commands::worker(load_config()?).await,
        Command::Migrate => commands::migrate(load_config()?).await,
    }
}

fn load_config() -> anyhow::Result<Config> {
    let config = Config::from_env().context("the configuration is not usable")?;
    telemetry::init(&config.telemetry)?;
    Ok(config)
}
