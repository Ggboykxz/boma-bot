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
            .map_err(|error| anyhow::anyhow!("installing the JSON log subscriber failed: {error}")),
        LogFormat::Pretty => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .try_init()
            .map_err(|error| anyhow::anyhow!("installing the log subscriber failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{LogFormat, TelemetrySettings, init};

    #[test]
    fn req_054_an_invalid_filter_is_an_error_and_not_a_silent_fallback() {
        let settings = TelemetrySettings {
            filter: "boma_api=not-a-level".to_owned(),
            format: LogFormat::Pretty,
        };
        let error = init(&settings).expect_err("an invalid filter must fail loudly");
        assert!(
            error.to_string().contains("BOMA_LOG_FILTER"),
            "the message must name the variable to fix: {error}"
        );
    }
}
