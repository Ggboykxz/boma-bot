//! The wire shapes of the RPC surface.
//!
//! Every field the client sees is declared here, so a rename is a compile error in the handler and
//! a visible diff in the document.
//!
//! Two rules, both pinned by the golden fixtures:
//!
//! * a field that is *always* present is serialized even when it is empty (`archivedAt: null`,
//!   `nextCursor: null`), so the client never has to guess whether a key is missing or empty;
//! * a field that is genuinely absent for a given case is omitted (`runId` on a message that no
//!   run produced), so the client never has to distinguish "no value" from "an empty value".

use boma_domain::{Block, Event, Message, Reaction};
use serde::{Deserialize, Serialize};

/// One emoji reaction, as the client sees it (REQ-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionView {
    /// The emoji.
    pub emoji: String,
    /// How many people reacted with it.
    pub count: u32,
    /// Whether the caller reacted with it.
    pub reacted: bool,
}

impl From<&Reaction> for ReactionView {
    fn from(reaction: &Reaction) -> Self {
        Self {
            emoji: reaction.emoji.clone(),
            count: reaction.count,
            reacted: reaction.reacted,
        }
    }
}

/// A message, as the client sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    /// The message's id.
    pub id: String,
    /// Its sequence in the thread.
    pub seq: i64,
    /// Who wrote it.
    pub role: String,
    /// The blocks it is made of.
    pub blocks: Vec<Block>,
    /// The run that produced it, when a run did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// The message this one answers, when it is a reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
    /// A short quote of the message being answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_quote: Option<String>,
    /// When it was written.
    pub created_at: String,
    /// The emoji reactions on this message (REQ-008).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<ReactionView>,
}

impl From<&Message> for MessageView {
    fn from(message: &Message) -> Self {
        Self {
            id: message.id.as_str().to_owned(),
            seq: message.seq,
            role: message.role.as_str().to_owned(),
            blocks: message.blocks.clone(),
            run_id: message.run_id.as_ref().map(|id| id.as_str().to_owned()),
            created_at: message.created_at.clone(),
            reply_to_message_id: message
                .reply_to_message_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            reply_quote: message.reply_quote.clone(),
            reactions: message.reactions.iter().map(ReactionView::from).collect(),
        }
    }
}

/// One frame of a thread's stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventView {
    /// The thread the event belongs to.
    pub thread_id: String,
    /// The thread's sequence for this event.
    pub seq: i64,
    /// What the event announces.
    #[serde(rename = "type")]
    pub kind: String,
    /// Its body.
    pub payload: serde_json::Value,
    /// When it happened.
    pub created_at: String,
}

impl From<&Event> for EventView {
    fn from(event: &Event) -> Self {
        Self {
            thread_id: event.thread_id.as_str().to_owned(),
            seq: event.seq,
            kind: event.kind.as_str().to_owned(),
            payload: event.payload.clone(),
            created_at: event.created_at.clone(),
        }
    }
}

/// A run, as the activity list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    /// The run's id.
    pub id: String,
    /// Where it is.
    pub status: String,
    /// What caused it.
    pub trigger: String,
    /// The beginning of the message that started it.
    pub prompt_snippet: Option<String>,
    /// When it started working.
    pub started_at: Option<String>,
    /// When it finished.
    pub completed_at: Option<String>,
}

impl RunView {
    /// Projects a run and the snippet of the message that started it.
    #[must_use]
    pub fn new(run: &boma_domain::Run, prompt_snippet: Option<String>) -> Self {
        Self {
            id: run.id.as_str().to_owned(),
            status: run.status.as_str().to_owned(),
            trigger: run.trigger.as_str().to_owned(),
            prompt_snippet,
            started_at: run.started_at.clone(),
            completed_at: run.completed_at.clone(),
        }
    }
}

/// The reply of `threads.send`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendReply {
    /// The message that was written.
    pub message_id: String,
    /// The run that was started for it.
    pub run_id: String,
    /// Where the run is.
    pub status: String,
}

