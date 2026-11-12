//! Run statuses, run triggers and the transition table.
//!
//! Source of truth: `spec/02-data-model.md` ("Run state machine") and REQ-012 in
//! `spec/01-requirements.md`. The table is exhaustive: any pair that is not listed is an error,
//! never a silent no-op, so a client cannot drag a run into a state the executor does not expect.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{
    error::DomainError,
    ids::{BotId, RunId, SpaceId, ThreadId, UserId},
};

/// Where a run is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Accepted, waiting for an executor to take a lease.
    Queued,
    /// An executor holds the lease and is about to start.
    Leased,
    /// The executor is working.
    Running,
    /// Paused on a question or an approval from a person.
    WaitingInput,
    /// Paused because a person asked to drive the computer.
    WaitingTakeover,
    /// Finished successfully.
    Completed,
    /// Finished with an error; the only status that may be retried.
    Failed,
    /// Stopped by a person or by policy.
    Cancelled,
}

impl RunStatus {
    /// Every status, in the order the vocabulary is written in the spec.
    pub const ALL: [Self; 8] = [
        Self::Queued,
        Self::Leased,
        Self::Running,
        Self::WaitingInput,
        Self::WaitingTakeover,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];

    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Leased => "leased",
            Self::Running => "running",
            Self::WaitingInput => "waiting_input",
            Self::WaitingTakeover => "waiting_takeover",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Statuses the chat shows as "working".
    #[must_use]
    pub const fn is_working(self) -> bool {
        matches!(self, Self::Queued | Self::Leased | Self::Running)
    }

    /// Statuses paused on a person: they hold no compute.
    #[must_use]
    pub const fn is_waiting_on_user(self) -> bool {
        matches!(self, Self::WaitingInput | Self::WaitingTakeover)
    }

    /// Statuses with no outgoing transition except the retry from `failed`.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Whether a new trigger for the same thread must wait behind this run.
    ///
    /// A question or an approval does not block the queue; a takeover does. This mirrors
    /// `QUEUE_HOLDING_RUN_STATUSES` in the reference (`packages/core/src/run-state.ts:19-23`).
    #[must_use]
    pub const fn holds_queue(self) -> bool {
        self.is_working() || matches!(self, Self::WaitingTakeover)
    }

    /// Whether the state machine allows moving from `self` to `next`.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Queued => matches!(next, Self::Leased | Self::Cancelled),
            Self::Leased => matches!(next, Self::Running | Self::Queued | Self::Cancelled),
            Self::Running => matches!(
                next,
                Self::WaitingInput
                    | Self::WaitingTakeover
                    | Self::Completed
                    | Self::Failed
                    | Self::Cancelled
                    | Self::Leased
            ),
            Self::WaitingInput | Self::WaitingTakeover => {
                matches!(next, Self::Queued | Self::Leased | Self::Cancelled)
            }
            Self::Completed | Self::Cancelled => false,
            Self::Failed => matches!(next, Self::Queued),
        }
    }

    /// Applies a transition or returns the rule that forbids it.
    ///
    /// # Errors
    ///
    /// Returns `DomainError::InvalidTransition` when the table does not allow the move. The error
    /// names both statuses so the caller can log the pair it attempted.
    pub fn transition_to(self, next: Self) -> Result<Self, DomainError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(DomainError::invalid_transition(self, next))
        }
    }
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RunStatus {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "leased" => Ok(Self::Leased),
            "running" => Ok(Self::Running),
            "waiting_input" => Ok(Self::WaitingInput),
            "waiting_takeover" => Ok(Self::WaitingTakeover),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(DomainError::UnknownStatus(other.to_owned())),
        }
    }
}

/// What caused a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunTrigger {
    /// A person sent a message.
    User,
    /// A routine fired.
    Routine,
    /// An inbound webhook.
    Webhook,
    /// Another bot.
    BotMessage,
    /// Ticket work.
    Tickets,
    /// The bot was just created.
    Created,
    /// A helper run returned.
    HelperResult,
    /// A retry of a failed run.
    Retry,
}

impl RunTrigger {
    /// Every trigger, in the order the spec lists them.
    pub const ALL: [Self; 8] = [
        Self::User,
        Self::Routine,
        Self::Webhook,
        Self::BotMessage,
        Self::Tickets,
        Self::Created,
        Self::HelperResult,
        Self::Retry,
    ];

    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Routine => "routine",
            Self::Webhook => "webhook",
            Self::BotMessage => "bot_message",
            Self::Tickets => "tickets",
            Self::Created => "created",
            Self::HelperResult => "helper_result",
            Self::Retry => "retry",
        }
    }

    /// Whether the run may take a person's message as steering.
    ///
    /// Routine, webhook, ticket, bot-to-bot and creation runs are not the conversation: their
    /// reply goes where it belongs, and a message that lands during them waits.
    #[must_use]
    pub const fn accepts_steering(self) -> bool {
        matches!(self, Self::User | Self::HelperResult | Self::Retry)
    }
}

impl fmt::Display for RunTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RunTrigger {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "user" => Ok(Self::User),
            "routine" => Ok(Self::Routine),
            "webhook" => Ok(Self::Webhook),
            "bot_message" => Ok(Self::BotMessage),
            "tickets" => Ok(Self::Tickets),
            "created" => Ok(Self::Created),
            "helper_result" => Ok(Self::HelperResult),
            "retry" => Ok(Self::Retry),
            other => Err(DomainError::unknown_trigger(other)),
        }
    }
}

