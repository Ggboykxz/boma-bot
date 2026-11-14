//! Messaging platforms (REQ-048): signature verification and inbound payloads.
//!
//! One deployment talks to one platform instance at a time per installation, and each ingress
//! path owns exactly one job: prove the caller is the platform, then hand the verified message
//! to the application. Slack signs every request (`X-Slack-Signature`, an HMAC-SHA256 over
//! `v0:<timestamp>:<body>` with the deployment's signing secret) and carries a ±5 minute clock;
//! Telegram presents the per-installation secret as a header; a generic HMAC-SHA256 signature
//! covers the Discord-class surfaces that sign bodies. Constant-time comparison everywhere,
//! body caps at the edge, and every failure looks like every other failure.
//!
//! Microsoft Teams is deliberately out of scope for this build: adding it is one more adapter
//! and one more verifier, nothing structural.

use serde::{Deserialize, Serialize};

use crate::DomainError;

/// The platforms this build speaks. `generic` is the HMAC-SHA256 shape Discord-class surfaces use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelPlatform {
    /// Slack, with its `v0` signing scheme.
    Slack,
    /// Telegram's bot webhook with a secret header.
    Telegram,
    /// Any HMAC-SHA256 signed surface configured by the member.
    Generic,
}

impl ChannelPlatform {
    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::Telegram => "telegram",
            Self::Generic => "generic",
        }
    }

    /// Parses the wire value.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidName`] for anything else.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "slack" => Ok(Self::Slack),
            "telegram" => Ok(Self::Telegram),
            "generic" => Ok(Self::Generic),
            other => Err(DomainError::InvalidName(format!(
                "unknown messaging platform `{other}`"
            ))),
        }
    }
}

/// The Slack signature version prefix.
pub const SLACK_VERSION: &str = "v0";

/// How far the Slack timestamp may drift from the ingress's clock.
pub const SLACK_MAX_SKEW_SECONDS: i64 = 300;

/// The largest body text carried into a run.
pub const CHANNEL_TEXT_MAX: usize = 8_000;

/// The verified parts of one inbound platform message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelMessage {
    /// The platform it arrived on.
    pub platform: ChannelPlatform,
    /// The workspace/team the installation belongs to (`team_id` for Slack).
    pub workspace_id: String,
    /// The conversation the platform used (channel, chat, or member-chosen id).
    pub conversation_id: String,
    /// The platform's user id of the author.
    pub user_id: String,
    /// The message's plain text, truncated for storage.
    pub text: String,
    /// The platform's message id, when the platform provides one.
    pub message_id: Option<String>,
}

/// Constant-time comparison of two byte slices.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Hex-encodes bytes (lowercase, no separator).
fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The Slack signature the ingress expects for a body.
///
/// `v0=<hex-hmac-sha256(secret, "v0:<timestamp>:<body>")>`
/// Computes the signature. Never fails: HMAC-SHA256 takes a key of any length.
#[must_use]
pub fn slack_signature(secret: &[u8], timestamp: &str, body: &[u8]) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    let Ok(mut mac) = Hmac::<sha2::Sha256>::new_from_slice(secret) else {
        return String::new(); // unreachable: HMAC-SHA256 accepts any key length
    };
    mac.update(SLACK_VERSION.as_bytes());
    mac.update(b":");
    mac.update(timestamp.as_bytes());
    mac.update(b":");
    mac.update(body);
    format!("{SLACK_VERSION}={}", to_hex(&mac.finalize().into_bytes()))
}

/// Whether the Slack signature header proves the body came from Slack.
///
/// The timestamp must be all ASCII digits, inside the ±5-minute window of `now`,
/// and the digest must match in constant time. Nothing else is consulted.
#[must_use]
pub fn verify_slack(secret: &[u8], timestamp: &str, body: &[u8], provided: &str, now: i64) -> bool {
    if timestamp.is_empty() || !timestamp.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let Ok(sent) = timestamp.parse::<i64>() else {
        return false;
    };
    if (now - sent).abs() > SLACK_MAX_SKEW_SECONDS {
        return false;
    }
    let expected = slack_signature(secret, timestamp, body);
    constant_time_eq(expected.as_bytes(), provided.as_bytes())
}

/// Whether a Telegram installation secret arrives on the header unchanged.
#[must_use]
pub fn verify_telegram(expected: &str, provided: &str) -> bool {
    constant_time_eq(expected.as_bytes(), provided.as_bytes())
}

