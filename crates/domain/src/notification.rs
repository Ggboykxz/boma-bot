//! Push notifications and the per-bot policy (REQ-064, improvement program).

use super::{BotId, SpaceId, UserId};

/// The event class a push announces for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    /// A run paused waiting on a person (an approval or question card).
    Waiting,
    /// The run the member started finished (successfully or with an error).
    Completed,
}

impl NotificationKind {
    /// The wire/DB value.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Completed => "completed",
        }
    }
}

/// A browser subscription to push notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSubscription {
    /// The id of the subscription.
    pub id: String,
    /// The space it was made in.
    pub space_id: SpaceId,
    /// The member it belongs to.
    pub user_id: UserId,
    /// The push service endpoint URL.
    pub endpoint: String,
    /// The client's public encryption key (base64url).
    pub p256dh: String,
    /// The client's auth secret (base64url).
    pub auth: String,
    /// The browser, for display only.
    pub user_agent: Option<String>,
    /// Consecutive failures so far; three permanent ones remove the row.
    pub fail_count: u32,
    /// When it was created.
    pub created_at: String,
    /// When it was last touched by a send.
    pub updated_at: String,
}

/// The subscription as supplied at subscribe time; minted with a fresh id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNotificationSubscription {
    /// The id of the subscription; minted by the app.
    pub id: String,
    /// The space it is created in.
    pub space_id: SpaceId,
    /// The member it belongs to.
    pub user_id: UserId,
    /// The push service endpoint URL.
    pub endpoint: String,
    /// The client's public encryption key (base64url).
    pub p256dh: String,
    /// The client's auth secret (base64url).
    pub auth: String,
    /// The browser, for display only.
    pub user_agent: Option<String>,
    /// When the subscription was created.
    pub created_at: String,
}

/// The per-(space, member, bot) push policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPolicy {
    /// The bot this policy governs.
    pub bot_id: BotId,
    /// Whether waiting (approval/question) pushes are sent.
    pub notify_waiting: bool,
    /// Whether completion (completed or failed) pushes are sent.
    pub notify_completed: bool,
    /// How many pushes this pair may send in a UTC day, across channels via later slices.
    pub daily_cap: u32,
    /// When it was created.
    pub created_at: String,
    /// When it was last updated.
    pub updated_at: String,
}

impl NotificationPolicy {
    /// The default a missing row behaves as: waiting on, completion off, 20 per UTC day.
    ///
    /// `now` is the RFC3339 stamp the caller stamps the placeholder with.
    #[must_use]
    pub fn defaults(bot_id: BotId, now: &str) -> Self {
        Self {
            bot_id,
            notify_waiting: true,
            notify_completed: false,
            daily_cap: 20,
            created_at: now.to_owned(),
            updated_at: now.to_owned(),
        }
    }
}

/// A patch to one policy row; unset fields stay as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetNotificationPolicy {
    /// The bot this policy governs.
    pub bot_id: BotId,
    /// Override waiting pings.
    pub notify_waiting: Option<bool>,
    /// Override completion pings.
    pub notify_completed: Option<bool>,
    /// Override the daily cap.
    pub daily_cap: Option<u32>,
}

/// What came of a delivery attempt, from the store's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationFailureOutcome {
    /// Nothing special to report.
    Ok,
    /// A fresh failure recorded (permanent or not).
    Recorded(u32),
    /// A third permanent failure landed earlier in the day; the row was removed,
    /// like a 404/410 returning again.
    Deleted,
}

#[cfg(test)]
mod tests {
    use super::{
        NotificationDestination, NotificationDestinationKind, NotificationKind, NotificationPolicy,
    };
    use crate::{BotId, SpaceId, UserId};

    #[test]
    fn req_065_destination_kinds_round_trip_their_str() {
        assert_eq!(
            NotificationDestinationKind::SlackWebhook.as_str(),
            "slack_webhook"
        );
        assert_eq!(
            NotificationDestinationKind::TeamsWebhook.as_str(),
            "teams_webhook"
        );
        assert_eq!(
            NotificationDestinationKind::SlackWebhook.webhook_prefix(),
            Some("https://hooks.slack.com/"),
        );
        assert_eq!(
            NotificationDestinationKind::TeamsWebhook.webhook_prefix(),
            None
        );
    }

