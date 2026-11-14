//! Pure business rules for Boma Bot.
//!
//! This crate is the part of the system that must be provably correct: it holds no IO, no clock,
//! no async runtime and no database knowledge, so every rule can be exercised with a unit or
//! property test in microseconds. Adapters elsewhere translate the outside world into these types.
//!
//! The run state machine lives here because it is the spine of the product: a rebuild that gets
//! `queued → leased → running → …` wrong cannot be trusted with an agent that spends money.

#![deny(missing_docs)]

pub mod account;
pub mod artifact;
pub mod audit;
pub mod bot;
pub mod card;
pub mod computer;
pub mod connector;
pub mod error;
pub mod event;
pub mod ids;
pub mod mcp;
pub mod memory;
pub mod message;
pub mod model;
pub mod page;
pub mod plugin;
pub mod routine;
pub mod run;
pub mod skill;
pub mod tenancy;
pub mod thread;
pub mod ticket;

pub use account::{Email, Session, Space, SpaceRole, User, validate_password};
pub use artifact::{
    ARTIFACT_MAX_BYTES, detect_mime, sniff_artifact_mime, validate_artifact_description,
    validate_artifact_name,
};
pub use audit::AuditKind;
pub use bot::Bot;
pub use card::{
    Card, CardId, CardKind, CardStatus, ExternalEffect, PendingToolCallStatus, ToolEffect,
    ToolKind, ToolSpec,
};
pub use computer::{
    Computer, ComputerId, ComputerStatus, MAX_CPU_LIMIT, MAX_IDLE_MINUTES, MAX_MEMORY_LIMIT_MB,
    MIN_CPU_LIMIT, MIN_IDLE_MINUTES, MIN_MEMORY_LIMIT_MB, validate_cpu_limit,
    validate_idle_minutes, validate_memory_limit,
};
pub use connector::{
    ConnectorAuth, ConnectorSpec, ConnectorToolSpec, builtin_connectors, validate_connector_id,
};
pub use error::DomainError;
pub use event::{Event, EventType};
pub use ids::{
    BotId, EventId, InviteId, MessageId, OrganizationId, RoutineId, RunId, SessionId, SpaceId,
    ThreadId, UserId,
};
pub use mcp::{
    McpTransport, program_allowed, sanitize_mcp_tool_name, split_command, validate_mcp_server_name,
    validate_mcp_server_url,
};
pub use memory::{MemoryDocument, MemoryScope, validate_memory_path};
pub use message::{
    Block, Message, MessagePage, MessageRole, Reaction, cursor_from_seq, parse_cursor,
    validate_message_text,
};
pub use model::{DeploymentModels, ModelChoice, ModelPreferences, resolve};
pub use page::{PageBlock, PageSource, validate_page_name, validate_page_source};
pub use plugin::{PluginSourceError, PluginSourceRef, parse_plugin_source};
pub use routine::{
    MAX_CRONS, MAX_ROUTINE_NAME_LENGTH, MAX_ROUTINE_PROMPT_LENGTH, ONCE, next_run_at,
    parse_trigger, validate_routine_name, validate_routine_prompt, validate_timezone,
    validate_triggers,
};
pub use run::{Run, RunStatus, RunTrigger};
pub use skill::{
    Skill, SkillOrigin, builtin_skills, validate_skill_description, validate_skill_instructions,
    validate_skill_name,
};
pub use tenancy::{SystemReason, TenantContext};
pub use thread::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, Thread, validate_page_size};
pub use ticket::{
    TICKET_COMMENT_MAX, TICKET_DESCRIPTION_MAX, TicketState, validate_ticket_text,
    validate_ticket_title,
};

/// Percent-encodes one part of a query or form (RFC 3986 unreserved survives).
///
/// One encoder for every caller in the deployment: the authorization URL the member's browser
/// opens, and the form the token endpoint receives.
#[must_use]
pub fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod percent_encode_tests {
    use super::percent_encode;

    #[test]
    fn req_033_percent_encoding_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(percent_encode("abcXYZ019-_.~"), "abcXYZ019-_.~");
        assert_eq!(percent_encode("a b&c=d+e"), "a%20b%26c%3Dd%2Be");
        assert_eq!(
            percent_encode("https://a.example/x?y=1"),
            "https%3A%2F%2Fa.example%2Fx%3Fy%3D1"
        );
    }
}
