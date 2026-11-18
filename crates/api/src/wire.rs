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
    /// The run that was started for it. This member's run first when several start.
    pub run_id: String,
    /// Where the run is.
    pub status: String,
    /// Every run started by the message, in creation order. Omitted for the regular one-bot case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_ids: Option<Vec<String>>,
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
    /// Who holds the screen, when someone does (REQ-029). Additive to the contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeover_by_user: Option<String>,
    /// When the hold began, when one is in effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeover_at: Option<String>,
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
            takeover_by_user: computer
                .takeover_by_user
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            takeover_at: computer.takeover_at.clone(),
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
    /// `private` (isolated per bot) or `team` (shared within the space).
    pub mode: String,
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
    /// Whether repository events from the bot's webhook wake this routine.
    pub github_enabled: bool,
    /// The exact addresses admitted to wake this routine by mail.
    #[serde(default)]
    pub email_senders: Vec<String>,
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
            github_enabled: routine.github_enabled,
            email_senders: routine.email_senders.clone(),
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

/// The reply of `botSecrets.set` (REQ-046). Write-only: never carries the value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretSetReply {
    /// The name the secret is stored under.
    pub name: String,
}

/// One stored secret's public face (REQ-046): a name, never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretView {
    /// The identifier stored under the bot.
    pub name: String,
    /// When the secret was written.
    pub created_at: String,
}

/// The reply of `botSecrets.list` (REQ-046): names only, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretsReply {
    /// The stored secrets.
    pub secrets: Vec<SecretView>,
}

/// One MCP server, as the member sees it (REQ-033). Credentials never appear here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerView {
    /// The server's id.
    pub id: String,
    /// The member's display name.
    pub name: String,
    /// `http` or `stdio`.
    pub transport: String,
    /// The HTTPS URL, when it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The command, when it is a stdio server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// `none`, `oauth_pending`, `connected` or `error`.
    pub auth_state: String,
    /// The member's explicit allowance for a private endpoint (defect D-05).
    pub allow_private: bool,
    /// Whether an OAuth client is configured.
    pub oauth_configured: bool,
    /// The number of tools the last probe discovered, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_count: Option<i32>,
    /// When the server was registered.
    pub created_at: String,
}

/// The reply of `mcp.servers.list` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServersListReply {
    /// The servers of the space, name ascending.
    pub servers: Vec<McpServerView>,
}

/// The reply of `mcp.servers.add` and `mcp.servers.probe` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerReply {
    /// The server.
    pub server: McpServerView,
}

/// The reply of `mcp.servers.remove` (REQ-033). Removing a missing server is a success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServersRemoveReply {
    /// Whether anything was there.
    pub removed: bool,
}

/// The reply of `mcp.oauth.start` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthStartReply {
    /// Where the member's browser opens.
    pub authorization_url: String,
    /// The state token to present to `mcp.oauth.complete`.
    pub state: String,
}

/// One catalogue connector with this space's install state (REQ-032).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorView {
    /// The catalogue id.
    pub connector_id: String,
    /// The member's display name.
    pub name: String,
    /// One line about what installing it gives.
    pub description: String,
    /// `none` or `api_key`.
    pub auth_kind: String,
    /// Whether this space installed it.
    pub installed: bool,
}

/// The reply of `connections.list` (REQ-032).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsListReply {
    /// The catalogue, id ascending.
    pub connections: Vec<ConnectorView>,
}

/// The reply of `connections.install` (REQ-032).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsInstallReply {
    /// The installed connector.
    pub connector: ConnectorView,
}

/// The reply of `connections.remove` (REQ-032). Revoking a missing install is a success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsRemoveReply {
    /// Whether anything was there.
    pub removed: bool,
}

/// One plugin skipped part, named with its reason (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInstallView {
    /// The plugin's name.
    pub name: String,
    /// The canonical source (`url[@ref][#subdir]`).
    pub source: String,
    /// The commit SHA, when one was resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_sha: Option<String>,
    /// The number of skills installed.
    pub skill_count: u32,
    /// The number of MCP servers registered.
    pub mcp_server_count: u32,
    /// What was left out, named with reasons.
    pub skipped: Vec<serde_json::Value>,
    /// When it was installed or last replaced.
    pub created_at: String,
}

/// The reply of `marketplace.list` (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceListReply {
    /// The connector catalogue with this space's install state.
    pub connectors: Vec<ConnectorView>,
    /// The plugins this space installed.
    pub installed_plugins: Vec<PluginInstallView>,
}

