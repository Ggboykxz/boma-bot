//! Computers: the sandbox a bot's browser and files live in (REQ-026..031).
//!
//! Source of truth: `spec/02-data-model.md` ("Computer") and REQ-026..031 in
//! `spec/01-requirements.md`. One computer per bot, started lazily, released when idle; the
//! provider may be the local Docker supervisor or `none`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{BotId, SpaceId};

crate::opaque_id!(
    /// The identifier of a computer.
    ComputerId
);

/// The lifecycle a computer is in.
///
/// The provisioning vocabulary from the data model (`queued, running, interrupted, completed,
/// failed`) plus `stopped` for "never started or released"; the wire carries exactly these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerStatus {
    /// The provider was asked to boot a computer and has not answered yet.
    Queued,
    /// The computer is running; its screen generation is current.
    Running,
    /// The provider interrupted the work (e.g. the host went away).
    Interrupted,
    /// The provider finished the computer's provisioning work (unused for long-lived computers).
    Completed,
    /// The computer failed to start or behaved unpredictably.
    Failed,
    /// The computer is not provisioned: never started, or released after idle.
    Stopped,
}

impl ComputerStatus {
    /// Every status, in declaration order.
    pub const ALL: [Self; 6] = [
        Self::Queued,
        Self::Running,
        Self::Interrupted,
        Self::Completed,
        Self::Failed,
        Self::Stopped,
    ];

    /// The wire name of the status.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Interrupted => "interrupted",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

impl fmt::Display for ComputerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ComputerStatus {
    type Err = DomainError;

    fn from_str(status: &str) -> Result<Self, Self::Err> {
        match status {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "interrupted" => Ok(Self::Interrupted),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "stopped" => Ok(Self::Stopped),
            _ => Err(DomainError::InvalidName(format!(
                "`{status}` is not a known computer status"
            ))),
        }
    }
}

/// A computer, as the stores hold it.
#[derive(Debug, Clone, PartialEq)]
pub struct Computer {
    /// The computer's id.
    pub id: ComputerId,
    /// The tenant it belongs to.
    pub space_id: SpaceId,
    /// The bot whose computer this is.
    pub bot_id: BotId,
    /// The provider key: `docker`, `none`, or a cloud provider.
    pub provider: String,
    /// The provider-side handle, when one exists (a container id, for docker).
    pub external_id: Option<String>,
    /// Where the computer is.
    pub status: ComputerStatus,
    /// The generation of the screen backend; bumps on every real boot.
    pub screen_generation: u64,
    /// The provider-side storage this computer's home lives in.
    pub home_id: Option<String>,
    /// Whether the computer is isolated from the space's trust boundary (REQ-031).
    pub private: bool,
    /// RAM cap for the computer, in megabytes.
    pub memory_limit_mb: u64,
    /// CPU cap for the computer, in CPUs.
    pub cpu_limit: f64,
    /// Minutes the computer may sit idle before the sweeper releases it.
    pub idle_minutes: u64,
    /// When the computer started, if it is running.
    pub started_at: Option<String>,
    /// When the computer last did anything.
    pub last_used_at: Option<String>,
    /// When the computer was released, if it is not running.
    pub stopped_at: Option<String>,
    /// When the computer row was created.
    pub created_at: String,
    /// When the computer row last changed.
    pub updated_at: String,
}

/// The RAM cap bounds, in megabytes (mirrors the database check and REQ-027).
pub const MIN_MEMORY_LIMIT_MB: u64 = 64;
/// The RAM cap bounds, in megabytes (mirrors the database check and REQ-027).
pub const MAX_MEMORY_LIMIT_MB: u64 = 4_096;
/// The CPU cap bounds (mirrors the database check and REQ-027).
pub const MIN_CPU_LIMIT: f64 = 0.25;
/// The CPU cap bounds (mirrors the database check and REQ-027).
pub const MAX_CPU_LIMIT: f64 = 4.0;
/// The idle window bounds, in minutes (mirrors the database check and REQ-027).
pub const MIN_IDLE_MINUTES: u64 = 1;
/// The idle window bounds, in minutes (mirrors the database check and REQ-027).
pub const MAX_IDLE_MINUTES: u64 = 1_440;

/// Checks a memory cap, returning the value unchanged when it is in bounds.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the cap is outside the bounds.
pub fn validate_memory_limit(mb: u64) -> Result<u64, DomainError> {
    if (MIN_MEMORY_LIMIT_MB..=MAX_MEMORY_LIMIT_MB).contains(&mb) {
        Ok(mb)
    } else {
        Err(DomainError::InvalidName(format!(
            "`{mb}` MB is outside the memory limit bounds"
        )))
    }
}

/// Checks a CPU cap, returning the value unchanged when it is in bounds.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the cap is outside the bounds, or not a number.
pub fn validate_cpu_limit(cpu: f64) -> Result<f64, DomainError> {
    if cpu.is_finite() && (MIN_CPU_LIMIT..=MAX_CPU_LIMIT).contains(&cpu) {
        Ok(cpu)
    } else {
        Err(DomainError::InvalidName(format!(
            "`{cpu}` is outside the cpu limit bounds"
        )))
    }
}

/// Checks an idle window, returning the value unchanged when it is in bounds.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the window is outside the bounds.
pub fn validate_idle_minutes(minutes: u64) -> Result<u64, DomainError> {
    if (MIN_IDLE_MINUTES..=MAX_IDLE_MINUTES).contains(&minutes) {
        Ok(minutes)
    } else {
        Err(DomainError::InvalidName(format!(
            "`{minutes}` minutes is outside the idle window bounds"
        )))
    }
}
