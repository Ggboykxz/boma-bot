//! Adapters for Boma Bot: Postgres, the filesystem and external providers.
//!
//! Every type here implements a port declared in `boma-app`. Nothing in this crate is imported by
//! `boma-domain`, and the only crate allowed to know a driver's name is this one.

#![deny(missing_docs)]

pub mod config;
pub mod db;

pub use config::{ConfigError, DatabaseSettings};
pub use db::Postgres;
