//! Sign-in requests (REQ-060, improvement program).

use super::{BotId, SpaceId, UserId};

/// One ask from a bot to sign in to a site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInRequest {
    /// The id of the request.
    pub id: String,
    /// The space it was asked in.
    pub space_id: SpaceId,
    /// The bot asking.
    pub bot_id: BotId,
    /// Who/what asked it (a member's user id, or a stored run's user context id).
    pub requested_by: UserId,
    /// Where in the room the question was filed.
    pub thread_id: Option<String>,
    /// The site origin (`example.com`).
    pub origin: String,
    /// How a person should read it (`the payment provider`).
    pub display_name: String,
    /// Where it stands today.
    pub status: SignInRequestStatus,
    /// Who answered it, when resolved.
    pub granted_by: Option<UserId>,
    /// When it was filed.
    pub created_at: String,
    /// When it was answered.
    pub updated_at: String,
}

/// What state a sign-in request is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInRequestStatus {
    /// Waiting for a member to answer.
    Pending,
    /// The member granted the bot access to the site.
    Granted,
    /// The member refused.
    Refused,
}

/// The ask is recorded before a member may answer it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSignInRequest {
    /// The id of the request; minted by the app.
    pub id: String,
    /// The space the ask lives in.
    pub space_id: SpaceId,
    /// The bot asking.
    pub bot_id: BotId,
    /// Who initiated the ask (the actor behind the tool call).
    pub requested_by: UserId,
    /// Where in the room the question was filed.
    pub thread_id: Option<String>,
    /// The site origin.
    pub origin: String,
    /// What a person should call it.
    pub display_name: String,
    /// When the ask was filed.
    pub created_at: String,
}
