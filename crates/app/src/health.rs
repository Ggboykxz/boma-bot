//! Readiness and capability reporting (REQ-053).
//!
//! Two separate questions, kept separate on purpose:
//!
//! * *Is the process alive?* — `/health` answers from memory and must never touch a dependency,
//!   otherwise a dependency outage would make the orchestrator kill a healthy process.
//! * *Can it serve traffic?* — `/ready` probes every declared dependency with a deadline and
//!   reports what this deployment can do, without a secret in sight (REQ-053, REQ-054).

use std::{fmt, sync::Arc, time::Duration};

use async_trait::async_trait;

/// A dependency that must answer before the process accepts traffic.
#[async_trait]
pub trait ReadinessCheck: Send + Sync + fmt::Debug {
    /// Stable name used in logs and in the failure summary.
    fn name(&self) -> &'static str;

    /// Probes the dependency.
    ///
    /// The error is a short static reason rather than the driver's error: a driver error can
    /// embed the connection string, and `/ready` is unauthenticated (REQ-054).
    async fn check(&self) -> Result<(), &'static str>;
}

/// The set of probes behind `/ready`.
#[derive(Debug, Clone, Default)]
pub struct Readiness {
    checks: Vec<Arc<dyn ReadinessCheck>>,
}

impl Readiness {
    /// Builds a readiness report from the given probes.
    #[must_use]
    pub fn new(checks: Vec<Arc<dyn ReadinessCheck>>) -> Self {
        Self { checks }
    }

    /// How many probes this deployment declares.
    #[must_use]
    pub fn len(&self) -> usize {
        self.checks.len()
    }

    /// Whether no probe is declared. The binary refuses to start in that state.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.checks.is_empty()
    }

    /// Runs every probe once, giving each of them `timeout`.
    ///
    /// A probe that does not answer inside `timeout` counts as a failure: an orchestrator must not
    /// wait on a database that stopped answering (REQ-053).
    pub async fn report(&self, timeout: Duration) -> ReadinessReport {
        let mut failing = Vec::new();
        for check in &self.checks {
            match tokio::time::timeout(timeout, check.check()).await {
                Ok(Ok(())) => {}
                Ok(Err(reason)) => {
                    tracing::warn!(check = check.name(), reason, "readiness probe failed");
                    failing.push(check.name());
                }
                Err(_elapsed) => {
                    tracing::warn!(
                        check = check.name(),
                        reason = "timeout",
                        "readiness probe failed"
                    );
                    failing.push(check.name());
                }
            }
        }
        ReadinessReport { failing }
    }
}

/// The outcome of one readiness pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadinessReport {
    failing: Vec<&'static str>,
}

impl ReadinessReport {
    /// Whether every probe answered in time.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.failing.is_empty()
    }

    /// Names of the dependencies that did not answer, in declaration order.
    #[must_use]
    pub fn failing(&self) -> &[&'static str] {
        &self.failing
    }
}

/// What this deployment can do, as `/ready` reports it.
///
/// The values come from configuration at startup; nothing here is read per request, and nothing
/// here is a secret. `degraded` is the flag an orchestrator reads when an optional subsystem is
/// switched off rather than broken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// Build version reported to operators.
    pub version: String,
    /// Where the agent executes: `sandboxed`, `host` or `disabled`.
    pub runtime: String,
    /// Computer provider in use: `docker`, `desktop` or `none`.
    pub sandbox: String,
    /// Integration families that are configured.
    pub integrations: Vec<String>,
    /// Whether a messaging channel is configured.
    pub messaging: bool,
    /// Inbound e-mail address, when the deployment accepts mail.
    pub email: Option<String>,
    /// Billing provider name, when one is configured.
    pub billing: Option<String>,
    /// Job execution mode: `inline`, `postgres` or `disabled`.
    pub jobs: String,
    /// Realtime transport: `postgres`, `memory` or `disabled`.
    pub realtime: String,
    /// Whether an optional subsystem is switched off.
    pub degraded: bool,
    /// Source revision the binary was built from, when known.
    pub revision: Option<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            runtime: "sandboxed".to_owned(),
            sandbox: "none".to_owned(),
            integrations: Vec::new(),
            messaging: false,
            email: None,
            billing: None,
            jobs: "inline".to_owned(),
            realtime: "postgres".to_owned(),
            degraded: false,
            revision: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_trait::async_trait;

    use super::{Capabilities, Readiness, ReadinessCheck};

    #[derive(Debug)]
    struct Probe {
        name: &'static str,
        outcome: Outcome,
    }

    #[derive(Debug)]
    enum Outcome {
        Healthy,
        Down,
        Hang,
    }

    #[async_trait]
    impl ReadinessCheck for Probe {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn check(&self) -> Result<(), &'static str> {
            match self.outcome {
                Outcome::Healthy => Ok(()),
                Outcome::Down => Err("database unavailable"),
                Outcome::Hang => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok(())
                }
            }
        }
    }

    #[tokio::test]
    async fn req_053_readiness_passes_when_every_probe_answers() {
        let readiness = Readiness::new(vec![
            std::sync::Arc::new(Probe {
                name: "database",
                outcome: Outcome::Healthy,
            }),
            std::sync::Arc::new(Probe {
                name: "blobstore",
                outcome: Outcome::Healthy,
            }),
        ]);
        let report = readiness.report(Duration::from_millis(50)).await;
        assert!(report.is_ready());
        assert!(report.failing().is_empty());
        assert_eq!(readiness.len(), 2);
    }

    #[tokio::test]
    async fn req_053_readiness_fails_and_names_the_dependency_when_a_probe_is_down() {
        let readiness = Readiness::new(vec![
            std::sync::Arc::new(Probe {
                name: "database",
                outcome: Outcome::Down,
            }),
            std::sync::Arc::new(Probe {
                name: "blobstore",
                outcome: Outcome::Healthy,
            }),
        ]);
        let report = readiness.report(Duration::from_millis(50)).await;
        assert!(!report.is_ready());
        assert_eq!(report.failing(), ["database"]);
    }

    #[tokio::test]
    async fn req_053_readiness_fails_when_a_probe_exceeds_its_deadline() {
        let readiness = Readiness::new(vec![std::sync::Arc::new(Probe {
            name: "database",
            outcome: Outcome::Hang,
        })]);
        let report = readiness.report(Duration::from_millis(10)).await;
        assert!(!report.is_ready(), "a hanging probe must not pass");
        assert_eq!(report.failing(), ["database"]);
    }

    #[test]
    fn req_053_capabilities_default_to_a_secret_free_report() {
        let capabilities = Capabilities::default();
        assert_eq!(capabilities.version, env!("CARGO_PKG_VERSION"));
        assert!(capabilities.email.is_none());
        assert!(capabilities.billing.is_none());
        assert!(!capabilities.degraded);
        assert!(!capabilities.messaging);
    }
}
