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
            model_provider: None,
            model_id: None,
            model_pinned: false,
            parent_run_id: None,
        };
        let json = serde_json::to_value(RunView::new(&run, None)).expect("the view serialises");
        assert_eq!(json["promptSnippet"], serde_json::Value::Null);
        assert_eq!(json["startedAt"], serde_json::Value::Null);
        assert_eq!(json["completedAt"], serde_json::Value::Null);
    }
}