/// Whether the `X-Boma-Signature` header proves the body came from the installation owner.
///
/// `sha256=<hex-hmac-sha256(secret, body)>` compared in constant time.
#[must_use]
pub fn verify_generic(secret: &[u8], body: &[u8], provided: &str) -> bool {
    use hmac::{Hmac, KeyInit, Mac};
    let Some(hex) = provided.strip_prefix("sha256=") else {
        return false;
    };
    if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return false;
    }
    let Ok(mut mac) = Hmac::<sha2::Sha256>::new_from_slice(secret) else {
        return false; // unreachable: HMAC-SHA256 accepts any key length
    };
    mac.update(body);
    let expected = to_hex(&mac.finalize().into_bytes());
    constant_time_eq(expected.as_bytes(), hex.as_bytes())
}

/// The `url_verification` handshake body Slack sends before it will deliver events.
#[derive(Debug, Clone, Deserialize)]
struct SlackUrlVerification {
    /// Must be `url_verification`.
    #[serde(rename = "type")]
    kind: String,
    /// The token Slack expects echoed back.
    challenge: String,
}

/// The challenge Slack expects echoed, when the body is a `url_verification` handshake.
#[must_use]
pub fn slack_url_verification_challenge(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<SlackUrlVerification>(body)
        .ok()
        .filter(|request| request.kind == "url_verification")
        .map(|request| request.challenge)
}

/// One Slack event as the events API wraps it.
#[derive(Debug, Clone, Deserialize)]
struct SlackEnvelope {
    /// `event_callback` for deliveries.
    #[serde(rename = "type")]
    kind: String,
    /// `team_id` — the workspace the delivery belongs to.
    team_id: Option<String>,
    /// The inner event.
    event: Option<SlackEvent>,
}

/// The inner Slack event.
#[derive(Debug, Clone, Deserialize)]
struct SlackEvent {
    /// `message` for author-typed messages.
    #[serde(rename = "type")]
    kind: String,
    /// Subtypes exist (`message_changed`, `bot_message`, ...) and are all refused here.
    subtype: Option<String>,
    /// The user who wrote.
    user: Option<String>,
    /// The channel the message landed in.
    channel: String,
    /// The plain text.
    text: Option<String>,
    /// The message's platform id.
    ts: String,
}

/// Parses a verified Slack event delivery into a [`ChannelMessage`].
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] for handshakes, bot messages, edits, and anything
/// that does not carry a user-typed plain text.
pub fn parse_slack_event(body: &[u8]) -> Result<ChannelMessage, DomainError> {
    let envelope: SlackEnvelope = serde_json::from_slice(body).map_err(|_| {
        DomainError::InvalidName("the Slack delivery is not a valid event".to_owned())
    })?;
    if envelope.kind != "event_callback" {
        return Err(DomainError::InvalidName(
            "the Slack delivery is not an event_callback".to_owned(),
        ));
    }
    let event = envelope.event.ok_or_else(|| {
        DomainError::InvalidName("the Slack delivery carries no event".to_owned())
    })?;
    if event.kind != "message" || event.subtype.is_some() {
        return Err(DomainError::InvalidName(
            "only author-typed messages start work".to_owned(),
        ));
    }
    let text = event.text.unwrap_or_default();
    if text.trim().is_empty() {
        return Err(DomainError::InvalidName(
            "the message carries no text".to_owned(),
        ));
    }
    Ok(ChannelMessage {
        platform: ChannelPlatform::Slack,
        workspace_id: envelope.team_id.unwrap_or_default(),
        conversation_id: event.channel,
        user_id: event.user.unwrap_or_default(),
        text: text.chars().take(CHANNEL_TEXT_MAX).collect(),
        message_id: Some(event.ts),
    })
}

/// One Telegram update.
#[derive(Debug, Clone, Deserialize)]
struct TelegramUpdate {
    /// The message, present for individual messages.
    message: Option<TelegramMessage>,
    /// Edited messages are not new work.
    edited_message: Option<serde_json::Value>,
}

/// The message inside a Telegram update.
#[derive(Debug, Clone, Deserialize)]
struct TelegramMessage {
    /// The chat the message landed in.
    chat: TelegramChat,
    /// The author.
    from: Option<TelegramUser>,
    /// The text, when the message carries one.
    text: Option<String>,
    /// The platform message id.
    message_id: i64,
}

