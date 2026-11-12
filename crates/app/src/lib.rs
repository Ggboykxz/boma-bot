//! Use cases and ports for Boma Bot: orchestration without a concrete adapter.
//!
//! The `app` crate owns what the product *does*: it composes the pure rules from `boma-domain`
//! and declares the ports (traits) it needs from the outside world. It never names a concrete
//! adapter, so a test can drive a use case with a fake, and `boma-infra` can be swapped without
//! touching a business rule.

#![deny(missing_docs)]

pub mod accounts;
pub mod error;
pub mod health;
pub mod ports;
pub mod service;
pub mod usecases;
pub mod worker;

pub use accounts::{Accounts, SessionGrant, SignIn, SignUp};
pub use error::AppError;
pub use health::{Capabilities, Readiness, ReadinessCheck, ReadinessReport};
pub use service::{Actor, App, BotWithThread, Failure, SendOutcome, Services, Stores};
pub use worker::Worker;
