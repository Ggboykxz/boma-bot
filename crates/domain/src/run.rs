//! Run statuses, run triggers and the transition table.
//!
//! Source of truth: `spec/02-data-model.md` ("Run state machine") and REQ-012 in
//! `spec/01-requirements.md`. The table is exhaustive: any pair that is not listed is an error,
//! never a silent no-op, so a client cannot drag a run into a state the executor does not expect.

use std::fmt;
use std::str::FromStr;

use crate::error::DomainError;

/// Where a run is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    /// The wire value used by the API, the database and the goldens.
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

/// Why a run exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RunTrigger {
    /// A person sent a message.
    User,
    /// A schedule fired.
    Routine,
    /// An inbound webhook woke the bot.
    Webhook,
    /// Another bot wrote to this one.
    BotMessage,
    /// Work attached to a board ticket.
    Tickets,
    /// The bot was created and is introducing itself.
    Created,
    /// A parent run delegated work.
    Helper,
    /// A previous attempt failed and is being retried.
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
        Self::Helper,
        Self::Retry,
    ];

    /// The wire value used by the API and the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Routine => "routine",
            Self::Webhook => "webhook",
            Self::BotMessage => "bot_message",
            Self::Tickets => "tickets",
            Self::Created => "created",
            Self::Helper => "helper",
            Self::Retry => "retry",
        }
    }

    /// Whether a user message may steer this run.
    ///
    /// Non-conversational triggers carry their own prompt, so a message that arrives during one
    /// waits for the next turn instead of being folded into it (REQ-003).
    #[must_use]
    pub const fn is_conversational(self) -> bool {
        !matches!(
            self,
            Self::Routine | Self::Webhook | Self::Created | Self::Tickets | Self::BotMessage
        )
    }

    /// Whether the run must stay out of the chat and report elsewhere (the board).
    #[must_use]
    pub const fn is_background(self) -> bool {
        matches!(self, Self::Tickets)
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
            "helper" => Ok(Self::Helper),
            "retry" => Ok(Self::Retry),
            other => Err(DomainError::unknown_trigger(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RunStatus, RunTrigger};
    use crate::error::DomainError;

    /// The table written out again, pair by pair. Two independent spellings of the same rule:
    /// a typo in either one fails the exhaustive test below.
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
    fn req_007_a_pending_question_does_not_hold_the_queue() {
        assert!(!RunStatus::WaitingInput.holds_queue());
        assert!(RunStatus::WaitingTakeover.holds_queue());
        assert!(RunStatus::Running.holds_queue());
        assert!(!RunStatus::Completed.holds_queue());
    }

    #[test]
    fn req_003_non_conversational_triggers_never_take_steering() {
        for trigger in RunTrigger::ALL {
            let conversational = trigger.is_conversational();
            let expected = matches!(
                trigger,
                RunTrigger::User | RunTrigger::Helper | RunTrigger::Retry
            );
            assert_eq!(conversational, expected, "{trigger}");
        }
        assert!(RunTrigger::Tickets.is_background());
        assert!(!RunTrigger::User.is_background());
    }

    #[test]
    fn req_012_status_and_trigger_wire_values_round_trip() {
        for status in RunStatus::ALL {
            assert_eq!(status.as_str().parse::<RunStatus>(), Ok(status));
        }
        for trigger in RunTrigger::ALL {
            assert_eq!(trigger.as_str().parse::<RunTrigger>(), Ok(trigger));
        }
        assert_eq!(
            "paused".parse::<RunStatus>(),
            Err(DomainError::UnknownStatus("paused".to_owned()))
        );
    }

    #[test]
    fn req_012_an_illegal_transition_is_an_error_not_a_no_op() {
        let error = RunStatus::Completed.transition_to(RunStatus::Running).err();
        assert!(matches!(error, Some(DomainError::InvalidTransition { .. })));
        assert_eq!(
            RunStatus::Queued.transition_to(RunStatus::Leased),
            Ok(RunStatus::Leased)
        );
    }
}
