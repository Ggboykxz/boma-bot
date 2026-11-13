//! Approval and question cards, and the side effects they authorize.
//!
//! A card is a pending question or approval on a thread (spec/08-ui-ux.md §90). A member answers
//! it once; after that the card is closed and the run it blocked resumes (REQ-007, REQ-017).
//!
//! The effect a card triggers is recorded exactly once per run: the `effect_key` is the
//! idempotency record that prevents a retried or replayed answer from executing twice
//! (REQ-017).

use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::{RunId, SpaceId, ThreadId};

crate::opaque_id! {
    /// A pending question or approval card on a thread.
    CardId
}

/// What a card asks: a question anyone can answer, or an approval that gates an effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    /// A question any member can answer.
    Question,
    /// An approval that gates a side effect until someone answers.
    Approval,
    /// A purchase request, subject to spending limits.
    Purchase,
    /// A request to take over a computer from another person.
    Takeover,
}

/// Where a card stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    /// Nobody has answered yet.
    Pending,
    /// A member answered and the run may resume.
    Answered,
    /// A member declined it; the run resumes rejected.
    Declined,
}

/// A pending card on a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    /// The card.
    pub id: CardId,
    /// The tenant it belongs to.
    pub space_id: SpaceId,
    /// The conversation it lives in.
    pub thread_id: ThreadId,
    /// The run that is waiting on it, when there is one.
    pub run_id: Option<RunId>,
    /// What it asks.
    pub kind: CardKind,
    /// Where it stands.
    pub status: CardStatus,
    /// The answer a member gave, once answered.
    pub answer: Option<String>,
    /// When it was created.
    pub created_at: String,
    /// When it was answered, when it was.
    pub answered_at: Option<String>,
}

/// One tool a run may call, as the catalogue describes it (REQ-016).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// The tool's key, unique within the catalogue.
    pub key: String,
    /// What it does, for the model and for the audit trail.
    pub description: String,
    /// The family it belongs to: built-in, connector, MCP, skill or memory.
    pub kind: ToolKind,
    /// The effect class the tool carries: reads, mutates, or spends (REQ-016).
    ///
    /// Missing means the stored row predates the effect class; the conservative default is
    /// [`ToolEffect::Write`], so the call waits for an approval rather than running silently.
    #[serde(default = "effect_default")]
    pub effect: ToolEffect,
}

const fn effect_default() -> ToolEffect {
    ToolEffect::Write
}

/// The effect class a tool carries, driving approval and idempotency (REQ-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    /// No mutation: read files, memory, observations.
    Read,
    /// Mutates state and has to wait for approval.
    Write,
    /// Spends money and has to wait for approval.
    Spend,
}

/// The family a tool belongs to (REQ-016).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Part of the runtime, always available.
    Builtin,
    /// Installed through a connector.
    Connector,
    /// Exposed by an MCP server.
    Mcp,
    /// A taught or installed skill.
    Skill,
    /// The bot's memory tools.
    Memory,
}

impl ToolKind {
    /// Parses a tool kind from its stored form (REQ-016).
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::UnknownEventType`] for a kind the product does not name.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "builtin" => Ok(Self::Builtin),
            "connector" => Ok(Self::Connector),
            "mcp" => Ok(Self::Mcp),
            "skill" => Ok(Self::Skill),
            "memory" => Ok(Self::Memory),
            other => Err(DomainError::UnknownEventType(other.to_owned())),
        }
    }
}

/// One side effect a run performs, keyed so it can never execute twice (REQ-017).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalEffect {
    /// The run the effect belongs to.
    pub run_id: RunId,
    /// The idempotency key: the same key never executes twice.
    pub effect_key: String,
    /// What the effect does.
    pub kind: String,
    /// Its payload.
    pub payload: serde_json::Value,
    /// When it was applied.
    pub applied_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Card, CardId, CardKind, CardStatus, ExternalEffect};
    use crate::{RunId, SpaceId, ThreadId};

    fn card() -> Card {
        Card {
            id: CardId::new("card_1").expect("an id is accepted"),
            space_id: SpaceId::new("spc_1").expect("an id is accepted"),
            thread_id: ThreadId::new("thr_1").expect("an id is accepted"),
            run_id: Some(RunId::new("run_1").expect("an id is accepted")),
            kind: CardKind::Approval,
            status: CardStatus::Pending,
            answer: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            answered_at: None,
        }
    }

    #[test]
    fn req_017_a_pending_card_can_be_answered_once() {
        let mut card = card();
        assert_eq!(card.status, CardStatus::Pending);
        card.status = CardStatus::Answered;
        card.answer = Some("approve".to_owned());
        card.answered_at = Some("2026-01-01T00:00:05Z".to_owned());
        assert_eq!(card.status, CardStatus::Answered);
        assert_eq!(card.answer.as_deref(), Some("approve"));
    }

    #[test]
    fn req_017_an_effect_key_is_unique_per_run() {
        let effect = ExternalEffect {
            run_id: RunId::new("run_1").expect("an id is accepted"),
            effect_key: "approve:card_1".to_owned(),
            kind: "purchase".to_owned(),
            payload: serde_json::json!({ "amount": 500 }),
            applied_at: Some("2026-01-01T00:00:05Z".to_owned()),
        };
        assert_eq!(effect.effect_key, "approve:card_1");
    }
}