    #[test]
    fn req_065_destinations_in_domain_are_label_and_kind_only() {
        let destination = NotificationDestination {
            id: "dest_1".to_owned(),
            space_id: SpaceId::new("space_golden".to_owned()).expect("the id"),
            kind: NotificationDestinationKind::SlackWebhook,
            label: "#crew".to_owned(),
            created_by: UserId::new("user_owner".to_owned()).expect("the id"),
            created_at: "2026-11-15T00:00:00Z".to_owned(),
        };
        let serialized = format!("{destination:?}");
        assert!(
            !serialized.contains("hooks.slack.com"),
            "no webhook value in the stored shape"
        );
        assert!(
            !serialized.contains("ciphertext"),
            "no ciphertext field in the public shape"
        );
        assert!(
            !serialized.contains("nonce"),
            "no nonce field in the public shape"
        );
    }

    #[test]
    fn req_064_notification_kinds_are_waiting_and_completed() {
        assert_eq!(NotificationKind::Waiting.as_str(), "waiting");
        assert_eq!(NotificationKind::Completed.as_str(), "completed");
    }

    #[test]
    fn req_064_a_missing_policy_row_behaves_as_the_documented_default() {
        let policy = NotificationPolicy::defaults(
            BotId::new("bot_golden".to_owned()).expect("id"),
            "2026-11-15T00:00:00Z",
        );
        assert!(policy.notify_waiting);
        assert!(!policy.notify_completed);
        assert_eq!(policy.daily_cap, 20);
    }
}

/// A Slack incoming webhook channel.
/// A Microsoft Teams incoming webhook channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationDestinationKind {
    /// Slack incoming webhook (`https://hooks.slack.com/services/...`).
    SlackWebhook,
    /// Teams incoming webhook (any https endpoint the webhook URL points at, by policy).
    TeamsWebhook,
}

impl NotificationDestinationKind {
    /// The wire/DB value.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SlackWebhook => "slack_webhook",
            Self::TeamsWebhook => "teams_webhook",
        }
    }

    /// The shape the webhook URL must match; `None` means any https webhook URL is acceptable.
    #[must_use]
    pub const fn webhook_prefix(&self) -> Option<&'static str> {
        match self {
            Self::SlackWebhook => Some("https://hooks.slack.com/"),
            Self::TeamsWebhook => None,
        }
    }
}

/// A destination, as stored and as returned over RPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDestination {
    /// The id of the destination.
    pub id: String,
    /// The space it belongs to; destinations are space-wide.
    pub space_id: SpaceId,
    /// The kind of incoming webhook it is.
    pub kind: NotificationDestinationKind,
    /// A human label (`#product-releases`).
    pub label: String,
    /// Who created it.
    pub created_by: UserId,
    /// When it was created.
    pub created_at: String,
}

/// A destination as supplied at create time; the webhook is transient in memory only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNotificationDestination {
    /// The id; minted by the app.
    pub id: String,
    /// The space it is created in.
    pub space_id: SpaceId,
    /// The kind of incoming webhook it is.
    pub kind: NotificationDestinationKind,
    /// A human label.
    pub label: String,
    /// The raw webhook URL — sealed on write, never returned by any other path. No `secrecy`
    /// dependency in the domain's own rules; the adapter seals this string on the way in and
    /// never returns it, not even via a getter.
    pub webhook: String,
    /// The creator.
    pub created_by: UserId,
    /// When it was created.
    pub created_at: String,
}

/// The per-(space, member, bot) destination flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationFlags {
    /// Whether workspace Slack destinations receive this member's pushes for this bot.
    pub slack_webhook: bool,
    /// Whether workspace Teams destinations receive this member's pushes for this bot.
    pub teams_webhook: bool,
}
