//! Bots backed by a local ACP command-line agent (REQ-061, improvement program).
//!
//! A local executor is a CLI on the machine running the deployment, spoken over the Agent Client
//! Protocol (newline-delimited JSON-RPC on stdio). One row per bot; absence = the bot does not
//! run ACP. The known executor registry lives in the application layer — the domain only names
//! the reference.

use super::{BotId, SpaceId};

/// One local ACP executor attached to a bot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpAgent {
    /// The bot that wears this attachment.
    pub bot_id: BotId,
    /// The tenant the bot lives in; reads are tenant-scoped.
    pub space_id: SpaceId,
    /// The known executor id (e.g. `opencode`); validated against the registry at the boundary.
    pub executor: String,
    /// When this attachment was saved.
    pub created_at: String,
    /// When this attachment was last updated.
    pub updated_at: String,
}

/// What is stored to attach a bot to a local ACP executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAcpAgent {
    /// The bot that will wear this attachment.
    pub bot_id: BotId,
    /// The tenant the bot lives in.
    pub space_id: SpaceId,
    /// The known executor id; the application layer refuses anything not in the registry.
    pub executor: String,
    /// When this attachment is being saved.
    pub saved_at: String,
}
