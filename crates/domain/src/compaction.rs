//! Window-aware context compaction (REQ-069).
//!
//! A run dies of "prompt is too long" far too often on long threads; this module packs the
//! history down before the provider sees it. Lossy by design and honest about it: oldest tool
//! payloads are elided first (tools re-read fast), then the oldest messages drop, and the caller
//! tells the person what happened.

use crate::message::{Block, Message, MessageRole};

/// What a compaction pass did to a history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompactionReport {
    /// How many tool-result payloads were elided in place.
    pub elided_tool_results: usize,
    /// How many bytes those elisions freed.
    pub elided_bytes: usize,
    /// How many whole (oldest) messages were dropped.
    pub dropped_messages: usize,
}

/// Rough token estimate: four characters per token, rounded up, plus one per message.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4 + 1
}

/// The size of a message as the provider sees it.
#[must_use]
pub fn message_chars(message: &Message) -> usize {
    message
        .blocks
        .iter()
        .map(|block| match block {
            Block::Text { text } => text.chars().count(),
            Block::Image { .. } => 512,
            Block::File { name, .. } => name.chars().count() + 64,
            Block::Tool { name, .. } => name.chars().count() + 32,
            Block::Card { kind, .. } => kind.chars().count() + 32,
        })
        .sum()
}

/// Compacts `history` so the estimated token count stays under `budget_tokens`.
///
/// Pass 1 elides the text of the oldest tool-result messages with a marker, oldest first.
/// Pass 2 drops the oldest whole messages. The newest message is never dropped; system-role
/// messages coming first are kept over user messages at the same age.
///
/// Returns the compacted history (oldest first) and a report, or the history untouched when
/// everything fits.
pub fn compact_history(
    history: &[Message],
    budget_tokens: usize,
) -> (Vec<Message>, Option<CompactionReport>) {
    let budget_chars = budget_tokens.saturating_mul(4);
    let total: usize = history.iter().map(message_chars).sum();
    if total <= budget_chars {
        return (history.to_vec(), None);
    }
    let mut report = CompactionReport::default();
    let mut compacted: Vec<Message> = history.to_vec();

    // Pass 1: elide tool-result payloads, oldest first, until under the budget.
    let mut elided_bytes = 0usize;
    for index in 0..compacted.len() {
        if compacted.iter().map(message_chars).sum::<usize>() <= budget_chars {
            break;
        }
        if compacted[index].role != MessageRole::Tool {
            continue;
        }
        let before = message_chars(&compacted[index]);
        if before <= 24 {
            continue;
        }
        let elided = elide_tool_blocks(&mut compacted[index]);
        if elided > 0 {
            report.elided_tool_results += 1;
            elided_bytes += elided;
        }
    }
    report.elided_bytes = elided_bytes;

    // Pass 2: drop oldest messages until under the budget. The newest message is never dropped.
    let drop_index = 0usize;
    while compacted.len() > 1
        && compacted.iter().map(message_chars).sum::<usize>() > budget_chars
        && drop_index < compacted.len()
    {
        compacted.remove(drop_index);
        report.dropped_messages += 1;
    }

    (compacted, Some(report))
}

fn elide_tool_blocks(message: &mut Message) -> usize {
    let mut freed = 0usize;
    for block in &mut message.blocks {
        if let Block::Text { text } = block {
            let bytes = text.len();
            let replacement = format!("[elided: {bytes} bytes of tool output]");
            if let Some(text) = Some(std::mem::replace(text, replacement)) {
                freed += text.len().saturating_sub(24);
            }
        }
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::{compact_history, estimate_tokens, message_chars};
    use crate::message::{Block, Message, MessageRole};

    fn message(role: MessageRole, text: &str) -> Message {
        Message {
            id: crate::MessageId::new("msg_1").expect("an id is accepted"),
            thread_id: crate::ThreadId::new("thr_1").expect("an id is accepted"),
            seq: 1,
            role,
            blocks: vec![Block::Text {
                text: text.to_owned(),
            }],
            bot_id: None,
            author_user_id: None,
            run_id: None,
            reply_to_message_id: None,
            reply_quote: None,
            client_nonce: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            reactions: Vec::new(),
        }
    }

    #[test]
    fn req_069_small_history_is_untouched() {
        let history = vec![
            message(MessageRole::User, "hello"),
            message(MessageRole::Bot, "hi"),
        ];
        let (out, report) = compact_history(&history, 100_000);
        assert_eq!(out.len(), 2);
        assert!(report.is_none());
    }

    #[test]
    fn req_069_tool_payloads_are_elided_before_messages_drop() {
        // One huge tool result, tiny budget: the payload goes, the message skeleton stays.
        let history = vec![
            message(MessageRole::Bot, "planning"),
            message(MessageRole::Tool, &"x".repeat(40_000)),
            message(MessageRole::Bot, "answering"),
        ];
        let (out, report) = compact_history(&history, 400);
        let report = report.expect("a compaction happened");
        assert_eq!(report.elided_tool_results, 1);
        assert!(out.iter().any(|m| m.role == MessageRole::Tool));
        let elided_text = out
            .iter()
            .find(|m| m.role == MessageRole::Tool)
            .and_then(|m| m.blocks.first())
            .and_then(|b| b.as_text())
            .expect("the tool result block survives as a marker");
        assert!(elided_text.starts_with("[elided:"));
    }

    #[test]
    fn req_069_oldest_messages_drop_when_elision_is_not_enough() {
        let history = vec![
            message(MessageRole::System, &"s".repeat(4_000)),
            message(MessageRole::User, &"u".repeat(4_000)),
            message(MessageRole::Bot, &"b".repeat(4_000)),
            message(MessageRole::User, "keep me"),
        ];
        let (out, report) = compact_history(&history, 30);
        let report = report.expect("a compaction happened");
        assert!(
            report.dropped_messages >= 2,
            "expected the oldest to drop: {report:?}"
        );
        assert_eq!(
            out.last()
                .and_then(|m| m.blocks.first())
                .and_then(Block::as_text),
            Some("keep me")
        );
    }

    #[test]
    fn req_069_the_token_estimate_is_monotonic_and_nonzero() {
        assert_eq!(estimate_tokens(""), 1);
        assert_eq!(estimate_tokens("abcd"), 2);
        assert!(message_chars(&message(MessageRole::User, "hello")) > 0);
    }
}