/// The reply of `marketplace.install` (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceInstallReply {
    /// The installation's outcome.
    pub install: PluginInstallView,
}

/// The reply of `marketplace.submit` (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceSubmitReply {
    /// Always true: a submission is recorded, nothing about who submitted it is disclosed.
    pub ok: bool,
    /// The submission's id.
    pub id: String,
}

/// The mail policy of one bot as the API exposes it (REQ-047).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailPolicyView {
    /// The bot's id.
    pub bot_id: String,
    /// `hold` (the default) or `drop`.
    pub policy: String,
    /// The exact addresses admitted to start work.
    pub allowed_senders: Vec<String>,
    /// When the rule last changed.
    pub updated_at: String,
}

/// The reply of `messaging.installUrl` (REQ-048).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallUrlReply {
    /// The URL the member opens.
    pub url: String,
}

/// The reply of `messaging.installTelegram`. The secret is shown exactly once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallTelegramReply {
    /// The installation id.
    pub installation_id: String,
    /// The bot username Telegram reported.
    pub bot_username: String,
    /// The webhook URL to register.
    pub webhook_url: String,
    /// The secret Telegram presents on every delivery. Shown here only.
    pub webhook_secret: String,
    /// Whether `setWebhook` succeeded. When false, register the URL manually.
    pub registered: bool,
}

/// The reply of `messaging.installGeneric`. The secret is shown exactly once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallGenericReply {
    /// The installation id.
    pub installation_id: String,
    /// The webhook URL the member configures in their platform.
    pub webhook_url: String,
    /// The `X-Boma-Signature` secret. Shown here only.
    pub secret: String,
}

/// One messaging installation as the member sees it (REQ-048).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagingInstallationView {
    /// The installation's id.
    pub id: String,
    /// The platform it talks to.
    pub platform: String,
    /// The bot its replies come from.
    pub bot_id: String,
    /// The platform's workspace/team id.
    pub workspace_id: String,
    /// The human-readable workspace name, when the platform offers one.
    pub workspace_name: String,
    /// The generic platform's reply target (`None` for Slack and Telegram).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_url: Option<String>,
    /// Whether the reply target may be a private host.
    pub allow_private: bool,
    /// `active` or `removed`.
    pub status: String,
    /// When it was created.
    pub created_at: String,
    /// When it last changed.
    pub updated_at: String,
}

impl From<&boma_app::ports::MessagingInstallation> for MessagingInstallationView {
    fn from(installation: &boma_app::ports::MessagingInstallation) -> Self {
        Self {
            id: installation.id.clone(),
            platform: installation.platform.as_str().to_owned(),
            bot_id: installation.bot_id.clone(),
            workspace_id: installation.workspace_id.clone(),
            workspace_name: installation.workspace_name.clone(),
            reply_url: installation.reply_url.clone(),
            allow_private: installation.allow_private,
            status: installation.status.clone(),
            created_at: installation.created_at.clone(),
            updated_at: installation.updated_at.clone(),
        }
    }
}

/// The reply of `messaging.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationsReply {
    /// The installations of the caller's space, newest first.
    pub installations: Vec<MessagingInstallationView>,
}

/// The reply of `messaging.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagingRemoveReply {
    /// Whether something was there to remove.
    pub removed: bool,
}

/// One external conversation as the member sees it (REQ-048).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationView {
    /// The external thread's id.
    pub thread_id: String,
    /// The conversation the platform used.
    pub conversation_id: String,
    /// When the conversation opened.
    pub created_at: String,
}

/// The reply of `messaging.conversations`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationsReply {
    /// The conversations of the installation, oldest first.
    pub conversations: Vec<ConversationView>,
}

/// One held message as the API exposes it (REQ-047).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldMailView {
    /// The held message's id.
    pub id: String,
    /// Who wrote.
    pub sender: String,
    /// The subject.
    pub subject: String,
    /// The plain-text body.
    pub body: String,
    /// Why it is held.
    pub reason: String,
    /// When it arrived.
    pub created_at: String,
}

/// The reply of `mail.policyGet` and `mail.policySet`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailPolicyReply {
    /// The policy.
    pub policy: MailPolicyView,
}

/// The reply of `mail.held.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldListReply {
    /// The held messages, newest first.
    pub held: Vec<HeldMailView>,
}

