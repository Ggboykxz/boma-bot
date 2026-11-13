//! Adapters for Boma Bot: Postgres, the filesystem and external providers.
//!
//! Every type here implements a port declared in `boma-app`. Nothing in this crate is imported by
//! `boma-domain`, and the only crate allowed to know a driver's name is this one.
//!
//! The adapters come in three families:
//!
//! * the **stores** ([`stores`]), one per port family, all of them Postgres;
//! * the **tenant plumbing** ([`tenant`]), which is the only place in the product that tells the
//!   database which tenant a query acts in (REQ-043);
//! * the **services** ([`crypto`], [`provider`], [`events`]), which need no database at all.

#![deny(missing_docs)]

pub mod cdp;
pub mod computer;
pub mod config;
pub mod crypto;
pub mod db;
pub mod events;
pub mod openai;
pub mod provider;
pub mod stores;
pub mod tenant;

pub use cdp::{Cdp, CdpError};
pub use computer::{DockerComputerRuntime, NoneComputerRuntime};
pub use config::{ConfigError, DatabaseSettings};
pub use crypto::{Argon2idHasher, IdGenerator, SystemClock, TokenGenerator};
pub use db::Postgres;
pub use events::BroadcastPublisher;
pub use openai::{OpenAiCompatibleAdapter, OpenAiConfig};
pub use provider::ScriptedProvider;
pub use stores::accounts::PostgresAccountStore;
pub use stores::bots::PostgresBotStore;
pub use stores::cards::{PostgresCardStore, PostgresEffectStore};
pub use stores::computer::PostgresComputerStore;
pub use stores::events::PostgresEventStore;
pub use stores::invitations::PostgresInvitationStore;
pub use stores::memory::PostgresMemoryStore;
pub use stores::messages::PostgresMessageStore;
pub use stores::pending_tool_calls::PostgresPendingToolCallStore;
pub use stores::rate_limits::PostgresRateLimitStore;
pub use stores::reactions::PostgresReactionStore;
pub use stores::read_states::PostgresReadStateStore;
pub use stores::roster::PostgresRosterStore;
pub use stores::routines::PostgresRoutineStore;
pub use stores::runs::PostgresRunStore;
pub use stores::screen::PostgresScreenCapabilityStore;
pub use stores::sessions::PostgresSessionStore;
pub use stores::spaces::PostgresSpaceStore;
pub use stores::threads::PostgresThreadStore;
pub use stores::tools::PostgresToolStore;
pub use stores::users::PostgresUserStore;
pub use tenant::{TenantFuture, with_tenant};
