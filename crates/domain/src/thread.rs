//! Threads: one conversation per bot, and the sequences that order it.

use serde::{Deserialize, Serialize};

use crate::ids::{BotId, SpaceId, ThreadId, UserId};

/// A conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    /// The thread's id.
    pub id: ThreadId,
    /// The tenant it belongs to.
    pub space_id: SpaceId,
    /// The bot whose conversation this is.
    pub bot_id: BotId,
    /// The member who owns it.
    pub user_id: UserId,
    /// The sequence the next message will take.
    pub next_message_seq: i64,
    /// The sequence the next event will take.
    pub next_event_seq: i64,
    /// When it was created.
    pub created_at: String,
}

/// The number of messages a page may hold at most.
///
/// A page is a page: the client asks for what it can render. The ceiling keeps one request
/// from asking for the whole history.
pub const MAX_PAGE_SIZE: usize = 100;

/// The number of messages a page holds when the caller does not say.
pub const DEFAULT_PAGE_SIZE: usize = 50;

/// Checks a requested page size.
///
/// # Errors
///
/// Returns [`crate::error::DomainError::InvalidPageSize`] when the size is zero or above
/// [`MAX_PAGE_SIZE`].
pub fn validate_page_size(size: usize) -> Result<(), crate::error::DomainError> {
    if size == 0 || size > MAX_PAGE_SIZE {
        return Err(crate::error::DomainError::InvalidPageSize {
            size,
            max: MAX_PAGE_SIZE,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, validate_page_size};

    #[test]
    fn req_005_a_page_has_a_ceiling_and_a_default() {
        assert_eq!(DEFAULT_PAGE_SIZE, 50);
        assert!(validate_page_size(DEFAULT_PAGE_SIZE).is_ok());
        assert!(validate_page_size(0).is_err());
        assert!(validate_page_size(MAX_PAGE_SIZE + 1).is_err());
    }
}
