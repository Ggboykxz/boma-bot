//! The event stream behind a thread's subscription.
//!
//! Every frame carries the thread's monotonic sequence, so a client that reconnects can resume
//! from the last sequence it saw without losing an event (REQ-005, REQ-020).

use serde::{Deserialize, Serialize};

use crate::ids::{ThreadId, UserId};

/// What an event announces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    /// A new message.
    Message,
    /// A message grew: a streamed reply.
    MessageUpdate,
    /// A run changed state.
    Run,
    /// The bot is typing.
    Typing,
    /// A card appeared or changed.
    Card,
    /// The roster changed.
    Roster,
    /// Nothing happened, but the connection is alive.
    Heartbeat,
}

impl EventType {
    /// Every event type the spec names.
    pub const ALL: [Self; 7] = [
        Self::Message,
        Self::MessageUpdate,
        Self::Run,
        Self::Typing,
        Self::Card,
        Self::Roster,
        Self::Heartbeat,
    ];

    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::MessageUpdate => "message-update",
            Self::Run => "run",
            Self::Typing => "typing",
            Self::Card => "card",
            Self::Roster => "roster",
            Self::Heartbeat => "heartbeat",
        }
    }

    /// Parses a wire value.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::DomainError::UnknownEventType`] for a value outside the
    /// vocabulary.
    pub fn parse(value: &str) -> Result<Self, crate::error::DomainError> {
        match value {
            "message" => Ok(Self::Message),
            "message-update" => Ok(Self::MessageUpdate),
            "run" => Ok(Self::Run),
            "typing" => Ok(Self::Typing),
            "card" => Ok(Self::Card),
            "roster" => Ok(Self::Roster),
            "heartbeat" => Ok(Self::Heartbeat),
            other => Err(crate::error::DomainError::UnknownEventType(
                other.to_owned(),
            )),
        }
    }
}

/// One frame of a thread's stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// The thread the event belongs to.
    pub thread_id: ThreadId,
    /// The thread's sequence for this event.
    pub seq: i64,
    /// What the event announces.
    #[serde(rename = "type")]
    pub kind: EventType,
    /// The event's body, free-form per type.
    pub payload: serde_json::Value,
    /// When it happened.
    pub created_at: String,
}

/// The run state a `run` event announces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    /// The run's id.
    pub run_id: String,
    /// The status the run moved to.
    pub status: String,
}

/// A `message-update` event: the part of a streamed reply that arrived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageDelta {
    /// The message being written.
    pub message_id: String,
    /// The text that arrived.
    pub delta: String,
}

/// A `message` event: a message that exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRef {
    /// The message's id.
    pub message_id: String,
    /// Its sequence in the thread.
    pub seq: i64,
}

/// The roster event: who can see this thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roster {
    /// The members.
    pub user_ids: Vec<UserId>,
}

#[cfg(test)]
mod tests {
    use super::EventType;

    #[test]
    fn req_005_the_event_types_are_the_ones_the_spec_names() {
        assert_eq!(EventType::ALL.len(), 7);
        assert_eq!(
            EventType::parse("message-update").expect("a known type"),
            EventType::MessageUpdate
        );
        assert!(EventType::parse("messageupdate").is_err());
    }
}
