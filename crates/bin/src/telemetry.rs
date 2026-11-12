//! Structured logging, initialised once.

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use crate::config::{LogFormat, TelemetrySettings};

/// Installs the global subscriber.
///
/// A filter expression that does not parse is an error rather than a silent fallback: a deployment
/// that asked for `info,boma_api=debug` and got `info` would hide the very logs it wanted.
///
/// # Errors
///
/// Returns an error when the filter is invalid or a subscriber is already installed.
pub fn init(settings: &TelemetrySettings) -> anyhow::Result<()> {
    let filter = EnvFilter::try_new(&settings.filter)
        .with_context(|| format!("BOMA_LOG_FILTER is not a valid filter: {}", settings.filter))?;
    match settings.format {
        LogFormat::Json => tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .with_current_span(true)
            .with_span_list(true)
            .try_init()
            .context("installing the JSON log subscriber failed"),
        LogFormat::Pretty => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .try_init()
            .context("installing the log subscriber failed"),
    }
}
