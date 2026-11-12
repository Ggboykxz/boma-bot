//! Routines: cron definitions that wake a bot (REQ-023).
//!
//! A routine is a prompt plus one or more cron expressions and a timezone. The next wake is
//! computed in the routine's own timezone and stored as UTC; `@once` is a one-shot that fires
//! and deactivates instead of rescheduling.

use std::str::FromStr as _;

use chrono::{DateTime, Utc};
use croner::Cron;

use crate::DomainError;

/// The special sentinel a routine carries to fire exactly once.
pub const ONCE: &str = "@once";

/// The longest routine name a member may write.
pub const MAX_ROUTINE_NAME_LENGTH: usize = 120;

/// The longest routine prompt.
pub const MAX_ROUTINE_PROMPT_LENGTH: usize = 16_000;

/// The most cron expressions one routine carries.
pub const MAX_CRONS: usize = 8;

/// One parsed routine trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutineTrigger {
    /// One cron expression.
    Cron(Box<Cron>),
    /// Fires exactly once and deactivates.
    Once,
}

/// Every parsed trigger of a routine, in declaration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineTriggers {
    /// The trigger list.
    pub triggers: Vec<RoutineTrigger>,
}

/// Checks the name of a routine.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the name is empty, one line, or too long.
pub fn validate_routine_name(name: &str) -> Result<(), DomainError> {
    if name.trim().is_empty()
        || name.contains('\n')
        || name.chars().count() > MAX_ROUTINE_NAME_LENGTH
    {
        return Err(DomainError::InvalidName(
            "the routine name is empty, multi-line, or too long".to_owned(),
        ));
    }
    Ok(())
}

/// Checks the prompt of a routine.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the prompt is empty or too long.
pub fn validate_routine_prompt(prompt: &str) -> Result<(), DomainError> {
    if prompt.trim().is_empty() || prompt.chars().count() > MAX_ROUTINE_PROMPT_LENGTH {
        return Err(DomainError::InvalidName(
            "the prompt is empty or too long".to_owned(),
        ));
    }
    Ok(())
}

/// Checks the timezone of a routine.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the value is not a known IANA zone.
pub fn validate_timezone(timezone: &str) -> Result<(), DomainError> {
    timezone.parse::<chrono_tz::Tz>().map_err(|_| {
        DomainError::InvalidName(format!("`{timezone}` is not a known IANA timezone"))
    })?;
    Ok(())
}

/// Parses one trigger entry; `@once` is the single one-shot sentinel.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the entry is neither `@once` nor a valid cron
/// expression.
pub fn parse_trigger(entry: &str) -> Result<RoutineTrigger, DomainError> {
    if entry == ONCE {
        return Ok(RoutineTrigger::Once);
    }
    Cron::from_str(entry)
        .map(|cron| RoutineTrigger::Cron(Box::new(cron)))
        .map_err(|_| DomainError::InvalidName(format!("`{entry}` is not a cron expression")))
}

/// Checks a list of triggers.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the list is empty, carries too many entries, or a
/// trigger is invalid.
pub fn validate_triggers(crons: &[String]) -> Result<(), DomainError> {
    if crons.is_empty() || crons.len() > MAX_CRONS {
        return Err(DomainError::InvalidName(
            "the routine carries no cron expressions, or too many".to_owned(),
        ));
    }
    for entry in crons {
        parse_trigger(entry)?;
    }
    Ok(())
}

/// The instant a routine wakes next, or none when it never wakes.
///
/// `@once` fires five minutes after `now` and then deactivates. Everything else takes the
/// earliest match across the triggers, computed in the routine's timezone and returned as a
/// UTC RFC3339 instant.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when a trigger is invalid.
pub fn next_run_at(crons: &[String], timezone: &str, now: &str) -> Result<String, DomainError> {
    let triggers = crons
        .iter()
        .map(|entry| parse_trigger(entry))
        .collect::<Result<Vec<_>, _>>()?;
    if triggers
        .iter()
        .any(|trigger| trigger == &RoutineTrigger::Once)
    {
        // The one-shot fires five minutes after the request lands.
        let instant = parse_instant(now)?;
        let plus = instant + chrono::Duration::minutes(5);
        return Ok(format_utc(plus));
    }
    validate_timezone(timezone)?;
    let zone: chrono_tz::Tz = timezone.parse().map_err(|_| {
        DomainError::InvalidName(format!("`{timezone}` is not a known IANA timezone"))
    })?;
    let now = parse_instant(now)?.with_timezone(&zone);
    let mut earliest: Option<DateTime<chrono_tz::Tz>> = None;
    for cron in triggers {
        let RoutineTrigger::Cron(cron) = cron else {
            continue;
        };
        let next = cron
            .find_next_occurrence(&now, false)
            .map_err(|_| DomainError::InvalidName("the cron expression never fires".to_owned()))?;
        match earliest {
            Some(at) if at <= next => {}
            _ => earliest = Some(next),
        }
    }
    let next = earliest
        .ok_or_else(|| DomainError::InvalidName("the routine has no next run".to_owned()))?;
    Ok(format_utc(next.with_timezone(&Utc)))
}

/// Parses an RFC3339 instant; the clock only writes RFC3339.
fn parse_instant(instant: &str) -> Result<DateTime<Utc>, DomainError> {
    instant
        .parse::<DateTime<Utc>>()
        .map_err(|_| DomainError::InvalidName(format!("`{instant}` is not an RFC3339 instant")))
}

/// Formats an instant as RFC3339 with `Z`.
fn format_utc(instant: DateTime<Utc>) -> String {
    instant.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-01-01T00:00:00Z";

    #[test]
    fn req_023_a_weekday_cron_lands_on_that_weekday_in_paris_time() {
        // 2026-01-01 is a Thursday. At midnight UTC (01:00 Paris), the 8am weekday slot
        // that is already past must not be offered.
        let next = next_run_at(
            &["0 8 * * 1-5".to_owned()],
            "Europe/Paris",
            "2026-01-01T09:00:00Z",
        )
        .expect("the next run computes");
        assert_eq!(next, "2026-01-02T07:00:00Z");
    }

    #[test]
    fn req_023_the_once_sentinel_fires_five_minutes_later() {
        let next = next_run_at(&[ONCE.to_owned()], "UTC", NOW).expect("the next run computes");
        assert_eq!(next, "2026-01-01T00:05:00Z");
    }

    #[test]
    fn req_023_a_bad_timezone_is_refused() {
        assert!(next_run_at(&["0 8 * * *".to_owned()], "Nowhere/Special", NOW).is_err());
    }

    #[test]
    fn req_023_a_bad_cron_is_refused() {
        assert!(next_run_at(&["at eight".to_owned()], "UTC", NOW).is_err());
    }
}