/// The chat the message landed in.
#[derive(Debug, Clone, Deserialize)]
struct TelegramChat {
    /// The chat id; stringified.
    id: i64,
}

/// The author of a Telegram message.
#[derive(Debug, Clone, Deserialize)]
struct TelegramUser {
    /// The author's user id.
    id: i64,
}

/// Parses a verified Telegram update into a [`ChannelMessage`].
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] for non-message updates and empty texts.
pub fn parse_telegram_update(body: &[u8]) -> Result<ChannelMessage, DomainError> {
    let update: TelegramUpdate = serde_json::from_slice(body)
        .map_err(|_| DomainError::InvalidName("the Telegram update is not valid".to_owned()))?;
    if update.edited_message.is_some() {
        return Err(DomainError::InvalidName(
            "edited messages are not new work".to_owned(),
        ));
    }
    let message = update
        .message
        .ok_or_else(|| DomainError::InvalidName("the update carries no message".to_owned()))?;
    let text = message.text.unwrap_or_default();
    if text.trim().is_empty() {
        return Err(DomainError::InvalidName(
            "the message carries no text".to_owned(),
        ));
    }
    Ok(ChannelMessage {
        platform: ChannelPlatform::Telegram,
        // The installation is resolved by the route's path parameter; the payload does not
        // carry the bot identity. This column exists for display only and stays empty here.
        workspace_id: String::new(),
        conversation_id: message.chat.id.to_string(),
        user_id: message
            .from
            .map(|user| user.id.to_string())
            .unwrap_or_default(),
        text: text.chars().take(CHANNEL_TEXT_MAX).collect(),
        message_id: Some(message.message_id.to_string()),
    })
}

/// The generic HMAC body.
#[derive(Debug, Clone, Deserialize)]
struct GenericMessage {
    /// The conversation the message belongs to.
    #[serde(rename = "conversationId")]
    conversation_id: String,
    /// The platform's author id.
    #[serde(rename = "userId")]
    user_id: String,
    /// The plain text.
    text: String,
    /// An optional idempotency key from the platform.
    #[serde(rename = "messageId")]
    message_id: Option<String>,
}

/// Parses a verified generic delivery into a [`ChannelMessage`].
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] for empty texts.
pub fn parse_generic_message(body: &[u8]) -> Result<ChannelMessage, DomainError> {
    let message: GenericMessage = serde_json::from_slice(body)
        .map_err(|_| DomainError::InvalidName("the delivery is not a valid message".to_owned()))?;
    if message.conversation_id.trim().is_empty() {
        return Err(DomainError::InvalidName(
            "the delivery carries no conversation id".to_owned(),
        ));
    }
    if message.text.trim().is_empty() {
        return Err(DomainError::InvalidName(
            "the delivery carries no text".to_owned(),
        ));
    }
    Ok(ChannelMessage {
        platform: ChannelPlatform::Generic,
        workspace_id: String::new(),
        conversation_id: message.conversation_id,
        user_id: message.user_id,
        text: message.text.chars().take(CHANNEL_TEXT_MAX).collect(),
        message_id: message.message_id,
    })
}

/// The external key for a conversation, used verbatim in the route and in uniqueness indexes.
#[must_use]
pub fn external_key(
    platform: ChannelPlatform,
    workspace_id: &str,
    conversation_id: &str,
) -> String {
    format!("{}:{workspace_id}:{conversation_id}", platform.as_str())
}

#[cfg(test)]
mod tests {
    use hmac::{Hmac, KeyInit, Mac};

    use super::{
        CHANNEL_TEXT_MAX, ChannelPlatform, parse_generic_message, parse_slack_event,
        parse_telegram_update, slack_url_verification_challenge, verify_generic, verify_slack,
        verify_telegram,
    };

    #[test]
    fn req_048_a_slack_signature_verifies_and_a_wrong_one_does_not() {
        let body = br#"{"ok":true}"#;
        let now = 1_800_000_000i64;
        let signature = super::slack_signature(b"signing-secret", "1799999700", body);
        assert!(verify_slack(
            b"signing-secret",
            "1799999700",
            body,
            &signature,
            now
        ));
        assert!(!verify_slack(
            b"signing-secret",
            "1799999700",
            body,
            "v0=de00",
            now
        ));
        assert!(!verify_slack(
            b"other-secret",
            "1799999700",
            body,
            &signature,
            now
        ));
    }

