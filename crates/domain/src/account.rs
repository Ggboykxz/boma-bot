//! Accounts, spaces and the rules that govern them.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    error::DomainError,
    ids::{OrganizationId, SpaceId, UserId},
};

/// An e-mail address, validated once and carried everywhere else.
///
/// The shape is deliberately simple: a local part, an `@`, a domain with at least one dot.
/// Anything stricter belongs to the deployment's configuration, not to the domain.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Email(String);

impl Email {
    /// Validates an address.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidEmail`] when the address is empty, has no `@`, has an
    /// empty local part or has a domain without a dot.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let trimmed = value.trim();
        let Some((local, domain)) = trimmed.split_once('@') else {
            return Err(DomainError::InvalidEmail(trimmed.to_owned()));
        };
        if local.is_empty() || domain.is_empty() || !domain.contains('.') {
            return Err(DomainError::InvalidEmail(trimmed.to_owned()));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The address as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The shortest password the product accepts.
///
/// Eight characters is the floor, not the goal: the deployment may demand more, and a longer
/// password is always allowed.
pub const MIN_PASSWORD_LENGTH: usize = 8;

/// The longest password the product accepts.
///
/// Argon2 hashes the whole input, so an unbounded password is a memory and CPU cost an
/// anonymous caller can choose. 256 bytes is far above any real passphrase.
pub const MAX_PASSWORD_LENGTH: usize = 256;

/// Checks a password against the product's floor.
///
/// # Errors
///
/// Returns [`DomainError::WeakPassword`] when the password is shorter than
/// [`MIN_PASSWORD_LENGTH`] or longer than [`MAX_PASSWORD_LENGTH`].
pub fn validate_password(password: &str) -> Result<(), DomainError> {
    let length = password.chars().count();
    if length < MIN_PASSWORD_LENGTH {
        return Err(DomainError::WeakPassword {
            reason: format!("a password needs at least {MIN_PASSWORD_LENGTH} characters"),
        });
    }
    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(DomainError::WeakPassword {
            reason: format!("a password cannot exceed {MAX_PASSWORD_LENGTH} bytes"),
        });
    }
    Ok(())
}

/// What a member may do in a space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpaceRole {
    /// Full control, including deletion and member management.
    Owner,
    /// Manages members and settings.
    Admin,
    /// Uses the space.
    Member,
}

impl SpaceRole {
    /// Every role, from the strongest to the weakest.
    pub const ALL: [Self; 3] = [Self::Owner, Self::Admin, Self::Member];

    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    /// Parses a wire value.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::UnknownRole`] for a value outside the vocabulary.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            other => Err(DomainError::UnknownRole(other.to_owned())),
        }
    }

    /// Whether this role may do what the other role may do.
    #[must_use]
    pub const fn covers(self, other: Self) -> bool {
        (self as u8) <= (other as u8)
    }
}

impl fmt::Display for SpaceRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A tenant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Space {
    /// The tenant's id.
    pub id: SpaceId,
    /// The billing entity that owns it.
    pub organization_id: OrganizationId,
    /// The name a person sees.
    pub name: String,
    /// Whether this is the space a member lands in.
    pub is_default: bool,
}

impl Space {
    /// Checks the rules a space must satisfy.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidName`] when the name is empty or longer than 120
    /// characters.
    pub fn validate(name: &str) -> Result<(), DomainError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(DomainError::InvalidName("a space needs a name".to_owned()));
        }
        if trimmed.chars().count() > 120 {
            return Err(DomainError::InvalidName(
                "a space name cannot exceed 120 characters".to_owned(),
            ));
        }
        Ok(())
    }
}

/// A person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// The person's id.
    pub id: UserId,
    /// The address they sign in with.
    pub email: Email,
    /// The name a person sees.
    pub name: String,
    /// Whether the address has been proven.
    pub email_verified: bool,
    /// The locale the interface is shown in.
    pub locale: String,
    /// When the account was deactivated, if it was.
    pub deactivated_at: Option<String>,
}

/// A session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The session's id.
    pub id: crate::ids::SessionId,
    /// The person it belongs to.
    pub user_id: UserId,
    /// When it expires.
    pub expires_at: String,
}

#[cfg(test)]
mod tests {
    use super::{Email, SpaceRole, validate_password};

    #[test]
    fn req_040_an_email_is_validated_once_and_carried_everywhere_else() {
        let email = Email::new("  person@example.com ").expect("a valid address");
        assert_eq!(email.as_str(), "person@example.com");
        for invalid in ["", "person", "person@", "@example.com", "person@example"] {
            assert!(Email::new(invalid).is_err(), "{invalid} must be refused");
        }
    }

    #[test]
    fn req_040_a_password_has_a_floor_and_a_ceiling() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("longenough").is_ok());
        assert!(validate_password(&"x".repeat(4096)).is_err());
    }

    #[test]
    fn req_042_roles_are_ordered_from_the_strongest_to_the_weakest() {
        assert!(SpaceRole::Owner.covers(SpaceRole::Member));
        assert!(SpaceRole::Admin.covers(SpaceRole::Member));
        assert!(!SpaceRole::Member.covers(SpaceRole::Owner));
        assert_eq!(
            SpaceRole::parse("owner").expect("a known role"),
            SpaceRole::Owner
        );
        assert!(SpaceRole::parse("superuser").is_err());
    }
}
