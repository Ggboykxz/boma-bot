//! The testkit: an in-memory implementation of the ports of Boma Bot.
//!
//! The product's use cases never name an adapter: they ask for a store, a clock, an identifier
//! generator or a publisher. This crate answers those asks with rows in memory, so a test can
//! drive a use case without a database, without a network and without a clock that moves.
//!
//! It is made of three parts:
//!
//! * [`memory`] holds the stores: one type per trait family, all sharing the same rows, and all
//!   obeying the rules the database obeys (the sequences the store allocates, the idempotency of
//!   a nonce, the atomic claim of a run, the fence that guards a write and the isolation of the
//!   tenants).
//! * [`scripts`] holds the scripted services: the identifiers, the clock, the tokens and the
//!   password hasher a test drives.
//! * [`build`] holds [`TestApp`], which assembles an application from all of them.
//!
//! ```no_run
//! # use boma_testkit::TestApp;
//! # async fn example() {
//! let test = TestApp::builder().build();
//! let user_id = match test.seed_user("person@example.com", "correct horse battery").await {
//!     Ok(user_id) => user_id,
//!     Err(_) => return,
//! };
//! assert!(!user_id.as_str().is_empty());
//! # }
//! ```
//!
//! Everything here is a test double. The password hash is not a real hash, the clock does not
//! move on its own and the ids are readable: none of it may reach a deployment.

#![deny(missing_docs)]

pub mod build;
pub mod memory;
pub mod scripts;

pub use build::{TestApp, TestAppBuilder};
pub use memory::{
    MemoryAccountStore, MemoryBotStore, MemoryCardStore, MemoryComputerStore, MemoryDatabase,
    MemoryEffectStore, MemoryEventPublisher, MemoryEventStore, MemoryInvitationStore,
    MemoryMemoryStore, MemoryMessageStore, MemoryPendingToolCallStore, MemoryPluginSubmissionStore,
    MemoryRateLimitStore, MemoryReactionStore, MemoryReadStateStore, MemoryRosterStore,
    MemoryRoutineStore, MemoryRunStore, MemoryScreenCapabilityStore, MemorySecretStore,
    MemorySessionStore, MemorySpaceStore, MemoryThreadStore, MemoryToolStore, MemoryUserStore,
    Organization, RecordedSubmission,
};
pub use scripts::{
    ComputerAnswer, FixedHasher, ScriptedClock, ScriptedComputerRuntime, ScriptedIds,
    ScriptedProvider, ScriptedTokens,
};

/// The application crate, re-exported so that a test needs only one dependency.
pub use boma_app;

/// The domain crate, re-exported so that a test needs only one dependency.
pub use boma_domain;
