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

mod space_keys;

pub mod artifact_files;
pub mod cdp;
pub mod computer;
pub mod config;
pub mod connectors;
pub mod crypto;
pub mod db;
pub mod dispatch;
pub mod events;
pub mod mcp_http;
pub mod mcp_stdio;
pub mod net_safety;
pub mod oauth;
pub mod openai;
pub mod plugin;
pub mod provider;
pub mod secret_box;
pub mod stores;
pub mod tenant;

pub use artifact_files::FileArtifactContentStore;
pub use cdp::{Cdp, CdpError};
pub use computer::{DockerComputerRuntime, NoneComputerRuntime};
pub use config::{ConfigError, DatabaseSettings};
pub use connectors::HttpConnectorRunner;
pub use crypto::{Argon2idHasher, IdGenerator, SystemClock, TokenGenerator};
pub use db::Postgres;
pub use dispatch::McpTransportDispatch;
pub use events::BroadcastPublisher;
pub use mcp_http::HttpMcpTransport;
pub use mcp_stdio::StdioMcpTransport;
pub use oauth::OAuthTokenClient;
pub use openai::{OpenAiCompatibleAdapter, OpenAiConfig};
pub use plugin::GitHubPluginFetcher;
pub use provider::ScriptedProvider;
pub use secret_box::{SecretBox, SecretBoxError};
pub use stores::accounts::PostgresAccountStore;
pub use stores::artifacts::PostgresArtifactStore;
pub use stores::audit::PostgresAuditStore;
pub use stores::bots::PostgresBotStore;
pub use stores::cards::{PostgresCardStore, PostgresEffectStore};
pub use stores::computer::PostgresComputerStore;
pub use stores::connectors::PostgresConnectorStore;
pub use stores::events::PostgresEventStore;
pub use stores::invitations::PostgresInvitationStore;
pub use stores::mcp::PostgresMcpServerStore;
pub use stores::memory::PostgresMemoryStore;
pub use stores::messages::PostgresMessageStore;
pub use stores::pages::PostgresPageStore;
pub use stores::pending_tool_calls::PostgresPendingToolCallStore;
pub use stores::plugin_installs::PostgresPluginInstallStore;
pub use stores::plugin_submissions::PostgresPluginSubmissionStore;
pub use stores::rate_limits::PostgresRateLimitStore;
pub use stores::reactions::PostgresReactionStore;
pub use stores::read_states::PostgresReadStateStore;
pub use stores::roster::PostgresRosterStore;
pub use stores::routines::PostgresRoutineStore;
pub use stores::runs::PostgresRunStore;
pub use stores::screen::PostgresScreenCapabilityStore;
pub use stores::secrets::{PostgresSecretStore, rekey_space_keys};
pub use stores::sessions::PostgresSessionStore;
pub use stores::skills::PostgresSkillStore;
pub use stores::spaces::PostgresSpaceStore;
pub use stores::threads::PostgresThreadStore;
pub use stores::tickets::PostgresTicketStore;
pub use stores::tools::PostgresToolStore;
pub use stores::users::PostgresUserStore;
pub use tenant::{TenantFuture, with_tenant};