/// The head of a thread, as the client renders the unread divider and its badge (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHeadReply {
    /// The thread's id.
    pub thread_id: String,
    /// The highest sequence the thread has allocated.
    pub last_seq: i64,
    /// When the newest message was written, when there is one.
    pub last_message_at: Option<String>,
    /// The caller's read position.
    pub read_seq: i64,
    /// How many visible messages sit above the caller's position.
    pub unread_count: u64,
}

impl From<&boma_app::usecases::ThreadHead> for ThreadHeadReply {
    fn from(head: &boma_app::usecases::ThreadHead) -> Self {
        Self {
            thread_id: head.thread_id.clone(),
            last_seq: head.last_seq,
            last_message_at: head.last_message_at.clone(),
            read_seq: head.read_seq,
            unread_count: head.unread_count,
        }
    }
}

/// One bot of the roster, as the rail renders it (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterBotView {
    /// The bot's id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The conversation of this bot.
    pub thread_id: String,
    /// When it was archived, if it was.
    pub archived_at: Option<String>,
    /// When the caller's snooze ends, when it is still in the future.
    pub snoozed_until: Option<String>,
    /// How many visible messages the caller has not read yet.
    pub unread_count: u64,
}

impl From<&boma_app::ports::BotSummary> for RosterBotView {
    fn from(summary: &boma_app::ports::BotSummary) -> Self {
        Self {
            id: summary.bot.id.as_str().to_owned(),
            name: summary.bot.name.clone(),
            thread_id: summary.thread_id.as_str().to_owned(),
            archived_at: summary.bot.archived_at.clone(),
            snoozed_until: summary.snoozed_until.clone(),
            unread_count: summary.unread_count,
        }
    }
}

/// The reply of `runs.retry` (REQ-012).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRetryReply {
    /// The run that was re-queued.
    pub run_id: String,
    /// Where it is after the retry: `queued`.
    pub status: String,
    /// How many attempts it has now consumed.
    pub attempt: u32,
}

impl From<&boma_domain::Run> for RunRetryReply {
    fn from(run: &boma_domain::Run) -> Self {
        Self {
            run_id: run.id.as_str().to_owned(),
            status: run.status.as_str().to_owned(),
            attempt: run.attempt,
        }
    }
}

/// The reply of `memory.write` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWriteReply {
    /// The path of the written document.
    pub path: String,
    /// The revision it is at now.
    pub revision: u64,
    /// When it was written.
    pub updated_at: String,
}

impl From<&boma_domain::MemoryDocument> for MemoryWriteReply {
    fn from(document: &boma_domain::MemoryDocument) -> Self {
        Self {
            path: document.path.clone(),
            revision: document.revision,
            updated_at: document.updated_at.clone(),
        }
    }
}

/// One memory document, as the client reads it (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDocumentReply {
    /// Its path.
    pub path: String,
    /// The revision it is at.
    pub revision: u64,
    /// Its content at that revision.
    pub content: String,
    /// When it was last written.
    pub updated_at: String,
}

impl From<&boma_domain::MemoryDocument> for MemoryDocumentReply {
    fn from(document: &boma_domain::MemoryDocument) -> Self {
        Self {
            path: document.path.clone(),
            revision: document.revision,
            content: document.content.clone(),
            updated_at: document.updated_at.clone(),
        }
    }
}

/// The reply of `memory.list` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDocumentsReply {
    /// The documents, in path order.
    pub documents: Vec<MemoryDocumentReply>,
}

/// The reply of `memory.remove` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRemoveReply {
    /// The path that was removed.
    pub path: String,
    /// Whether anything was there.
    pub removed: bool,
}

/// The reply of `memory.provider` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProviderReply {
    /// The provider backing memory documents.
    pub provider: String,
}

/// The reply of `routines.create` (REQ-023).
///
/// `@once` routines carry no timezone: they fire once, they do not reschedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineWriteReply {
    /// The new routine's id.
    pub id: String,
    /// When it wakes next, or none when it is inactive.
    pub next_run_at: Option<String>,
    /// Whether it wakes at all.
    pub active: bool,
    /// The IANA timezone its cron computes in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// The reply of `computer.openScreen` (REQ-030).
