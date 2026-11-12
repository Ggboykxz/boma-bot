//! The `boma` executable.
//!
//! Composition only: this crate reads configuration, starts logging, builds the adapters and hands
//! them to the API. It owns no business rule, which is why it is the one place allowed to use
//! `anyhow` (AGENTS.md "Rust standards").

#![deny(missing_docs)]

mod cli;
mod commands;
mod config;
mod probe;
mod telemetry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run().await
}