/// The reply of `mail.held.release`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldReleaseReply {
    /// The run the message started.
    pub run_id: String,
}

/// The reply of `mail.held.drop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldDropReply {
    /// Whether something was there.
    pub dropped: bool,
}

/// One ticket of the board as the API exposes it (REQ-038).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketView {
    /// The ticket's id.
    pub id: String,
    /// The human-facing key: `BOT-<n>`.
    pub key: String,
    /// What to do.
    pub title: String,
    /// Longer context.
    pub description: String,
    /// The board column.
    pub state: String,
    /// The assigned bot, when one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_bot_id: Option<String>,
    /// The number of comments.
    pub comment_count: i32,
    /// When it was created.
    pub created_at: String,
    /// When it last changed.
    pub updated_at: String,
}

/// The reply of `boards.get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardsGetReply {
    /// Every ticket of the space, in key order.
    pub tickets: Vec<TicketView>,
}

/// The reply of `tickets.create` and `tickets.move`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketReply {
    /// The ticket.
    pub ticket: TicketView,
}

/// One comment as the API exposes it (REQ-038).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketCommentView {
    /// The comment's id.
    pub id: String,
    /// The ticket it belongs to.
    pub ticket_id: String,
    /// `member` or `bot`.
    pub author_kind: String,
    /// Who wrote it.
    pub author_id: String,
    /// The body.
    pub body: String,
    /// When it was written.
    pub created_at: String,
}

/// The reply of `tickets.comment`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketCommentReply {
    /// The comment.
    pub comment: TicketCommentView,
}

/// The reply of `pages.save` (REQ-037).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagesSaveReply {
    /// The page's id.
    pub page_id: String,
    /// Its version in the family.
    pub version: i32,
    /// The library label.
    pub name: String,
}

/// The reply of `pages.get` (REQ-037): the page plus its source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagesGetReply {
    /// The page's id.
    pub page_id: String,
    /// The library label.
    pub name: String,
    /// `private` or `link`.
    pub visibility: String,
    /// The id of the current version.
    pub current_version_id: Option<String>,
    /// The version read (latest by default).
    pub version: i32,
    /// The validated document.
    pub source: serde_json::Value,
    /// When it was compiled.
    pub updated_at: String,
}

/// The reply of `pages.share` (REQ-037). The token is handed out once, never stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagesShareReply {
    /// The capability: open it to read the page, signed out.
    pub token: String,
    /// The public path to share.
    pub path: String,
}

/// The reply of `pages.revokeShare`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagesRevokeShareReply {
    /// Whether anything changed.
    pub revoked: bool,
}

/// One artifact as the API exposes it (REQ-036).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactView {
    /// The artifact's id.
    pub id: String,
    /// The bot that produced it, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
    /// The group thread it belongs to, when one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// The run that produced it, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// The member's file name.
    pub name: String,
    /// One line about what the file is.
    #[serde(default)]
    pub description: String,
    /// What the client declared.
    pub declared_mime: String,
    /// What the bytes proved (defect D-03).
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
    /// `sha256:<hex>` of the content.
    pub hash: String,
    /// The root of the version family.
    pub root_artifact_id: String,
    /// 1 for the first version, +1 per update.
    pub version: i32,
    /// When it was stored.
    pub created_at: String,
}

/// The reply of `artifacts.create` — the golden: metadata only, never the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactCreateReply {
    /// The artifact's id.
    pub id: String,
    /// Its version in the family.
    pub version: i32,
    /// The family root.
    pub root_artifact_id: String,
    /// Size in bytes.
    pub size: u64,
    /// `sha256:<hex>` of the content.
    pub hash: String,
}

/// The reply of `artifacts.get` / `artifacts.getById` — the artifact plus its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactGetReply {
    /// The artifact's metadata.
    pub artifact: ArtifactView,
    /// The content, base64-encoded.
    pub content_base64: String,
}

/// One artifact in a versions list (REQ-036): id, version and family, newest first (the golden).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVersionView {
    /// The artifact's id.
    pub id: String,
    /// Its version in the family.
    pub version: i32,
    /// The family root.
    pub root_artifact_id: String,
}

/// The reply of `artifacts.listVersions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVersionsReply {
    /// Every version of the family, newest first.
    pub versions: Vec<ArtifactVersionView>,
}

/// The reply of `artifacts.listSpace`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactListSpaceReply {
    /// The latest version of every family in the space.
    pub artifacts: Vec<ArtifactView>,
}

