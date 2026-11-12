//! The HTTP surface: routing, DTOs, OpenAPI and error mapping.
//!
//! This crate is the only place that knows an HTTP status code. It calls `boma-app` and never
//! `boma-infra`: a handler that reached for the database directly would bypass tenancy, limits and
//! the approval gate.

#![deny(missing_docs)]

pub mod config;
pub mod error;
pub mod health;
pub mod openapi;
pub mod router;
pub mod state;

pub use config::{HttpConfig, HttpConfigError};
pub use error::{ApiError, ErrorBody, ErrorCode};
pub use router::router;
pub use state::AppState;
