//! Memory documents (REQ-022).
//!
//! A memory document is addressed by (tenant, scope, owner, path). Its content is the projection
//! of the newest revision; revisions are append-only and revision numbers only grow.

use serde::{Deserialize, Serialize};

use crate::{BotId, DomainError, SpaceId};

/// Where a memory document lives: one bot, or the whole space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryScope {
    /// Owned by one bot inside the space.
    Bot,
    /// Readable by (and owned by) the space.
    Shared,
}

impl MemoryScope {
    /// The wire value of the scope.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Bot => "bot",
            Self::Shared => "shared",
        }
    }
}
impl std::str::FromStr for MemoryScope {
    type Err = DomainError;

    /// Parses a scope from the wire.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidName`] when the value is neither `bot` nor `shared`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "bot" => Ok(Self::Bot),
            "shared" => Ok(Self::Shared),
            other => Err(DomainError::InvalidName(format!(
                "the memory scope `{other}` is not `bot` or `shared`"
            ))),
        }
    }
}

/// The longest path a memory document may carry.
pub const MAX_MEMORY_PATH_LENGTH: usize = 512;

/// Checks the path of a memory document.
///
/// Paths are relative and POSIX-like: no absolute prefix, no `.` or `..` segments, no empty
/// segments, no backslashes and no control characters. The rule exists so a bot can never make
/// a path that reads as escaping the memory tree.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the path is refused.
pub fn validate_memory_path(path: &str) -> Result<(), DomainError> {
    if path.is_empty() || path.len() > MAX_MEMORY_PATH_LENGTH {
        return Err(DomainError::InvalidName(
            "the memory path is empty or too long".to_owned(),
        ));
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err(DomainError::InvalidName(
            "the memory path is not relative".to_owned(),
        ));
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(DomainError::InvalidName(
                "the memory path carries an empty or dotted segment".to_owned(),
            ));
        }
        if segment.chars().any(|c| c.is_control() || c == '\\') {
            return Err(DomainError::InvalidName(
                "the memory path carries a control character or a backslash".to_owned(),
            ));
        }
    }
    Ok(())
}

/// A memory document, as the store carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDocument {
    /// The tenant it belongs to.
    pub space_id: SpaceId,
    /// The bot it belongs to, or none for the space-shared memory.
    pub bot_id: Option<BotId>,
    /// Its path, relative and POSIX-like.
    pub path: String,
    /// The content at the newest revision.
    pub content: String,
    /// The revision the content is at; grows by one per write.
    pub revision: u64,
    /// When it was last written.
    pub updated_at: String,
}

impl MemoryDocument {
    /// The scope the document lives in.
    #[must_use]
    pub fn scope(&self) -> MemoryScope {
        if self.bot_id.is_some() {
            MemoryScope::Bot
        } else {
            MemoryScope::Shared
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr as _;

    #[test]
    fn req_022_a_valid_memory_path_is_accepted() {
        for path in ["notes/commits.md", "a", "a/b/c/d.txt", "notes.md"] {
            validate_memory_path(path).expect("the path is accepted");
        }
    }

    #[test]
    fn req_022_a_dotted_segment_is_refused() {
        for path in ["../secrets.md", "a/../b", "a/./b", "./notes.md", "a//b"] {
            assert!(validate_memory_path(path).is_err(), "{path} is refused");
        }
    }

    #[test]
    fn req_022_an_absolute_or_trailing_path_is_refused() {
        for path in ["/etc/passwd", "a/", "", "a\\b.md"] {
            assert!(validate_memory_path(path).is_err(), "{path} is refused");
        }
    }

    #[test]
    fn req_022_a_control_character_is_refused() {
        assert!(validate_memory_path("a/\nb.md").is_err());
    }

    #[test]
    fn req_022_the_scope_round_trips() {
        for (raw, scope) in [("bot", MemoryScope::Bot), ("shared", MemoryScope::Shared)] {
            use std::str::FromStr as _;
            assert_eq!(MemoryScope::from_str(raw).expect("parsed"), scope);
            assert_eq!(scope.as_str(), raw);
        }
        assert!(MemoryScope::from_str("entries").is_err());
    }
}