/// One execution of the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// The run's id.
    pub id: RunId,
    /// The tenant it belongs to.
    pub space_id: SpaceId,
    /// The bot that works.
    pub bot_id: BotId,
    /// The thread it writes to.
    pub thread_id: ThreadId,
    /// The member who started it.
    pub user_id: UserId,
    /// Where it is.
    pub status: RunStatus,
    /// What caused it.
    pub trigger: RunTrigger,
    /// The nonce that made the start idempotent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_nonce: Option<String>,
    /// The message that started it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_message_id: Option<crate::ids::MessageId>,
    /// When it was created.
    pub created_at: String,
    /// When it started working.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// When it finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    /// Why it failed, when it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Who holds the lease, while the run is leased or running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_owner: Option<String>,
    /// The fence token: it increases on every claim, so a stale worker can be refused.
    #[serde(default)]
    pub lease_fence: i64,
    /// When the lease expires; an expired lease is recoverable (REQ-014).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<String>,
}

impl Run {
    /// Whether the given worker still holds this run's lease.
    ///
    /// A worker that was slow past its lease expiry, or that was replaced by a reconciler, must
    /// not write: the fence it presents no longer matches (REQ-012).
    #[must_use]
    pub fn holds_lease(&self, owner: &str, fence: i64) -> bool {
        self.lease_owner.as_deref() == Some(owner) && self.lease_fence == fence
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{RunStatus, RunTrigger};

    /// The table from `spec/02-data-model.md`, written out.
    const ALLOWED: &[(RunStatus, RunStatus)] = &[
        (RunStatus::Queued, RunStatus::Leased),
        (RunStatus::Queued, RunStatus::Cancelled),
        (RunStatus::Leased, RunStatus::Running),
        (RunStatus::Leased, RunStatus::Queued),
        (RunStatus::Leased, RunStatus::Cancelled),
        (RunStatus::Running, RunStatus::WaitingInput),
        (RunStatus::Running, RunStatus::WaitingTakeover),
        (RunStatus::Running, RunStatus::Completed),
        (RunStatus::Running, RunStatus::Failed),
        (RunStatus::Running, RunStatus::Cancelled),
        (RunStatus::Running, RunStatus::Leased),
        (RunStatus::WaitingInput, RunStatus::Queued),
        (RunStatus::WaitingInput, RunStatus::Leased),
        (RunStatus::WaitingInput, RunStatus::Cancelled),
        (RunStatus::WaitingTakeover, RunStatus::Queued),
        (RunStatus::WaitingTakeover, RunStatus::Leased),
        (RunStatus::WaitingTakeover, RunStatus::Cancelled),
        (RunStatus::Failed, RunStatus::Queued),
    ];

    #[test]
    fn req_012_transition_table_is_exhaustive() {
        for from in RunStatus::ALL {
            for to in RunStatus::ALL {
                let expected = ALLOWED.contains(&(from, to));
                assert_eq!(
                    from.can_transition_to(to),
                    expected,
                    "unexpected rule for {from} -> {to}"
                );
            }
        }
    }

    #[test]
    fn req_012_only_the_releasable_statuses_may_return_to_the_queue() {
        // The reference table (`packages/core/src/run-state.ts`, `allowed`) allows a lease to be
        // released (`leased -> queued`), a waiting run to be re-queued
        // (`waiting_input|waiting_takeover -> queued`) and a failed run to be retried
        // (`failed -> queued`). Nothing else may go back to the queue.
        for from in RunStatus::ALL {
            let may_requeue = matches!(
                from,
                RunStatus::Failed
                    | RunStatus::Leased
                    | RunStatus::WaitingInput
                    | RunStatus::WaitingTakeover
            );
            assert_eq!(
                from.can_transition_to(RunStatus::Queued),
                may_requeue,
                "{from} -> queued"
            );
        }
    }

    #[test]
    fn req_012_terminal_statuses_accept_nothing_but_retry() {
        for status in [RunStatus::Completed, RunStatus::Cancelled] {
            for next in RunStatus::ALL {
                assert!(!status.can_transition_to(next), "{status} -> {next}");
            }
        }
        assert!(RunStatus::Completed.is_terminal());
        assert!(RunStatus::Cancelled.is_terminal());
        assert!(RunStatus::Failed.is_terminal());
        assert!(!RunStatus::Running.is_terminal());
    }

    #[test]
    fn req_012_an_illegal_transition_is_an_error_not_a_no_op() {
        let error = RunStatus::Completed
            .transition_to(RunStatus::Running)
            .expect_err("the table forbids it");
        assert!(error.is_invalid_transition());
    }

    #[test]
    fn req_012_status_and_trigger_wire_values_round_trip() {
        for status in RunStatus::ALL {
            assert_eq!(
                RunStatus::from_str(status.as_str()).expect("a known status"),
                status
            );
        }
        for trigger in RunTrigger::ALL {
            assert_eq!(
                RunTrigger::from_str(trigger.as_str()).expect("a known trigger"),
                trigger
            );
        }
        assert!(RunStatus::from_str("paused").is_err());
        assert!(RunTrigger::from_str("cron").is_err());
    }

    #[test]
    fn req_003_non_conversational_triggers_never_take_steering() {
        for trigger in RunTrigger::ALL {
            let expected = matches!(
                trigger,
                RunTrigger::User | RunTrigger::HelperResult | RunTrigger::Retry
            );
            assert_eq!(
                trigger.accepts_steering(),
                expected,
                "unexpected steering rule for {trigger}"
            );
        }
    }
}
