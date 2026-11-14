//! The ticket board (REQ-038): states, and the name rules for titles and comments.

use crate::DomainError;

/// The longest ticket title.
pub const TICKET_TITLE_MAX: usize = 200;
/// The longest ticket description.
pub const TICKET_DESCRIPTION_MAX: usize = 4_000;
/// The longest comment.
pub const TICKET_COMMENT_MAX: usize = 10_000;

/// The four states a ticket lives in, in board order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketState {
    /// Not yet picked up.
    Backlog,
    /// Wanted, not started.
    Todo,
    /// Being worked on.
    InProgress,
    /// Finished.
    Done,
}

impl TicketState {
    /// The wire value stored in the `state` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Done => "done",
        }
    }

    /// Every state, in board order.
    pub const ALL: [Self; 4] = [Self::Backlog, Self::Todo, Self::InProgress, Self::Done];

    /// Parses a wire value.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidName`] for a value outside the vocabulary.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "backlog" => Ok(Self::Backlog),
            "todo" => Ok(Self::Todo),
            "in_progress" => Ok(Self::InProgress),
            "done" => Ok(Self::Done),
            other => Err(DomainError::InvalidName(format!(
                "the ticket state `{other}` is not one of backlog, todo, in_progress, done"
            ))),
        }
    }
}

/// Checks a ticket title: trimmed, small, printable.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the title is refused.
pub fn validate_ticket_title(title: &str) -> Result<(), DomainError> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidName(
            "the ticket title must not be empty".to_owned(),
        ));
    }
    if trimmed.chars().count() > TICKET_TITLE_MAX {
        return Err(DomainError::InvalidName(format!(
            "the ticket title is longer than {TICKET_TITLE_MAX} characters"
        )));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(DomainError::InvalidName(
            "the ticket title must not contain control characters".to_owned(),
        ));
    }
    Ok(())
}

/// Checks a ticket description or comment body: bounded, printable.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the text is refused.
pub fn validate_ticket_text(text: &str, max: usize, what: &str) -> Result<(), DomainError> {
    if text.chars().count() > max {
        return Err(DomainError::InvalidName(format!(
            "the {what} is longer than {max} characters"
        )));
    }
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(DomainError::InvalidName(format!(
            "the {what} must not contain control characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TicketState, validate_ticket_text, validate_ticket_title};

    #[test]
    fn req_038_the_state_vocabulary_round_trips() {
        for state in TicketState::ALL {
            assert_eq!(
                TicketState::parse(state.as_str()).expect("the state parses"),
                state
            );
        }
        assert!(TicketState::parse("closed").is_err());
        assert!(TicketState::parse("").is_err());
    }

    #[test]
    fn req_038_a_title_must_be_printable_and_bounded() {
        assert!(validate_ticket_title("Write the release report").is_ok());
        assert!(validate_ticket_title("").is_err());
        assert!(validate_ticket_title("   ").is_err());
        assert!(validate_ticket_title(&"x".repeat(201)).is_err());
        assert!(validate_ticket_title("bad\u{0007}bell").is_err());
    }

    #[test]
    fn req_038_a_comment_is_bounded_and_printable() {
        assert!(validate_ticket_text("Looks good.", 10_000, "comment").is_ok());
        assert!(validate_ticket_text(&"x".repeat(10_001), 10_000, "comment").is_err());
        assert!(validate_ticket_text("bad\u{0007}bell", 10_000, "comment").is_err());
    }
}
