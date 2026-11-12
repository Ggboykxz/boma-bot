//! Opaque identifiers.
//!
//! Every id is an opaque string: the spec says ids "must not be treated as ordered or
//! guessable", so nothing in the product may rely on their shape. The newtypes exist so a
//! bot id cannot be passed where a thread id is expected.

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Declares an opaque identifier type.
#[macro_export]
macro_rules! opaque_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Wraps an identifier that came from outside.
            ///
            /// The value is not validated: ids are opaque and their format is the storage's
            /// business. An empty id is rejected, because that is always a bug.
            ///
            /// # Errors
            ///
            /// Returns [`DomainError::EmptyId`] when the value is blank.
            pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(DomainError::EmptyId(stringify!($name)));
                }
                Ok(Self(value))
            }

            /// The identifier as the string the wire and the database use.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

opaque_id!(
    /// A routine.
    RoutineId
);

opaque_id!(
    /// A person.
    UserId
);
opaque_id!(
    /// A session.
    SessionId
);
opaque_id!(
    /// A billing entity that owns spaces.
    OrganizationId
);
opaque_id!(
    /// A tenant. Every aggregate belongs to exactly one.
    SpaceId
);
opaque_id!(
    /// A bot.
    BotId
);
opaque_id!(
    /// A conversation.
    ThreadId
);
opaque_id!(
    /// A message in a thread.
    MessageId
);
opaque_id!(
    /// One execution of the agent.
    RunId
);
opaque_id!(
    /// One event in a thread's stream.
    EventId
);
opaque_id!(
    /// An invitation to a space.
    InviteId
);

#[cfg(test)]
mod tests {
    use super::{BotId, ThreadId};

    #[test]
    fn req_043_an_identifier_is_opaque_and_cannot_be_empty() {
        let bot = BotId::new("bot_123").expect("an id is accepted");
        assert_eq!(bot.as_str(), "bot_123");
        assert_eq!(
            bot.as_str(),
            ThreadId::new("bot_123")
                .expect("an id is accepted")
                .as_str()
        );
        assert!(BotId::new("  ").is_err(), "an empty id is a bug");
    }
}
