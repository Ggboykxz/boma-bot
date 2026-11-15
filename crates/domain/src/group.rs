//! The group chats (REQ-010).
//!
//! A group is a named, shared room: one thread threaded through several bots plus the members
//! that follow it. The thread is created with the group — one shared conversation — and the
//! membership rows record who is *in* the room. The door-keeping rule lives in the use cases:
//! a mention addresses only the bots it names (REQ-010).

use serde::{Deserialize, Serialize};

use crate::{DomainError, SpaceId, ThreadId};

/// The longest group name, in characters.
pub const MAX_GROUP_NAME_LENGTH: usize = 64;
/// The largest number of bots in one group. Beyond eight the mention UX degrades badly.
pub const MAX_GROUP_BOTS: usize = 8;

/// What a membership points at: a person or a bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    /// A person (member of the space).
    Person,
    /// A bot (member of the space).
    Bot,
}

impl MemberKind {
    /// The wire value the stores persist.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Bot => "bot",
        }
    }
}

/// The group's id: `grp_<n>` kin with spaces, people, runs and artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupId(String);

impl GroupId {
    /// Creates an id from its text.
    ///
    /// # Errors
    ///
    /// Refuses an empty handle.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError::InvalidName("empty group id".to_owned()));
        }
        Ok(Self(value))
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A group, as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    /// The group's id.
    pub id: GroupId,
    /// The tenant it belongs to.
    pub space_id: SpaceId,
    /// The name people see.
    pub name: String,
    /// The conversation everyone shares; held for every reply.
    pub thread_id: ThreadId,
    /// When it was created.
    pub created_at: String,
    /// The bot members, in creation order.
    pub bot_ids: Vec<crate::BotId>,
}

impl Group {
    /// Creates a group, validating the name.
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidName`] on empty or over-long names.
    pub fn validate_name(name: &str) -> Result<String, DomainError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(DomainError::InvalidName("a group needs a name".to_owned()));
        }
        if trimmed.chars().count() > MAX_GROUP_NAME_LENGTH {
            return Err(DomainError::InvalidName(format!(
                "a group name cannot exceed {MAX_GROUP_NAME_LENGTH} characters"
            )));
        }
        Ok(trimmed.to_owned())
    }
}
