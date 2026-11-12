//! The row a computer lives in, and the mapping to the domain type.

use std::str::FromStr;

use boma_domain::{BotId, Computer, ComputerId, ComputerStatus, SpaceId};

use boma_app::AppError;

/// The row a computer lives in, as Postgres stores it.
#[derive(sqlx::FromRow)]
pub(crate) struct ComputerRow {
    pub(crate) id: String,
    pub(crate) space_id: String,
    pub(crate) bot_id: String,
    pub(crate) provider: String,
    pub(crate) external_id: Option<String>,
    pub(crate) status: String,
    pub(crate) screen_generation: i32,
    pub(crate) home_id: Option<String>,
    pub(crate) takeover_by: Option<String>,
    pub(crate) takeover_at: Option<String>,
    pub(crate) private: bool,
    pub(crate) memory_limit_mb: i32,
    pub(crate) cpu_limit: f64,
    pub(crate) idle_minutes: i32,
    pub(crate) started_at: Option<String>,
    pub(crate) last_used_at: Option<String>,
    pub(crate) stopped_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

/// The columns every computer row projects.
pub(crate) const COMPUTER_COLUMNS: &str = "id, space_id, bot_id, provider, external_id, status,
       screen_generation, home_id, takeover_by, takeover_at, private,
       memory_limit_mb, cpu_limit, idle_minutes,
       boma.rfc3339(started_at) AS started_at,
       boma.rfc3339(last_used_at) AS last_used_at,
       boma.rfc3339(stopped_at) AS stopped_at,
       boma.rfc3339(takeover_at) AS takeover_at,
       boma.rfc3339(created_at) AS created_at,
       boma.rfc3339(updated_at) AS updated_at";

/// Turns a stored row into the computer it describes.
pub(crate) fn computer(row: ComputerRow) -> Result<Computer, AppError> {
    Ok(Computer {
        id: ComputerId::new(row.id)?,
        space_id: SpaceId::new(row.space_id)?,
        bot_id: BotId::new(row.bot_id)?,
        provider: row.provider,
        external_id: row.external_id,
        status: ComputerStatus::from_str(&row.status)?,
        screen_generation: u64::try_from(row.screen_generation).unwrap_or(u64::MAX),
        home_id: row.home_id,
        takeover_by_user: row.takeover_by.map(boma_domain::UserId::new).transpose()?,
        takeover_at: row.takeover_at,
        private: row.private,
        memory_limit_mb: u64::try_from(row.memory_limit_mb).unwrap_or(u64::MAX),
        cpu_limit: row.cpu_limit,
        idle_minutes: u64::try_from(row.idle_minutes).unwrap_or(u64::MAX),
        started_at: row.started_at,
        last_used_at: row.last_used_at,
        stopped_at: row.stopped_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