/// The reply of `artifacts.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRemoveReply {
    /// How many version rows went with the family.
    pub removed: u64,
}

/// One skill in a list (REQ-035). Built-ins carry no timestamps; taught and plugin skills do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillView {
    /// The wire name the model loads.
    pub name: String,
    /// One line a person reads.
    pub description: String,
    /// `builtin`, `taught` or `plugin`.
    pub origin: String,
    /// The plugin source it was installed from, when it was installed from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_source: Option<String>,
    /// When the skill was written; empty for built-ins.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created_at: String,
}

/// The reply of `skills.list` (REQ-035): built-ins first, then the space's own, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsReply {
    /// The skills visible to the caller.
    pub skills: Vec<SkillView>,
}

/// The reply of `skills.get` (REQ-035): one skill with its instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillReply {
    /// The wire name.
    pub name: String,
    /// One line a person reads.
    pub description: String,
    /// The instructions handed to the model.
    pub instructions: String,
    /// `builtin`, `taught` or `plugin`.
    pub origin: String,
    /// The plugin source it was installed from, when it was installed from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_source: Option<String>,
}

/// The reply of `skills.teach` (REQ-035).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachSkillReply {
    /// The taught skill's name.
    pub name: String,
}

/// The reply of `botSecrets.forget` (REQ-046). Deleting a missing name is a success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretForgetReply {
    /// The name that was forgotten.
    pub name: String,
    /// Whether anything was there.
    pub removed: bool,
}

/// One model row of `models.catalog` (REQ-052, golden `models-catalog`).
///
/// Exactly the golden's four fields: the catalogue says what the deployment reaches, and nothing
/// about keys or account identifiers (the note in `spec/golden/models-catalog.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModelView {
    /// The model identifier.
    pub id: String,
    /// A name a person can read.
    pub name: String,
    /// The context window in tokens.
    pub context_window: u64,
    /// The roles the model supports.
    pub roles: Vec<String>,
}

/// One provider of `models.catalog` (REQ-052).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProviderView {
    /// The provider id.
    pub id: String,
    /// A name a person can read.
    pub name: String,
    /// The models of this deployment on that provider.
    pub models: Vec<CatalogModelView>,
}

/// One model offered by a local loopback server (REQ-075).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocalDiscoveredModelView {
    /// Which server offered it: `ollama` or `lm-studio`.
    pub source: String,
    /// The model identifier the server listed.
    pub id: String,
}

/// The reply of `models.discovered` — what local loopback servers offer (REQ-075).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDiscoveredReply {
    /// The discovered models, Ollama first. Empty when no local server answers.
    pub models: Vec<LocalDiscoveredModelView>,
}

/// The reply of `models.catalog` (REQ-052, golden `models-catalog`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsCatalogReply {
    /// The providers, in the deployment's declaration order.
    pub providers: Vec<CatalogProviderView>,
}

/// One connected credential, as `models.credentials` lists it (REQ-052).
///
/// The key itself is never listed: the reply says which providers are connected and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCredentialView {
    /// The provider.
    pub provider: String,
    /// `connected`.
    pub status: String,
    /// When it was connected.
    pub created_at: String,
}

/// The reply of `models.credentials` (REQ-052).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCredentialsReply {
    /// The connected providers.
    pub credentials: Vec<ModelCredentialView>,
}

/// The reply of `models.connect` and `models.disconnect` (REQ-052).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConnectReply {
    /// The provider that is now connected, or was forgotten.
    pub provider: String,
    /// Whether the operation left a credential connected (`false` for a forgotten absent one).
    pub connected: bool,
}

/// The reply of `models.preferences.get` and `models.preferences.set` (REQ-052).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreferenceReply {
    /// The person's model key, or `null` for the deployment default.
    pub model_key: Option<String>,
}

/// The reply of `billing.status` (REQ-051).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingStatusReply {
    /// `free` or `pro`.
    pub plan: String,
    /// `active`, `canceled` or `past_due`.
    pub status: String,
    /// The credits the checkout seeded, in minor units.
    pub credits_minor_units: i64,
    /// When the current period ends, when known.
    pub current_period_end: Option<String>,
    /// Whether this deployment can open a checkout.
    pub checkout_allowed: bool,
}

/// The reply of `billing.checkout` (REQ-051).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingCheckoutReply {
    /// The URL the member's browser opens.
    pub url: String,
    /// The provider's reference for the session.
    pub provider_ref: String,
}

