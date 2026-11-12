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
    /// An identifier arrived empty.
    EmptyId(&'static str),
    /// An e-mail address does not have the accepted shape.
    InvalidEmail(String),
    /// A password is shorter than the floor or longer than the ceiling.
    WeakPassword {
        /// Why the password was refused.
        reason: String,
    },
    /// A name is empty or too long.
    InvalidName(String),
    /// A role string that is not part of the vocabulary.
    UnknownRole(String),
    /// A message was blank.
    EmptyMessage,
    /// A message was longer than the limit.
    MessageTooLong {
        /// The limit, in characters.
        limit: usize,
    },
    /// A cursor this product did not write.
    InvalidCursor(String),
    /// A client nonce that is too long to store safely.
    InvalidNonce,
    /// An event type string that is not part of the vocabulary.
    UnknownEventType(String),
    /// A bot that is archived cannot receive a message.
    ArchivedBot,
    /// A user already exists with that address.
    EmailAlreadyTaken,
    /// A system reason string that is not part of the vocabulary.
    UnknownSystemReason(String),
    /// A page size outside the accepted range.
    InvalidPageSize {
        /// The size that was asked for.
        size: usize,
        /// The largest size the product accepts.
        max: usize,
    },
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { from, to } => {
                write!(f, "a run cannot move from {from} to {to}")
            }
            Self::UnknownStatus(value) => write!(f, "unknown run status: {value}"),
            Self::UnknownTrigger(value) => write!(f, "unknown run trigger: {value}"),
            Self::EmptyId(kind) => write!(f, "a {kind} id cannot be empty"),
            Self::InvalidEmail(_) => write!(f, "the e-mail address is not valid"),
            Self::WeakPassword { reason } | Self::InvalidName(reason) => write!(f, "{reason}"),
            Self::UnknownRole(value) => write!(f, "unknown role: {value}"),
            Self::EmptyMessage => write!(f, "a message cannot be blank"),
            Self::MessageTooLong { limit } => {
                write!(f, "a message cannot exceed {limit} characters")
            }
            Self::InvalidCursor(_) => write!(f, "the cursor is not one this product wrote"),
            Self::InvalidNonce => write!(f, "the client nonce is not acceptable"),
            Self::UnknownEventType(value) => write!(f, "unknown event type: {value}"),
            Self::ArchivedBot => write!(f, "the bot is archived"),
            Self::EmailAlreadyTaken => write!(f, "the address is already registered"),
            Self::UnknownSystemReason(value) => write!(f, "unknown system reason: {value}"),
            Self::InvalidPageSize { size, max } => {
                write!(f, "a page of {size} messages is not allowed (max {max})")
            }
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

    #[test]
    fn req_040_an_email_error_never_quotes_the_address() {
        let error = DomainError::InvalidEmail("person@example.com".to_owned());
        assert!(
            !error.to_string().contains("person@example.com"),
            "an address is personal data: the message must not quote it"
        );
    }
}
