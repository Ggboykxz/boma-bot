//! Pure business rules for Boma Bot.
//!
//! This crate is the part of the system that must be provably correct: it holds no IO, no clock,
//! no async runtime and no database knowledge, so every rule can be exercised with a unit or
//! property test in microseconds. Adapters elsewhere translate the outside world into these types.
//!
//! The run state machine lives here because it is the spine of the product: a rebuild that gets
//! `queued → leased → running → …` wrong cannot be trusted with an agent that spends money.

#![deny(missing_docs)]

pub mod account;
pub mod bot;
pub mod card;
pub mod error;
pub mod event;
pub mod ids;
pub mod message;
pub mod model;
pub mod run;
pub mod tenancy;
pub mod thread;

pub use account::{Email, Session, Space, SpaceRole, User, validate_password};
pub use bot::Bot;
pub use card::{Card, CardId, CardKind, CardStatus, ExternalEffect, ToolKind, ToolSpec};
pub use error::DomainError;
pub use event::{Event, EventType};
pub use ids::{
    BotId, EventId, InviteId, MessageId, OrganizationId, RunId, SessionId, SpaceId, ThreadId,
    UserId,
};
pub use message::{
    Block, Message, MessagePage, MessageRole, cursor_from_seq, parse_cursor, validate_message_text,
};
pub use model::{DeploymentModels, ModelChoice, ModelPreferences, resolve};
pub use run::{Run, RunStatus, RunTrigger};
pub use tenancy::{SystemReason, TenantContext};
pub use thread::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, Thread, validate_page_size};
