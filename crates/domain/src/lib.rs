//! Pure business rules for Boma Bot.
//!
//! This crate is the part of the system that must be provably correct: it holds no IO, no clock,
//! no async runtime and no database knowledge, so every rule can be exercised with a unit or
//! property test in microseconds. Adapters elsewhere translate the outside world into these types.
//!
//! The run state machine lives here because it is the spine of the product: a rebuild that gets
//! `queued → leased → running → …` wrong cannot be trusted with an agent that spends money.

#![deny(missing_docs)]

pub mod error;
pub mod run;

pub use error::DomainError;
pub use run::{RunStatus, RunTrigger};
