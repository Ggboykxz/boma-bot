//! Bots backed by an external agent executor (REQ-059, improvement program).

use super::{BotId, SpaceId};

/// One external executor attached to a bot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAgent {
    /// The bot that wears this connection.
    pub bot_id: BotId,
    /// The tenant the bot lives in; reads are tenant-scoped.
    pub space_id: SpaceId,
    /// The AG-UI executor base address — operator-supplied.
    pub base_url: String,
    /// Which agent the executor should run, when it exposes several.
    pub agent_name: Option<String>,
    /// Name of the space's bot-secret carrying the bearer, when one is configured.
    pub secret_name: Option<String>,
    /// When this connection was saved.
    pub created_at: String,
    /// When this connection was last updated.
    pub updated_at: String,
}

/// What is stored to connect a bot to an external executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewExternalAgent {
    /// The bot that will wear this connection.
    pub bot_id: BotId,
    /// The tenant the bot lives in.
    pub space_id: SpaceId,
    /// The AG-UI executor base address — operator-supplied.
    pub base_url: String,
    /// Which agent the executor should run, when it exposes several.
    pub agent_name: Option<String>,
    /// Name of the space's bot-secret carrying the bearer, when one is configured.
    pub secret_name: Option<String>,
    /// When this connection is being saved.
    pub saved_at: String,
}
