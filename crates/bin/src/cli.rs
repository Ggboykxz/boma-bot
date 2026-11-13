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
#[command(
    name = "boma",
    version,
    about = "Boma Bot server, worker and operator tools"
)]
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
    /// Re-wrap every space's data key under a new master key (REQ-046).
    ///
    /// Reads `BOMA_MASTER_KEY` (the current key) and `BOMA_NEW_MASTER_KEY` (the new one).
    /// Secret ciphertexts are never re-encrypted; only the wrapping rows change.
    Rekey,
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
        Command::Rekey => commands::rekey(load_config()?).await,
    }
}

fn load_config() -> anyhow::Result<Config> {
    let config = Config::from_env().context("the configuration is not usable")?;
    telemetry::init(&config.telemetry)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{AdminCommand, Cli, Command};

    #[test]
    fn req_058_every_job_has_a_command_line() {
        for (arguments, expected) in [
            (vec!["boma", "serve"], "serve"),
            (vec!["boma", "worker"], "worker"),
            (vec!["boma", "migrate"], "migrate"),
            (vec!["boma", "rekey"], "rekey"),
            (vec!["boma", "probe"], "probe"),
        ] {
            let cli = Cli::try_parse_from(&arguments).expect("the command parses");
            let name = match cli.command {
                Command::Serve => "serve",
                Command::Worker => "worker",
                Command::Migrate => "migrate",
                Command::Rekey => "rekey",
                Command::Probe { .. } => "probe",
                Command::Admin { .. } => "admin",
            };
            assert_eq!(name, expected);
        }
    }

    #[test]
    fn req_058_the_probe_defaults_to_the_local_readiness_path() {
        let cli = Cli::try_parse_from(["boma", "probe"]).expect("the command parses");
        let Command::Probe { addr, path } = cli.command else {
            panic!("the command must be the probe");
        };
        assert_eq!(addr.to_string(), "127.0.0.1:3000");
        assert_eq!(path, "/ready");
    }

    #[test]
    fn req_058_the_operator_tools_are_a_subcommand_of_admin() {
        let cli =
            Cli::try_parse_from(["boma", "admin", "check-config"]).expect("the command parses");
        let Command::Admin { command } = cli.command else {
            panic!("the command must be admin");
        };
        assert!(matches!(command, AdminCommand::CheckConfig));
    }

    #[test]
    fn req_058_an_unknown_job_is_refused() {
        let error = Cli::try_parse_from(["boma", "launch-the-missiles"])
            .expect_err("an unknown job must not parse");
        assert!(error.to_string().contains("launch-the-missiles"), "{error}");
    }
}