///
/// The token is returned once, here, and the server stores only its hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerOpenScreenReply {
    /// The opaque bearer token for `/api/internal/screen-target`.
    pub capability: String,
    /// The generation the capability names.
    pub screen_generation: u64,
}

/// The reply of `computer.status` (REQ-026, REQ-027).
///
/// `status` is one of `queued | running | interrupted | completed | failed | stopped`, or
/// `unavailable` when the deployment has no computer backend at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerStatusReply {
    /// Where the computer is. `stopped` when the bot has none; never an error.
    pub status: String,
    /// The provider configured for this deployment.
    pub provider: String,
    /// When the computer started, when it is running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// The generation of the screen backend; bumps on restart and invalidates old capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_generation: Option<u64>,
}

impl ComputerTakeoverReply {
    /// Builds the takeover-shaped reply of a computer the stores hold.
    #[must_use]
    pub fn for_computer(computer: &boma_domain::Computer) -> Self {
        Self {
            status: computer.status.as_str().to_owned(),
            provider: computer.provider.clone(),
            takeover_by_user: computer
                .takeover_by_user
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            takeover_at: computer.takeover_at.clone(),
        }
    }
}

impl ComputerStatusReply {
    /// Builds the reply of a computer the stores hold; a stopped computer carries no timestamps.
    #[must_use]
    pub fn for_computer(computer: &boma_domain::Computer) -> Self {
        let running = computer.status != boma_domain::ComputerStatus::Stopped;
        Self {
            status: computer.status.as_str().to_owned(),
            provider: computer.provider.clone(),
            started_at: running.then(|| computer.started_at.clone()).flatten(),
            screen_generation: running.then_some(computer.screen_generation),
        }
    }
}

/// The reply of `computer.takeover` and `computer.release` (REQ-029).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerTakeoverReply {
    /// Where the computer is.
    pub status: String,
    /// The provider configured for this deployment.
    pub provider: String,
    /// Who holds the screen, when someone does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub takeover_by_user: Option<String>,
    /// When the hold began, when one is in effect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub takeover_at: Option<String>,
}

/// One provider the deployment could run computers on (REQ-027).
///
/// Four booleans rather than a state machine: each names one honest capability of that provider
/// on this deployment — none of them implies another.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerProviderInfo {
    /// The provider name: `none`, `docker`, or a cloud provider the deployment recognises.
    pub name: String,
    /// Whether this deployment is configured for it.
    pub available: bool,
    /// Whether a frame command is configured (docker only).
    pub frame: bool,
    /// Whether shell tools run (anything other than `none`).
    pub shell: bool,
    /// Whether screenshots can be produced (a frame command is required).
    pub screenshot: bool,
}

/// The reply of `computer.provider` (REQ-027).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerProviderReply {
    /// The provider this deployment is configured with.
    pub active: String,
    /// Every provider the deployment recognises.
    pub providers: Vec<ComputerProviderInfo>,
}

/// The reply of `computer.settings` (REQ-026).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerSettingsReply {
    /// The RAM cap, in megabytes.
    pub memory_limit_mb: u64,
    /// The CPU cap, in CPUs.
    pub cpu_limit: f64,
    /// Minutes the computer may sit idle before it is released.
    pub idle_minutes: u64,
}

/// The reply of `routines.runNow` (REQ-025).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRunNowReply {
    /// The routine that was fired.
    pub routine_id: String,
    /// The run the trial started.
    pub run_id: String,
    /// Where that run is.
    pub status: String,
}

impl From<&boma_app::usecases::RoutineRunNowOutcome> for RoutineRunNowReply {
    fn from(outcome: &boma_app::usecases::RoutineRunNowOutcome) -> Self {
        Self {
            routine_id: outcome.routine.id.as_str().to_owned(),
            run_id: outcome.run.id.as_str().to_owned(),
            status: outcome.run.status.as_str().to_owned(),
        }
    }
}

