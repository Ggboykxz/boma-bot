//! The `boma automation` operator client (REQ-072).
//!
//! A thin CLI over the deployment's local automation surface (`/api/v1/automation/*`): scripts
//! and operators drive bots on this machine with a deployment-wide bearer
//! (`BOMA_AUTOMATION_TOKEN`). Every call maps to a use case on the server side with the same
//! authorization rules; the CLI adds no authority of its own.
//!
//! Output goes through the log subscriber, like every other `boma` job (`print_stdout` is
//! denied workspace-wide; see AGENTS.md). `BOMA_LOG_FORMAT=json` produces one JSON line per
//! answer, with the surface's body in the `body` field. Scripts that need to consume the body
//! programmatically should call the HTTP surface directly — that is the interface kept stable
//! for them, this CLI is the operator's ergonomic shorthand over it.
//!
//! Configuration:
//! - `BOMA_AUTOMATION_TOKEN` (required) — the bearer the deployment configured.
//! - `BOMA_AUTOMATION_ENDPOINT` (optional, default `http://127.0.0.1:3000`) — where the
//!   deployment serves HTTP. Loopback by default, like everything else here.

use std::time::Duration;

use anyhow::{Context, bail};
use clap::Subcommand;

/// The operator automation jobs of `boma automation`.
#[derive(Debug, Subcommand)]
pub enum AutomationCommand {
    /// List the agents the token may drive.
    Agents,
    /// Send one message to an agent, optionally waiting for the run to finish.
    Send {
        /// The bot id to drive.
        #[arg(long)]
        bot: String,
        /// The message text for the agent.
        #[arg(long)]
        text: String,
        /// Wait for a terminal run before exiting.
        #[arg(long)]
        wait: bool,
        /// How long to wait for a terminal run, in seconds (capped by the deployment).
        #[arg(long)]
        timeout_seconds: Option<u64>,
    },
    /// List the pending approval cards of this deployment.
    Approvals,
    /// Answer a pending approval card.
    Answer {
        /// The card id to answer.
        #[arg(long)]
        card: String,
        /// The answer to record (`approve` or `deny`).
        #[arg(long)]
        answer: String,
    },
    /// Fire a routine once, optionally waiting for its run to finish.
    RoutineRun {
        /// The bot id the routine belongs to.
        #[arg(long)]
        bot: String,
        /// The routine id to fire.
        #[arg(long)]
        routine: String,
        /// Wait for the routine's run to finish before exiting. Bounded by the deployment's
        /// `BOMA_AUTOMATION_WAIT_TIMEOUT_SECONDS`.
        #[arg(long)]
        wait: bool,
    },
    /// Print the state of one run (poll until it leaves `running`).
    RunStatus {
        /// The run id to read.
        #[arg(long)]
        run: String,
    },
}

/// The endpoint root; loopback by default.
fn endpoint() -> String {
    std::env::var("BOMA_AUTOMATION_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:3000".to_owned())
        .trim_end_matches('/')
        .to_owned()
}

/// The configured bearer; a missing one is an operator error, not a silent fallback.
fn token() -> anyhow::Result<String> {
    std::env::var("BOMA_AUTOMATION_TOKEN")
        .context("BOMA_AUTOMATION_TOKEN is required (the deployment's automation bearer)")
}

/// Performs one call and prints the JSON body; a non-2xx answer is a failure with the body on
/// stderr, never a silent success.
async fn request(
    client: &reqwest::Client,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> anyhow::Result<()> {
    let url = format!("{}{path}", endpoint());
    let mut builder = client
        .request(method, &url)
        .bearer_auth(token()?)
        // Whole-request cap; covers the server's `wait=true` long poll plus margin. Operators
        // running longer waits raise `BOMA_AUTOMATION_CLIENT_TIMEOUT_SECONDS`.
        .timeout(Duration::from_secs(client_timeout_seconds()));
    if let Some(body) = body {
        builder = builder.json(&body);
    }
    let response = builder.send().await.with_context(|| {
        format!("the automation surface at {url} did not answer; is the deployment serving?")
    })?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if status.is_success() {
        tracing::info!(%status, body = %text, "automation answer");
        return Ok(());
    }
    tracing::error!(%status, body = %text, "the automation surface refused the call");
    bail!("the automation surface refused the call ({status})")
}

/// The whole-request cap; 300 s by default, overridable for long `wait=true` calls.
fn client_timeout_seconds() -> u64 {
    std::env::var("BOMA_AUTOMATION_CLIENT_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(300)
}

/// Installs a log subscriber so the CLI's answers are visible; `print_stdout` is denied
/// workspace-wide. `BOMA_LOG_FORMAT` (default `pretty`) and `BOMA_LOG_FILTER` (default
/// `info`) are honoured; a bad filter is the operator's error to fix, never silent.
fn init_output() -> anyhow::Result<()> {
    let settings = crate::config::TelemetrySettings {
        filter: std::env::var("BOMA_LOG_FILTER").unwrap_or_else(|_| "info".to_owned()),
        format: match std::env::var("BOMA_LOG_FORMAT").as_deref() {
            Ok("json") => crate::config::LogFormat::Json,
            _ => crate::config::LogFormat::Pretty,
        },
    };
    crate::telemetry::init(&settings)
}

/// Runs one automation job.
///
/// # Errors
///
/// Returns an error when the deployment does not answer, refuses the call, or the token is
/// missing.
pub async fn run(command: AutomationCommand) -> anyhow::Result<()> {
    init_output()?;
    let client = reqwest::Client::new();
    match command {
        AutomationCommand::Agents => {
            request(
                &client,
                reqwest::Method::GET,
                "/api/v1/automation/agents",
                None,
            )
            .await
        }
        AutomationCommand::Send {
            bot,
            text,
            wait,
            timeout_seconds,
        } => {
            let mut body = serde_json::json!({ "text": text, "wait": wait });
            if let Some(timeout_seconds) = timeout_seconds {
                body["timeoutSeconds"] = serde_json::json!(timeout_seconds);
            }
            request(
                &client,
                reqwest::Method::POST,
                &format!("/api/v1/automation/agents/{bot}/messages"),
                Some(body),
            )
            .await
        }
        AutomationCommand::Approvals => {
            request(
                &client,
                reqwest::Method::GET,
                "/api/v1/automation/approvals",
                None,
            )
            .await
        }
        AutomationCommand::Answer { card, answer } => {
            request(
                &client,
                reqwest::Method::POST,
                &format!("/api/v1/automation/approvals/{card}/answer"),
                Some(serde_json::json!({ "answer": answer })),
            )
            .await
        }
        AutomationCommand::RoutineRun { bot, routine, wait } => {
            request(
                &client,
                reqwest::Method::POST,
                &format!("/api/v1/automation/agents/{bot}/routines/{routine}/run"),
                Some(serde_json::json!({ "wait": wait })),
            )
            .await
        }
        AutomationCommand::RunStatus { run } => {
            request(
                &client,
                reqwest::Method::GET,
                &format!("/api/v1/automation/runs/{run}"),
                None,
            )
            .await
        }
    }
}
