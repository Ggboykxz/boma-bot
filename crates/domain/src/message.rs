//! Messages, blocks and the thread's ordering rules.

use serde::{Deserialize, Serialize};

use crate::{
    error::DomainError,
    ids::{BotId, MessageId, RunId, ThreadId, UserId},
};

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// A person.
    User,
    /// A bot.
    Bot,
    /// The system itself (a card, a notice).
    System,
    /// A tool's output.
    Tool,
}

impl MessageRole {
    /// Every role, in the order the spec lists them.
    pub const ALL: [Self; 4] = [Self::User, Self::Bot, Self::System, Self::Tool];

    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Bot => "bot",
            Self::System => "system",
            Self::Tool => "tool",
        }
    }

    /// Parses a wire value.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::UnknownRole`] for a value outside the vocabulary.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "user" => Ok(Self::User),
            "bot" => Ok(Self::Bot),
            "system" => Ok(Self::System),
            "tool" => Ok(Self::Tool),
            other => Err(DomainError::UnknownRole(other.to_owned())),
        }
    }
}

/// A piece of a message.
///
/// The spec names five block types. Only the text block carries content in M1; the others exist
/// so the shape is stable when they arrive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Block {
    /// Plain text.
    Text {
        /// The text itself.
        text: String,
    },
    /// An image.
    Image {
        /// The artifact id the image is stored under.
        artifact_id: String,
    },
    /// A file.
    File {
        /// The artifact id the file is stored under.
        artifact_id: String,
        /// The file's name.
        name: String,
    },
    /// A tool call.
    Tool {
        /// The tool's name.
        name: String,
        /// Whether the call is still running.
        running: bool,
    },
    /// A card: a question or an approval waiting for an answer.
    Card {
        /// The card's id.
        card_id: String,
        /// The card's kind.
        kind: String,
    },
}

impl Block {
    /// The text of a text block, if this is one.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// A message in a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// The message's id.
    pub id: MessageId,
    /// The thread it belongs to.
    pub thread_id: ThreadId,
    /// The per-thread sequence, monotonic and the only ordering a client may trust.
    pub seq: i64,
    /// Who wrote it.
    pub role: MessageRole,
    /// The blocks it is made of.
    pub blocks: Vec<Block>,
    /// The bot that wrote it, when a bot did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<BotId>,
    /// The person who wrote it, when a person did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<UserId>,
    /// The run that produced it, when a run did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// The message this one answers, when it is a reply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<MessageId>,
    /// A short quote of the message being answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_quote: Option<String>,
    /// The nonce that made the send idempotent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_nonce: Option<String>,
    /// When it was written.
    pub created_at: String,
}

/// The longest text a person may send in one message.
pub const MAX_MESSAGE_LENGTH: usize = 16_000;

/// Checks the text of a message the person wrote.
///
/// # Errors
///
/// Returns [`DomainError::EmptyMessage`] when the text is blank and
/// [`DomainError::MessageTooLong`] when it is longer than [`MAX_MESSAGE_LENGTH`].
pub fn validate_message_text(text: &str) -> Result<(), DomainError> {
    if text.trim().is_empty() {
        return Err(DomainError::EmptyMessage);
    }
    if text.chars().count() > MAX_MESSAGE_LENGTH {
        return Err(DomainError::MessageTooLong {
            limit: MAX_MESSAGE_LENGTH,
        });
    }
    Ok(())
}

/// A page of messages, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagePage {
    /// The messages, newest first.
    pub messages: Vec<Message>,
    /// The cursor for the page before this one, when there is one.
    pub next_cursor: Option<String>,
}

/// The cursor format: the sequence the next page starts before.
///
/// The client treats it as opaque; the shape is `seq:<n>`.
#[must_use]
pub fn cursor_from_seq(seq: i64) -> String {
    format!("seq:{seq}")
}

/// Reads a cursor back.
///
/// # Errors
///
/// Returns [`DomainError::InvalidCursor`] when the cursor is not one this product wrote.
pub fn parse_cursor(cursor: &str) -> Result<i64, DomainError> {
    let Some(raw) = cursor.strip_prefix("seq:") else {
        return Err(DomainError::InvalidCursor(cursor.to_owned()));
    };
    raw.parse::<i64>()
        .map_err(|_| DomainError::InvalidCursor(cursor.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{MessageRole, cursor_from_seq, parse_cursor, validate_message_text};

    #[test]
    fn req_001_a_message_needs_text_and_stays_inside_its_limit() {
        assert!(validate_message_text("hello").is_ok());
        assert!(validate_message_text("   ").is_err());
        assert!(validate_message_text(&"x".repeat(16_001)).is_err());
    }

    #[test]
    fn req_005_a_cursor_is_opaque_but_readable_by_the_server() {
        assert_eq!(cursor_from_seq(42), "seq:42");
        assert_eq!(parse_cursor("seq:42").expect("a known cursor"), 42);
        assert!(parse_cursor("42").is_err());
        assert!(parse_cursor("seq:abc").is_err());
    }

    #[test]
    fn req_005_the_message_roles_are_the_four_the_spec_names() {
        assert_eq!(MessageRole::ALL.len(), 4);
        assert_eq!(
            MessageRole::parse("bot").expect("a known role"),
            MessageRole::Bot
        );
        assert!(MessageRole::parse("assistant").is_err());
    }
}
