//! Time helpers shared across crates (REQ-064).

/// A deterministic RFC3339 stamp of the current instant, second precision.
#[must_use]
pub fn format_now() -> String {
    boma_domain::pipeline_now_string()
}

/// Today's UTC day of month prefix `YYYY-MM-DD`, for day-windowed counters.
#[must_use]
pub fn format_today() -> String {
    let at = time::OffsetDateTime::now_utc();
    format!("{:04}-{:02}-{:02}", at.year(), at.month() as u8, at.day())
}