/// The reply of `billing.limit.get` and `billing.limit.set` (REQ-018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendingLimitReply {
    /// Whether the cap is enforced.
    pub enabled: bool,
    /// The ISO currency of the cap.
    pub currency: String,
    /// The cap in minor units.
    pub cap_minor_units: i64,
    /// What the space spent this calendar month, in minor units.
    pub spent_minor_units: i64,
    /// What remains, or `null` when the cap is disabled.
    pub remaining_minor_units: Option<i64>,
}

/// One audit event, as the admin surfaces see it (REQ-045).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventView {
    /// The event's id.
    pub id: String,
    /// The space the event belongs to; `null` for deployment-scoped events (absent from the file).
    pub space_id: Option<String>,
    /// The member who caused it, when there was one.
    pub actor_user_id: Option<String>,
    /// What happened.
    pub kind: String,
    /// What it happened to.
    pub target: String,
    /// The structured payload, with secret-shaped values redacted.
    pub metadata: serde_json::Value,
    /// When it happened.
    pub created_at: String,
}

/// The reply of `audit.export` (REQ-045): the header plus the JSONL body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditExportReply {
    /// The format of the export (`1` for the stable JSONL shape used here).
    pub version: String,
    /// How many events the file carries.
    pub events: u64,
    /// One line of metadata, then one event per line, oldest first.
    pub jsonl: String,
}

/// The reply of `security.events.list` (REQ-045).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityEventsReply {
    /// The security events of the caller's space, newest first, capped at two hundred.
    pub events: Vec<AuditEventView>,
}

/// Per-section counts for one account archive, as the client sees them (REQ-044).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCountsView {
    /// The number of bots.
    pub bots: u64,
    /// The number of threads.
    pub threads: u64,
    /// The number of messages.
    pub messages: u64,
    /// The number of bot-scoped memory documents.
    pub memory: u64,
    /// The number of routines.
    pub routines: u64,
    /// The number of artifacts included.
    pub artifacts: u64,
}

/// The reply of `export.start` (REQ-044).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStartReply {
    /// The export id.
    pub export_id: String,
    /// `pending` right now; the zip renders when the client asks for it (or polls status).
    pub status: String,
}

/// The reply of `export.status` (REQ-044).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStatusReply {
    /// The export id.
    pub export_id: String,
    /// `pending` for a fresh request, `ready` for one a previous call rendered.
    pub status: String,
    /// The counts of each section.
    pub counts: ExportCountsView,
    /// Where the browser can pick the archive up.
    pub download_url: String,
}

/// The body of `export.status` and `export.download`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportIdRequest {
    /// The export id returned by `export.start`.
    pub export_id: String,
}

/// The reply of `export.download` (REQ-044).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDownloadReply {
    /// The route that carries the zip archive.
    pub download_url: String,
}

/// The body of `account.delete` (REQ-044). The word is deliberate; the server refuses anything else.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountDeleteRequest {
    /// The literal confirmation: `delete` (case-insensitive).
    pub confirmation: String,
}

/// The reply of `account.delete` (REQ-044).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDeleteReply {
    /// Always `deleted` on success.
    pub status: String,
}

/// One member of a space as the admin surface sees them (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminMemberView {
    /// The member's id.
    pub user_id: String,
    /// Their address.
    pub email: String,
    /// The display name.
    pub name: String,
    /// Their role.
    pub role: String,
    /// When they were last seen, when known; `null` otherwise.
    pub last_seen_at: Option<String>,
}

/// The reply of `admin.members.list` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminMembersReply {
    /// The members of the caller's space.
    pub members: Vec<AdminMemberView>,
}

/// One pending invitation as the admin surface sees them (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminInvitationView {
    /// The invitation's id.
    pub id: String,
    /// The invited email; shown to admins only.
    pub email: String,
    /// The role the invitation grants.
    pub role: String,
    /// When it expires.
    pub expires_at: String,
}

/// The reply of `admin.invitations.list` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminInvitationsReply {
    /// The pending invitations of the caller's space.
    pub invitations: Vec<AdminInvitationView>,
}

/// The body of `admin.invitations.revoke` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminInvitationRevokeRequest {
    /// The invitation id; the server refuses a blank or unknown one.
    pub invitation_id: String,
}

/// The reply of `admin.invitations.revoke` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminInvitationRevokeReply {
    /// Whether the pending invitation was revoked by this call.
    pub revoked: bool,
}