    #[test]
    fn req_048_a_stale_slack_timestamp_is_refused() {
        let body = br#"{"ok":true}"#;
        let signature = super::slack_signature(b"s", "1799999400", body);
        // The same signature ten minutes later: replay refused.
        assert!(!verify_slack(
            b"s",
            "1799999400",
            body,
            &signature,
            1_800_000_000
        ));
        // The same signature inside the window: accepted.
        assert!(verify_slack(
            b"s",
            "1799999700",
            body,
            &super::slack_signature(b"s", "1799999700", body),
            1_800_000_000
        ));
        assert!(!verify_slack(
            b"s",
            "not-a-number",
            body,
            &signature,
            1_800_000_000
        ));
    }

    #[test]
    fn req_048_a_slack_url_verification_handshake_is_recognised() {
        let body = br#"{"type":"url_verification","challenge":"abc123"}"#;
        assert_eq!(
            slack_url_verification_challenge(body).as_deref(),
            Some("abc123")
        );
        assert_eq!(
            slack_url_verification_challenge(br#"{"type":"event_callback"}"#),
            None
        );
    }

    #[test]
    fn req_048_a_slack_bot_message_is_refused() {
        let bot_message = br#"{"type":"event_callback","team_id":"T1","event":{"type":"message","subtype":"bot_message","channel":"C1","user":"U1","text":"hello","ts":"1.2"}}"#;
        assert!(parse_slack_event(bot_message).is_err());
        let ok = br#"{"type":"event_callback","team_id":"T1","event":{"type":"message","channel":"C1","user":"U9","text":"ship it","ts":"1234.5678"}}"#;
        let message = parse_slack_event(ok).expect("the event parses");
        assert_eq!(message.platform, ChannelPlatform::Slack);
        assert_eq!(message.workspace_id, "T1");
        assert_eq!(message.conversation_id, "C1");
        assert_eq!(message.user_id, "U9");
        assert_eq!(message.message_id.as_deref(), Some("1234.5678"));
    }

    #[test]
    fn req_048_a_telegram_update_parses() {
        let body = br#"{"update_id":7,"message":{"message_id":42,"from":{"id":424242},"chat":{"id":-100123,"type":"supergroup"},"text":"build the report"}}"#;
        let message = parse_telegram_update(body).expect("the update parses");
        assert_eq!(message.conversation_id, "-100123");
        assert_eq!(message.user_id, "424242");
        assert_eq!(message.message_id.as_deref(), Some("42"));
        assert!(verify_telegram("expected-secret", "expected-secret"));
        assert!(!verify_telegram("expected-secret", "other"));
        let edited = br#"{"update_id":8,"edited_message":{"message_id":42,"chat":{"id":1},"text":"edited"}}"#;
        assert!(parse_telegram_update(edited).is_err());
    }

    #[test]
    fn req_048_a_generic_signature_verifies() {
        let body = br#"{"conversationId":"C","userId":"U","text":"hi"}"#;
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"generic-secret").expect("any length");
        mac.update(body);
        let signature = format!("sha256={}", super::to_hex(&mac.finalize().into_bytes()));
        assert!(verify_generic(b"generic-secret", body, &signature));
        assert!(!verify_generic(b"generic-secret", body, "sha256=00"));
        assert!(!verify_generic(b"generic-secret", body, "md5=00"));
        assert!(!verify_generic(b"other", body, &signature));
        let message = parse_generic_message(body).expect("the message parses");
        assert_eq!(message.text, "hi");
        assert_eq!(message.conversation_id, "C");
        let long = format!(
            r#"{{"conversationId":"C","userId":"U","text":"{}"}}"#,
            "x".repeat(CHANNEL_TEXT_MAX + 10)
        );
        assert_eq!(
            parse_generic_message(long.as_bytes())
                .expect("parses")
                .text
                .chars()
                .count(),
            CHANNEL_TEXT_MAX
        );
    }

    #[test]
    fn req_048_the_platform_wire_values_round_trip() {
        for platform in [
            ChannelPlatform::Slack,
            ChannelPlatform::Telegram,
            ChannelPlatform::Generic,
        ] {
            assert_eq!(
                ChannelPlatform::parse(platform.as_str()).expect("a known platform"),
                platform
            );
        }
        assert!(ChannelPlatform::parse("teams").is_err());
    }
}
