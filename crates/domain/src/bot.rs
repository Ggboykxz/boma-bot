//! Bots: the agents a member talks to.

use serde::{Deserialize, Serialize};

use crate::{
    error::DomainError,
    ids::{BotId, SpaceId, ThreadId, UserId},
};

/// The longest name a bot may carry.
pub const MAX_BOT_NAME_LENGTH: usize = 120;

/// A bot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bot {
    /// The bot's id.
    pub id: BotId,
    /// The tenant it belongs to.
    pub space_id: SpaceId,
    /// The member who created it.
    pub user_id: UserId,
    /// The name a person sees.
    pub name: String,
    /// What the bot is for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The colour a person sees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Its conversation.
    pub thread_id: ThreadId,
    /// When it was archived, if it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// When it was created.
    pub created_at: String,
}

impl Bot {
    /// Checks the rules a bot must satisfy before it is created.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidName`] when the name is empty or longer than
    /// [`MAX_BOT_NAME_LENGTH`].
    pub fn validate_name(name: &str) -> Result<(), DomainError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(DomainError::InvalidName("a bot needs a name".to_owned()));
        }
        if trimmed.chars().count() > MAX_BOT_NAME_LENGTH {
            return Err(DomainError::InvalidName(format!(
                "a bot name cannot exceed {MAX_BOT_NAME_LENGTH} characters"
            )));
        }
        Ok(())
    }

    /// Whether the bot can receive a message.
    #[must_use]
    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::Bot;

    #[test]
    fn req_001_a_bot_needs_a_name() {
        assert!(Bot::validate_name("").is_err());
        assert!(Bot::validate_name("   ").is_err());
        assert!(Bot::validate_name("Analyst").is_ok());
        assert!(Bot::validate_name(&"x".repeat(121)).is_err());
    }
}