/// One server-level grant of hosted access, as the admin surface sees them (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminGrantView {
    /// The granted email.
    pub email: String,
    /// The admin's reason for the grant.
    pub reason: String,
    /// Who granted it: the admin id.
    pub granted_by: String,
    /// When it was written.
    pub created_at: String,
}

/// The reply of `admin.grants.list` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminGrantsReply {
    /// Every grant on the deployment, alphabetical email.
    pub grants: Vec<AdminGrantView>,
}

/// The body of `admin.grants.create` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminGrantCreateRequest {
    /// The granted email.
    pub email: String,
    /// The admin's reason for the grant; one line, at most 200 characters.
    pub reason: String,
}

/// The reply of `admin.grants.create` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminGrantCreateReply {
    /// The grant as stored.
    pub grant: AdminGrantView,
}

/// The body of `admin.grants.revoke` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminGrantRevokeRequest {
    /// The email to revoke.
    pub email: String,
}

/// The reply of `admin.grants.revoke` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminGrantRevokeReply {
    /// Whether a grant was actually revoked by this call.
    pub revoked: bool,
}

/// One plugin submission as the admin surface sees them (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginSubmissionView {
    /// The submission's id.
    pub id: String,
    /// The canonical source.
    pub source: String,
    /// The plugin's display name, when the author gave one.
    pub name: Option<String>,
    /// The review state.
    pub status: String,
    /// When it was filed.
    pub created_at: String,
}

/// The body of `admin.plugins.list` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminPluginsListRequest {
    /// The status to filter by; absent lists everything.
    pub status: Option<String>,
}

/// The reply of `admin.plugins.list` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginsReply {
    /// The submissions of the review queue, newest first.
    pub submissions: Vec<AdminPluginSubmissionView>,
}

/// The body of `admin.plugins.decide` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminPluginDecideRequest {
    /// The submission's id.
    pub submission_id: String,
    /// The decision: `approved` or `rejected`.
    pub decision: String,
}

/// The reply of `admin.plugins.decide` (REQ-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginDecideReply {
    /// The stored decision.
    pub status: String,
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
    fn req_010_a_bot_message_serialises_its_attribution_and_a_user_message_does_not() {
        use boma_domain::BotId;

        let base = boma_domain::Message {
            id: MessageId::new("msg_1").expect("an id is accepted"),
            thread_id: ThreadId::new("thr_1").expect("an id is accepted"),
            seq: 1,
            role: MessageRole::Bot,
            blocks: vec![Block::Text {
                text: "Three PRs, one revert.".to_owned(),
            }],
            bot_id: Some(BotId::new("bot_a").expect("an id is accepted")),
            author_user_id: None,
            run_id: Some(RunId::new("run_1").expect("an id is accepted")),
            reply_to_message_id: None,
            reply_quote: None,
            client_nonce: None,
            created_at: "2026-01-01T00:00:01Z".to_owned(),
            reactions: Vec::new(),
        };
        let bot = super::GroupMessageView::from(&base);
        let json = serde_json::to_value(bot).expect("the view serialises");
        assert_eq!(json["botId"], serde_json::json!("bot_a"));
        assert_eq!(json["runId"], serde_json::json!("run_1"));
        assert_eq!(json["role"], serde_json::json!("bot"));

        let user = boma_domain::Message {
            role: MessageRole::User,
            bot_id: None,
            run_id: None,
            ..base
        };
        let json = serde_json::to_value(super::GroupMessageView::from(&user))
            .expect("the view serialises");
        assert!(json.get("botId").is_none());
        assert_eq!(json["role"], serde_json::json!("user"));
    }

    #[test]
    fn req_001_the_send_reply_names_the_message_and_the_run() {
        let reply = SendReply {
            message_id: "msg_1".to_owned(),
            run_id: "run_1".to_owned(),
            status: "queued".to_owned(),
            run_ids: None,
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

/// The body of `invitations.accept` (REQ-042): the opaque token exactly as it appeared in the link.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvitationAcceptRequest {
    /// The invitation token, held only by the invitee.
    pub token: String,
}

/// The reply of `invitations.accept` (REQ-042).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationAcceptReply {
    /// The space the caller now belongs to.
    pub space_id: String,
}