impl From<&boma_app::ports::Routine> for RoutineWriteReply {
    fn from(routine: &boma_app::ports::Routine) -> Self {
        Self {
            id: routine.id.as_str().to_owned(),
            next_run_at: routine.next_run_at.clone(),
            active: routine.active,
            timezone: if routine.crons.iter().any(|entry| entry == boma_domain::ONCE) {
                None
            } else {
                Some(routine.timezone.clone())
            },
        }
    }
}

/// One routine, as the management view lists it (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineView {
    /// The routine's id.
    pub id: String,
    /// The bot it wakes.
    pub bot_id: String,
    /// Its conversation.
    pub thread_id: String,
    /// The name a person sees.
    pub name: String,
    /// The prompt the agent receives.
    pub prompt: String,
    /// The cron expressions.
    pub crons: Vec<String>,
    /// Its IANA timezone.
    pub timezone: String,
    /// Whether it wakes at all.
    pub active: bool,
    /// Whether the owner is told about each run.
    pub notify: bool,
    /// When it last fired.
    pub last_run_at: Option<String>,
    /// When it wakes next.
    pub next_run_at: Option<String>,
    /// The consecutive failures since the last success.
    pub account_failures: u32,
    /// When it was created.
    pub created_at: String,
    /// When it last changed.
    pub updated_at: String,
}

impl From<&boma_app::ports::Routine> for RoutineView {
    fn from(routine: &boma_app::ports::Routine) -> Self {
        Self {
            id: routine.id.as_str().to_owned(),
            bot_id: routine.bot_id.as_str().to_owned(),
            thread_id: routine.thread_id.as_str().to_owned(),
            name: routine.name.clone(),
            prompt: routine.prompt.clone(),
            crons: routine.crons.clone(),
            timezone: routine.timezone.clone(),
            active: routine.active,
            notify: routine.notify,
            last_run_at: routine.last_run_at.clone(),
            next_run_at: routine.next_run_at.clone(),
            account_failures: routine.account_failures,
            created_at: routine.created_at.clone(),
            updated_at: routine.updated_at.clone(),
        }
    }
}

/// The reply of `routines.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutinesReply {
    /// The routines of the bot, in creation order.
    pub routines: Vec<RoutineView>,
}

/// The reply of `routines.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRemoveReply {
    /// The routine that was removed.
    pub routine_id: String,
    /// Whether anything was there.
    pub removed: bool,
}

/// The reply of `bots.list` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotsListReply {
    /// The bots of the caller's space, oldest first.
    pub bots: Vec<RosterBotView>,
}

/// The reply of `threads.snooze` and `threads.unsnooze` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnoozeReply {
    /// The thread that was (or was not) snoozed.
    pub thread_id: String,
    /// The deadline stored, or `None` when the snooze was cleared.
    pub snoozed_until: Option<String>,
}

impl From<&boma_app::usecases::SnoozeOutcome> for SnoozeReply {
    fn from(outcome: &boma_app::usecases::SnoozeOutcome) -> Self {
        Self {
            thread_id: outcome.thread_id.clone(),
            snoozed_until: outcome.snoozed_until.clone(),
        }
    }
}

/// The reply of `threads.messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagesReply {
    /// The messages, newest first.
    pub messages: Vec<MessageView>,
    /// The cursor for the older page.
    pub next_cursor: Option<String>,
}

/// The reply of `bots.create` and `bots.get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotReply {
    /// The bot's id.
    pub id: String,
    /// The name a person sees.
    pub name: String,
    /// Its conversation.
    pub thread_id: String,
    /// When it was archived, if it was.
    pub archived_at: Option<String>,
    /// When it was created.
    pub created_at: String,
}

/// The reply of `spaces.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceReply {
    /// The space's id.
    pub id: String,
    /// The name a person sees.
    pub name: String,
    /// Whether this is the space a member lands in.
    pub is_default: bool,
    /// The caller's role in it.
    pub role: String,
}

