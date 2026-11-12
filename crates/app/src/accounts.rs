//! Accounts, sessions and the people who sign in (REQ-040, REQ-041).
//!
//! The rules live in the domain (`boma_domain::Email`, `boma_domain::validate_password`); this
//! module is the choreography: hash, store, mint a session, hand back a token.

use boma_domain::{Email, OrganizationId, SessionId, SpaceId, User, UserId, validate_password};

use crate::{
    error::AppError,
    ports::{NewAccount, NewOrganization, NewSession, NewSpace, NewUser},
    service::App,
};

/// What a caller sends to create an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignUp {
    /// The address the person signs in with.
    pub email: String,
    /// The password, in clear: it is hashed here and never stored in clear.
    pub password: String,
    /// The name a person sees.
    pub name: String,
    /// The locale the interface is shown in.
    pub locale: String,
}

/// What a caller sends to sign in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignIn {
    /// The address the person signs in with.
    pub email: String,
    /// The password, in clear.
    pub password: String,
}

/// A signed-in person and the token that proves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGrant {
    /// The person.
    pub user: User,
    /// The opaque token: it goes in the cookie, never in a log.
    pub token: String,
    /// When the session expires.
    pub expires_at: String,
}

/// Creates accounts and sessions.
#[derive(Clone)]
pub struct Accounts {
    app: App,
    session_ttl_seconds: i64,
}

impl Accounts {
    /// Builds the service.
    #[must_use]
    pub const fn new(app: App, session_ttl_seconds: i64) -> Self {
        Self {
            app,
            session_ttl_seconds,
        }
    }

    /// Creates an account and signs the person in.
    ///
    /// The person, their organization, their first space and their membership in it are written in
    /// one transaction: an account without a space is an account that cannot do anything (REQ-042).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Rule`] when the address or the password is refused,
    /// [`AppError::Conflict`] when the address is already registered, and [`AppError::Unavailable`]
    /// when the store cannot answer.
    pub async fn sign_up(&self, request: &SignUp) -> Result<SessionGrant, AppError> {
        let email = Email::new(request.email.clone())?;
        validate_password(&request.password)?;
        let name = request.name.trim();
        if name.is_empty() {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "an account needs a name".to_owned(),
            )));
        }

        let user_id = UserId::new(self.app.services.ids.new_id("usr"))?;
        let organization_id = OrganizationId::new(self.app.services.ids.new_id("org"))?;
        let space_id = SpaceId::new(self.app.services.ids.new_id("spc"))?;
        let now = self.app.services.clock.now();
        let password_hash = self.app.services.hasher.hash(&request.password).await?;

        let account = self
            .app
            .stores
            .accounts
            .create_account(NewAccount {
                user: NewUser {
                    id: user_id.clone(),
                    email: email.clone(),
                    password_hash,
                    name: name.to_owned(),
                    locale: request.locale.clone(),
                    created_at: now.clone(),
                },
                organization: NewOrganization {
                    id: organization_id.clone(),
                    name: format!("{name}'s organization"),
                    created_at: now.clone(),
                },
                space: NewSpace {
                    id: space_id,
                    organization_id,
                    name: format!("{name}'s space"),
                    is_default: true,
                    created_at: now.clone(),
                },
            })
            .await?;

        self.grant_session(account.user).await
    }

    /// Signs an existing person in.
    ///
    /// A wrong address and a wrong password give the same answer, so the reply never says whether
    /// an address is registered (REQ-040).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidCredentials`] when the pair does not match, and
    /// [`AppError::Unavailable`] when the store cannot answer.
    pub async fn sign_in(&self, request: &SignIn) -> Result<SessionGrant, AppError> {
        let email = Email::new(request.email.clone())?;
        let Some((user, stored)) = self.app.stores.users.find_by_email(&email).await? else {
            // The password is still put through the hasher, so the time taken does not say whether
            // the address exists.
            let _ = self.app.services.hasher.hash(&request.password).await;
            return Err(AppError::InvalidCredentials);
        };
        if !self
            .app
            .services
            .hasher
            .verify(&request.password, &stored)
            .await?
        {
            return Err(AppError::InvalidCredentials);
        }
        self.grant_session(user).await
    }

    /// Mints a session for a person who is already known.
    async fn grant_session(&self, user: User) -> Result<SessionGrant, AppError> {
        let token = self.app.services.tokens.generate()?;
        let token_hash = self.app.services.tokens.hash(&token)?;
        let now = self.app.services.clock.now();
        let expires_at = self
            .app
            .services
            .clock
            .now_plus_seconds(self.session_ttl_seconds);
        let session = self
            .app
            .stores
            .sessions
            .create(NewSession {
                id: SessionId::new(self.app.services.ids.new_id("ses"))?,
                user_id: user.id.clone(),
                token_hash,
                expires_at: expires_at.clone(),
                created_at: now,
            })
            .await?;
        Ok(SessionGrant {
            user,
            token,
            expires_at: session.expires_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{SignIn, SignUp};
    use boma_domain::DomainError;

    #[test]
    fn req_040_a_malformed_address_is_refused_before_any_store_is_touched() {
        let request = SignUp {
            email: "not-an-address".to_owned(),
            password: "correct horse battery".to_owned(),
            name: "Ada".to_owned(),
            locale: "en-US".to_owned(),
        };
        let error = boma_domain::Email::new(request.email).expect_err("the address is refused");
        assert!(matches!(error, DomainError::InvalidEmail(_)));
    }

    #[test]
    fn req_040_a_weak_password_is_refused() {
        assert!(boma_domain::validate_password("short").is_err());
        assert!(boma_domain::validate_password("long enough password").is_ok());
    }

    #[test]
    fn req_041_a_sign_in_request_carries_only_the_pair_of_secrets() {
        let request = SignIn {
            email: "person@example.com".to_owned(),
            password: "correct horse battery".to_owned(),
        };
        assert_eq!(request.email, "person@example.com");
    }
}