/// The public reply of `GET /api/v1/invites/{token}` (REQ-042).
///
/// The inviter's display name, the space's name, the role and the expiry — nothing else. An
/// anonymous visitor learns only what the link itself would tell them by existing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationPublicPreview {
    /// The name of the space the invitee is joining.
    pub space_name: String,
    /// The display name of the inviter, when it was recorded.
    pub inviter_name: Option<String>,
    /// The role the invitee is about to take.
    pub role: String,
    /// When the invitation stops being usable.
    pub expires_at: String,
}

/// The body of `groups.create` (REQ-010).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupsCreateRequest {
    /// The name people see.
    pub name: String,
    /// The bots in the room, at most eight.
    #[serde(default)]
    pub bot_ids: Vec<String>,
}

/// The reply of `groups.create` (REQ-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupsCreateReply {
    /// The new group's id.
    pub id: String,
    /// The name people see.
    pub name: String,
    /// The conversation everyone shares.
    pub thread_id: String,
    /// The member bots, in creation order.
    pub member_bot_ids: Vec<String>,
}

/// The body of `groups.get` — one group as its members see it (REQ-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupBotView {
    /// The bot's id.
    pub id: String,
    /// The bot's display name.
    pub name: String,
}

/// The reply of `groups.get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupGetReply {
    /// The group's id.
    pub id: String,
    /// The name people see.
    pub name: String,
    /// The thread every participant shares.
    pub thread_id: String,
    /// The member bots, in join order.
    pub member_bot_ids: Vec<String>,
    /// The member bots with the names they answer to.
    pub member_bots: Vec<GroupBotView>,
    /// When the group was created.
    pub created_at: String,
    /// The run working in this room right now, when there is one (REQ-068).
    pub active_run: Option<GroupActiveRunView>,
}

/// The run currently working in a group room, so the room can offer reassign (REQ-068).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupActiveRunView {
    /// The run's id.
    pub id: String,
    /// The bot currently working.
    pub bot_id: String,
    /// The run status.
    pub status: String,
}

/// The answer to `groups.reassign` (REQ-068).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReassignReply {
    /// The new run.
    pub run_id: String,
    /// The hand-off message in the room.
    pub message_id: String,
    /// The new run's status.
    pub status: String,
}

/// One message as the group room renders it, including the bot that authored it.
///
/// The same body shape as [`MessageView`] with attribution on top: a bot message carries
/// `botId`; a user message carries none (the member is author by the auth context, not by a
/// payload field).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMessageView {
    /// Everything a regular message view carries.
    #[serde(flatten)]
    pub base: MessageView,
    /// The bot that wrote this message, when a bot did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
}

impl From<&Message> for GroupMessageView {
    fn from(message: &Message) -> Self {
        Self {
            base: MessageView::from(message),
            bot_id: message.bot_id.as_ref().map(|bot| bot.as_str().to_owned()),
        }
    }
}

/// The reply of `groups.messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMessagesReply {
    /// The messages, newest first.
    pub messages: Vec<GroupMessageView>,
    /// The cursor of the older page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// The reply of `groups.list` — the caller's groups with the shared thread handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupsListReply {
    /// The groups the caller belongs to, oldest first.
    pub groups: Vec<GroupListView>,
}

/// One group in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupListView {
    /// The group's id.
    pub id: String,
    /// The name people see.
    pub name: String,
    /// The thread everyone shares.
    pub thread_id: String,
    /// The id of the bot every message is attributed to until someone mentions another.
    pub owner_bot_id: String,
    /// The number of bots in this group.
    pub member_count: u32,
    /// When it was created.
    pub created_at: String,
}

/// The reply of `bots.connectExternal` — one external agent now answers for the bot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentConnectedReply {
    /// The bot now connected.
    pub bot_id: String,
    /// The executor address it points at.
    pub base_url: String,
    /// Which agent the executor was told to run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// The space's bot-secret name carrying the executor's bearer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_name: Option<String>,
    /// When the connection was saved.
    pub connected_at: String,
}

/// The reply of `bots.disconnectExternal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentDisconnectedReply {
    /// The bot it disconnected.
    pub bot_id: String,
    /// Always true when the handler returns.
    pub disconnected: bool,
}

/// The reply of `bots.connectAcp` — one local executor now answers for the bot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentConnectedReply {
    /// The bot now connected.
    pub bot_id: String,
    /// The known executor id it runs on.
    pub executor: String,
    /// When the attachment was saved.
    pub attached_at: String,
}