/// The reply of `spaces.invite`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationReply {
    /// The invitation's id.
    pub invitation_id: String,
    /// When it expires.
    pub expires_at: String,
    /// Whether it is still pending.
    pub status: String,
}

/// The reply of `runs.cancel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelReply {
    /// The run the caller stopped.
    pub id: String,
    /// Where it is now: `cancelled`.
    pub status: String,
}

/// The reply of `threads.react` (REQ-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactReply {
    /// The message reacted to.
    pub message_id: String,
    /// The truth for the message after the change.
    pub reactions: Vec<ReactionView>,
}

/// The reply of `threads.answer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerReply {
    /// Whether the answer was accepted.
    pub ok: bool,
    /// The run that resumes, when the card was waiting on one.
    pub run_id: Option<String>,
}

/// The reply of `runs.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsReply {
    /// The rows, newest first.
    pub runs: Vec<RunView>,
    /// The cursor for the older page.
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use boma_domain::{Block, MessageId, MessageRole, RunId, ThreadId};

    use super::{MessageView, RunView, SendReply};

    #[test]
    fn req_005_a_message_without_a_run_omits_the_field_entirely() {
        let message = boma_domain::Message {
            id: MessageId::new("msg_1").expect("an id is accepted"),
            thread_id: ThreadId::new("thr_1").expect("an id is accepted"),
            seq: 2,
            role: MessageRole::User,
            blocks: vec![Block::Text {
                text: "hello".to_owned(),
            }],
            bot_id: None,
            author_user_id: None,
            run_id: None,
            reply_to_message_id: None,
            reply_quote: None,
            client_nonce: None,
            created_at: "2026-01-01T00:00:02Z".to_owned(),
            reactions: Vec::new(),
        };
        let json = serde_json::to_value(MessageView::from(&message)).expect("the view serialises");
        let object = json.as_object().expect("the view is an object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["blocks", "createdAt", "id", "role", "seq"]);
        assert_eq!(json["blocks"][0]["type"], "text");
        assert_eq!(json["blocks"][0]["text"], "hello");
    }

    #[test]
    fn req_001_the_send_reply_names_the_message_and_the_run() {
        let reply = SendReply {
            message_id: "msg_1".to_owned(),
            run_id: "run_1".to_owned(),
            status: "queued".to_owned(),
        };
        let json = serde_json::to_value(reply).expect("the reply serialises");
        assert_eq!(
            json,
            serde_json::json!({"messageId": "msg_1", "runId": "run_1", "status": "queued"})
        );
        let _ = RunId::new("run_1").expect("an id is accepted");
    }

    #[test]
    fn req_011_a_run_always_carries_its_empty_fields() {
        let run = boma_domain::Run {
            id: RunId::new("run_1").expect("an id is accepted"),
            space_id: boma_domain::SpaceId::new("spc_1").expect("an id is accepted"),
            bot_id: boma_domain::BotId::new("bot_1").expect("an id is accepted"),
            thread_id: ThreadId::new("thr_1").expect("an id is accepted"),
            user_id: boma_domain::UserId::new("usr_1").expect("an id is accepted"),
            status: boma_domain::RunStatus::Queued,
            trigger: boma_domain::RunTrigger::User,
            client_nonce: None,
            source_message_id: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            started_at: None,
            completed_at: None,
            error: None,
            lease_owner: None,
            lease_fence: 0,
            lease_expires_at: None,
            steering_from_seq: None,
            steering_consumed_seq: 0,
            attempt: 1,
            model_provider: None,
            model_id: None,
            model_pinned: false,
            parent_run_id: None,
            routine_id: None,
            not_before: None,
        };
        let json = serde_json::to_value(RunView::new(&run, None)).expect("the view serialises");
        assert_eq!(json["promptSnippet"], serde_json::Value::Null);
        assert_eq!(json["startedAt"], serde_json::Value::Null);
        assert_eq!(json["completedAt"], serde_json::Value::Null);
    }
}
