//! Use cases and ports for Boma Bot.
//!
//! The `app` crate owns what the product *does*: it composes the pure rules from `boma-domain`
//! and declares the ports (traits) it needs from the outside world. It never names a concrete
//! adapter, so a test can drive a use case with a fake, and `boma-infra` can be swapped without
//! touching a business rule.

#![deny(missing_docs)]

pub mod error;
pub mod health;

pub use error::AppError;
pub use health::{Capabilities, Readiness, ReadinessCheck, ReadinessReport};
