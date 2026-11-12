//! Errors raised by pure domain rules.

use std::fmt;

use crate::run::RunStatus;

/// A rule violation that carries no IO context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// The state machine does not allow this transition.
    InvalidTransition {
        /// Status the run is in.
        from: RunStatus,
        /// Status the caller asked for.
        to: RunStatus,
    },
    /// A status string that is not part of the vocabulary.
    UnknownStatus(String),
    /// A trigger string that is not part of the vocabulary.
    UnknownTrigger(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { from, to } => {
                write!(f, "a run cannot move from {from} to {to}")
            }
            Self::UnknownStatus(value) => write!(f, "unknown run status: {value}"),
            Self::UnknownTrigger(value) => write!(f, "unknown run trigger: {value}"),
        }
    }
}

impl std::error::Error for DomainError {}

impl DomainError {
    /// Builds the error for a forbidden transition.
    #[must_use]
    pub const fn invalid_transition(from: RunStatus, to: RunStatus) -> Self {
        Self::InvalidTransition { from, to }
    }

    /// Builds the error for an unparsable trigger.
    pub fn unknown_trigger(value: impl Into<String>) -> Self {
        Self::UnknownTrigger(value.into())
    }

    /// Returns whether the caller asked for something the state machine forbids.
    #[must_use]
    pub const fn is_invalid_transition(&self) -> bool {
        matches!(self, Self::InvalidTransition { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn req_012_domain_error_renders_a_readable_message() {
        let error = DomainError::invalid_transition(RunStatus::Completed, RunStatus::Running);
        assert!(error.is_invalid_transition());
        assert!(error.to_string().contains("completed"));
    }
}