/// The ACP executors detected on this deployment's machine (REQ-061).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedExecutorsReply {
    /// The compiled-in executor ids found on this machine's PATH.
    pub executors: Vec<String>,
}

/// The reply of `bots.disconnectAcp`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentDisconnectedReply {
    /// The bot it detached.
    pub bot_id: String,
    /// Always true when the handler returns.
    pub detached: bool,
}

/// One sign-in request, as returned over RPC (never plaintext secrets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInRequestView {
    /// The id to refer to this request later.
    pub id: String,
    /// The bot asking.
    pub bot_id: String,
    /// The site a URL origin.
    pub origin: String,
    /// How a person should read it.
    pub display_name: String,
    /// What state the request is in.
    pub status: String,
    /// When the request was filed.
    pub created_at: String,
}

/// The reply of `bots.signIns`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotsSignInsReply {
    /// Newest first.
    pub requests: Vec<SignInRequestView>,
}

/// The reply of `bots.requestSignIn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInRequestedReply {
    /// The id of the new request.
    pub id: String,
    /// The display name of the site.
    pub display_name: String,
    /// The site origin.
    pub origin: String,
}

/// The reply of `bots.externalAgentStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentStatusReply {
    /// Whether the bot points at an external executor.
    pub connected: bool,
    /// The address the bot points at, when it does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// The agent name the executor was told to run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// The secret name carrying the bearer, when one is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_name: Option<String>,
    /// When the connection was saved, when it exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<String>,
}

/// The reply for `bots.externalAgentStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentStatusView {
    /// Whether the bot points at an external executor.
    pub connected: bool,
    /// The address the bot points at, when it does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// The agent name configured for the executor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// The secret name carrying the bearer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_name: Option<String>,
    /// When the connection was saved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<String>,
}

/// The reply of `notifications.status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsStatusReply {
    /// Whether the deployment is sending pushes.
    pub enabled: bool,
    /// The browser-facing public key, present only when pushes are enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vapid_public_key: Option<String>,
}

/// The reply of `notifications.subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsSubscribeReply {
    /// The subscription id.
    pub id: String,
}

/// The reply of `notifications.unsubscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsUnsubscribeReply {
    /// Whether a row was removed.
    pub removed: bool,
}

/// The per-bot policy view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)] // four independent wiring switches, each engaged separately
pub struct NotificationsPolicyView {
    /// The bot this policy governs.
    pub bot_id: String,
    /// Whether waiting pushes go to this member for this bot.
    pub notify_waiting: bool,
    /// Whether completion (completed/failed) pushes go to this member for this bot.
    pub notify_completed: bool,
    /// The UTC-day cap across this member and this bot.
    pub daily_cap: u32,
    /// Whether this member's pushes also fire into Slack destinations for this bot.
    pub destination_slack_webhook: bool,
    /// Whether this member's pushes also fire into Teams destinations for this bot.
    pub destination_teams_webhook: bool,
}

/// The request of `notifications.policyGet`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationsPolicyGetRequest {
    /// The bot to read.
    pub bot_id: String,
}

/// The request of `notifications.policySet`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationsPolicySetRequest {
    /// The bot to manage.
    pub bot_id: String,
    /// Override waiting pings.
    #[serde(default)]
    pub notify_waiting: Option<bool>,
    /// Override completion pings.
    #[serde(default)]
    pub notify_completed: Option<bool>,
    /// Override the daily cap.
    #[serde(default)]
    pub daily_cap: Option<u32>,
    /// Override the Slack destinations flag.
    #[serde(default)]
    pub destination_slack_webhook: Option<bool>,
    /// Override the Teams destinations flag.
    #[serde(default)]
    pub destination_teams_webhook: Option<bool>,
}

/// A destination view — never the webhook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDestinationView {
    /// The id of the destination.
    pub id: String,
    /// Slack incoming webhook or Teams incoming webhook.
    pub kind: String,
    /// A human name for the channel.
    pub label: String,
    /// When it was created.
    pub created_at: String,
}

/// The reply of `notifications.destinationsList`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsDestinationsListReply {
    /// Newest first.
    pub destinations: Vec<NotificationDestinationView>,
}

/// The reply of `notifications.destinationsCreate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsDestinationsCreateReply {
    /// The created destination, without the webhook.
    pub destination: NotificationDestinationView,
}

/// The reply of `notifications.destinationsDelete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationsDestinationsDeleteReply {
    /// Whether a row was removed.
    pub removed: bool,
}
