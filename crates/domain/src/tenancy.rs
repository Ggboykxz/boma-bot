//! The tenant context.
//!
//! Every user-initiated query runs with a tenant context, and the database enforces it: the
//! rows carry a `space_id`, the policies compare it with the one the context set, and a
//! query that forgets to set the context sees nothing (REQ-043).
//!
//! The only way out is a *system* path, and the reason is an enum rather than a string: a
//! free-text reason would let any caller claim any exception (defect D-09).

use crate::{
    error::DomainError,
    ids::{SpaceId, UserId},
};

/// Why a query runs without a tenant context.
///
/// Each variant is a code path the product owns. There is no free-text variant, on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemReason {
    /// The process starts and reads its own state.
    Boot,
    /// Authentication looks a user up before a tenant exists.
    Authentication,
    /// An inbound webhook identifies itself and is then scoped to its space.
    WebhookDelivery,
    /// A scheduled job runs for the space it belongs to.
    ScheduledJob,
    /// A migration reads across tenants.
    DataMigration,
}

impl SystemReason {
    /// Every reason, in the order the product declares them.
    pub const ALL: [Self; 5] = [
        Self::Boot,
        Self::Authentication,
        Self::WebhookDelivery,
        Self::ScheduledJob,
        Self::DataMigration,
    ];

    /// The wire value, used in the audit log.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boot => "boot",
            Self::Authentication => "authentication",
            Self::WebhookDelivery => "webhook_delivery",
            Self::ScheduledJob => "scheduled_job",
            Self::DataMigration => "data_migration",
        }
    }

    /// Parses a wire value.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::UnknownSystemReason`] for a value outside the vocabulary.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "boot" => Ok(Self::Boot),
            "authentication" => Ok(Self::Authentication),
            "webhook_delivery" => Ok(Self::WebhookDelivery),
            "scheduled_job" => Ok(Self::ScheduledJob),
            "data_migration" => Ok(Self::DataMigration),
            other => Err(DomainError::UnknownSystemReason(other.to_owned())),
        }
    }
}

/// Who is asking, and in which tenant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TenantContext {
    /// A signed-in member acting inside one space.
    Member {
        /// The space every query is limited to.
        space_id: SpaceId,
        /// The member.
        user_id: UserId,
    },
    /// A system path that is allowed to cross tenants, for a reason the product owns.
    System {
        /// Why the query crosses tenants.
        reason: SystemReason,
        /// The member the path acts on behalf of, when there is one.
        user_id: Option<UserId>,
    },
}

impl TenantContext {
    /// The space the context is limited to, when it is limited to one.
    #[must_use]
    pub const fn space_id(&self) -> Option<&SpaceId> {
        match self {
            Self::Member { space_id, .. } => Some(space_id),
            Self::System { .. } => None,
        }
    }

    /// The member the context acts for, when there is one.
    #[must_use]
    pub const fn user_id(&self) -> Option<&UserId> {
        match self {
            Self::Member { user_id, .. } => Some(user_id),
            Self::System { user_id, .. } => user_id.as_ref(),
        }
    }

    /// Whether the context may cross tenants.
    #[must_use]
    pub const fn is_system(&self) -> bool {
        matches!(self, Self::System { .. })
    }

    /// The reason the context crosses tenants, when it does.
    #[must_use]
    pub const fn system_reason(&self) -> Option<SystemReason> {
        match self {
            Self::System { reason, .. } => Some(*reason),
            Self::Member { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SystemReason, TenantContext};
    use crate::ids::{SpaceId, UserId};

    #[test]
    fn req_043_a_member_context_is_limited_to_one_space() {
        let context = TenantContext::Member {
            space_id: SpaceId::new("spc_1").expect("an id is accepted"),
            user_id: UserId::new("usr_1").expect("an id is accepted"),
        };
        assert_eq!(
            context.space_id().map(crate::ids::SpaceId::as_str),
            Some("spc_1")
        );
        assert!(!context.is_system());
        assert_eq!(context.system_reason(), None);
    }

    #[test]
    fn req_043_a_system_context_always_carries_an_enum_reason() {
        for reason in SystemReason::ALL {
            let context = TenantContext::System {
                reason,
                user_id: None,
            };
            assert!(context.is_system());
            assert_eq!(context.space_id(), None);
            assert_eq!(context.system_reason(), Some(reason));
            assert_eq!(
                SystemReason::parse(reason.as_str()).expect("a known reason"),
                reason
            );
        }
        assert!(
            SystemReason::parse("because").is_err(),
            "a free-text reason is refused"
        );
    }
}
