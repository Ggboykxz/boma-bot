//! The seeded world the golden fixtures replay against.
//!
//! The fixtures in `spec/golden/` are captured transactions: a request and the reply the
//! reference produced. To replay them byte for byte, the environment around the request must be
//! the one the capture was made in — the same space, the same bot, the same thread, the same
//! messages, the same ids.
//!
//! This module builds that world. It is deliberately explicit: every id a fixture names is
//! written here, so a fixture that stops matching the world is a visible change rather than a
//! silent drift.
//!
//! The two placeholders a fixture may carry are substituted by the harness, never by the world:
//!
//! * `session=<redacted>` becomes the world's session cookie;
//! * `<synthetic-address>` becomes a real address, because an address is personal data and the
//!   export replaced it.
//!
//! This module is only ever used by the golden replay, which is a test: the `expect` calls below
//! state that a seeded id must be accepted, and a failure there is a bug in the seed, not a
//! runtime condition to handle.
#![allow(clippy::expect_used, clippy::panic)]

use boma_app::{
    App,
    ports::{NewAccount, NewBot, NewOrganization, NewSession, NewSpace, NewThread, NewUser},
};
use boma_domain::{
    Block, Bot, BotId, Card, CardId, CardKind, CardStatus, Email, Event, EventType, InviteId,
    MemoryDocument, Message, MessageId, MessageRole, OrganizationId, Run, RunId, RunStatus,
    RunTrigger, Session, SessionId, SpaceId, SpaceRole, Thread, ThreadId, UserId,
};
use boma_testkit::TestApp;

/// The instant every seeded row carries.
pub const SEED_INSTANT: &str = "2026-01-01T00:00:00Z";

/// The instant the run's start carries.
pub const RUN_STARTED_AT: &str = "2026-01-01T00:00:01Z";

/// The token the fixtures' session cookie carries.
///
/// The fixture says `session=<redacted>`; the harness substitutes this value.
pub const SESSION_TOKEN: &str = "golden-session-token";

/// The space the fixtures act in.
pub const SPACE_ID: &str = "spc_golden";

/// Another space, used by the cross-tenant fixture.
pub const OTHER_SPACE_ID: &str = "spc_other_golden";

/// The bot the fixtures act on.
pub const BOT_ID: &str = "bot_golden";

/// The thread of that bot.
pub const THREAD_ID: &str = "thr_golden";

/// The user the fixtures act as.
pub const USER_ID: &str = "usr_golden";

/// The organization that owns the space.
pub const ORGANIZATION_ID: &str = "org_golden";

/// The session the fixtures sign in with.
pub const SESSION_ID: &str = "ses_golden";

/// The run the fixtures show.
pub const RUN_ID: &str = "run_golden";

/// The world a fixture replays against.
#[derive(Clone)]
pub struct GoldenWorld {
    /// The application under test.
    pub app: App,
    /// The cookie the fixtures send.
    pub cookie: String,
}

/// The ids a procedure creates during a replay, per fixture.
///
/// A fixture whose procedure mints an id needs it here, in the order the procedure mints them; a
/// fixture that only reads needs nothing.
#[must_use]
pub fn created_ids(fixture: &str) -> Vec<String> {
    let ids: &[&str] = match fixture {
        "routine-create" => &["rtn_golden"],
        "routine-once" => &["rtn_once_golden"],
        "send-message" | "send-message-replay" => &["msg_golden", "run_golden"],
        "space-create" => &["spc_new_golden"],
        "bot-create" => &["bot_golden", "thr_golden"],
        "space-invite" => &["inv_golden"],
        _ => &[],
    };
    ids.iter().map(|id| (*id).to_owned()).collect()
}

/// Builds the world for one fixture.
///
/// Returns `None` for the fixtures that need no world: health, capabilities, unauthorized.
///
/// # Panics
///
/// Panics when a seeded id is refused by the domain: that is a bug in the seed, not a runtime
/// condition. The golden replay is a test, so a panic is the honest failure.
pub async fn world_for(fixture: &str) -> Option<GoldenWorld> {
    let needs_world = matches!(
        fixture,
        "send-message"
            | "send-message-replay"
            | "bot-create"
            | "space-create"
            | "space-invite"
            | "thread-messages"
            | "thread-subscribe"
            | "thread-answer"
            | "run-list"
            | "cross-tenant-not-found"
            | "run-retry"
            | "memory-write"
            | "routine-create"
            | "routine-once"
    );
    if !needs_world {
        return None;
    }

    // The capture clock is part of the replay: most fixtures stand at SEED_INSTANT, but a
    // fixture whose capture ran past its scheduled instant states its own here.
    let clock_seed = match fixture {
        "memory-write" => "2026-01-01T00:01:00Z",
        // The fixture's capture landed after the Thursday 08:00 Paris slot, so the next
        // weekday wake is Friday 07:00 UTC, exactly what the golden pins.
        "routine-create" => "2026-01-01T09:00:00Z",
        _ => SEED_INSTANT,
    };
    let test_app = TestApp::builder()
        .ids(created_ids(fixture))
        .fixed_clock(clock_seed)
        .build();
    let app = test_app.app().clone();
    let cookie = format!("session={SESSION_TOKEN}");
    let (user_id, space_id) = seed_account_and_session(&test_app, &app).await;

    match fixture {
        "send-message" | "send-message-replay" | "routine-create" | "routine-once" => {
            seed_bot(&app, &space_id, &user_id).await;
        }
        "bot-create" | "space-create" | "space-invite" => {}
        "thread-messages" | "run-list" => {
            let (bot, thread) = seed_bot(&app, &space_id, &user_id).await;
            seed_history(&test_app, &space_id, &bot, &thread, &user_id);
        }
        "thread-subscribe" => {
            let (bot, thread) = seed_bot(&app, &space_id, &user_id).await;
            seed_events(&test_app, &space_id, &bot, &thread);
        }
        "thread-answer" => {
            let (bot, thread) = seed_bot(&app, &space_id, &user_id).await;
            seed_history(&test_app, &space_id, &bot, &thread, &user_id);
            seed_card(&test_app, &space_id, &thread);
        }
        "memory-write" => {
            let (bot, _thread) = seed_bot(&app, &space_id, &user_id).await;
            seed_memory_document(
                &test_app,
                &space_id,
                bot.id.clone(),
                "notes/commits.md",
                "# Commits\n- previous work\n",
                3,
            );
        }
        "run-retry" => {
            let (bot, thread) = seed_bot(&app, &space_id, &user_id).await;
            seed_failed_run(&test_app, &space_id, &bot, &thread, &user_id);
        }
        "cross-tenant-not-found" => {
            let (bot, thread) = seed_bot(&app, &space_id, &user_id).await;
            let _ = (bot, thread);
            seed_other_space(&app, &user_id).await;
        }
        _ => return None,
    }

    Some(GoldenWorld { app, cookie })
}

/// Creates the account and its session: everyone signs in as the same golden person.
async fn seed_account_and_session(test_app: &TestApp, app: &App) -> (UserId, SpaceId) {
    let user_id = UserId::new(USER_ID).expect("the seed id is valid");
    let organization_id = OrganizationId::new(ORGANIZATION_ID).expect("the seed id is valid");
    let space_id = SpaceId::new(SPACE_ID).expect("the seed id is valid");
    app.stores
        .accounts
        .create_account(NewAccount {
            user: NewUser {
                id: user_id.clone(),
                email: Email::new("person@example.com").expect("the seed address is valid"),
                password_hash: "hash:correct horse battery".to_owned(),
                name: "Person".to_owned(),
                locale: "en-US".to_owned(),
                created_at: SEED_INSTANT.to_owned(),
            },
            organization: NewOrganization {
                id: organization_id.clone(),
                name: "Golden organization".to_owned(),
                created_at: SEED_INSTANT.to_owned(),
            },
            space: NewSpace {
                id: space_id.clone(),
                organization_id: organization_id.clone(),
                name: "Golden space".to_owned(),
                is_default: true,
                created_at: SEED_INSTANT.to_owned(),
            },
        })
        .await
        .expect("the account is created");

    let token_hash = {
        use boma_app::ports::TokenGenerator;
        test_app
            .tokens()
            .hash(SESSION_TOKEN)
            .expect("the token hashes")
    };
    app.stores
        .sessions
        .create(NewSession {
            id: SessionId::new(SESSION_ID).expect("the seed id is valid"),
            user_id: user_id.clone(),
            token_hash,
            expires_at: "2026-02-01T00:00:00Z".to_owned(),
            created_at: SEED_INSTANT.to_owned(),
        })
        .await
        .expect("the session is created");

    (user_id, space_id)
}

/// Creates the bot and its thread with the ids the fixture names.
async fn seed_bot(app: &App, space_id: &SpaceId, user_id: &UserId) -> (Bot, Thread) {
    app.stores
        .bots
        .create_with_thread(
            NewBot {
                id: BotId::new(BOT_ID).expect("the seed id is valid"),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: Some("Reads commits and reports.".to_owned()),
                color: Some("blue".to_owned()),
                created_at: SEED_INSTANT.to_owned(),
            },
            NewThread {
                id: ThreadId::new(THREAD_ID).expect("the seed id is valid"),
                space_id: space_id.clone(),
                bot_id: BotId::new(BOT_ID).expect("the seed id is valid"),
                user_id: user_id.clone(),
                created_at: SEED_INSTANT.to_owned(),
            },
        )
        .await
        .expect("the bot is created")
}

/// Seeds the messages and the run the page and activity fixtures capture.
fn seed_history(
    test_app: &TestApp,
    space_id: &SpaceId,
    bot: &Bot,
    thread: &Thread,
    user_id: &UserId,
) {
    let database = test_app.database();
    database.seed_message(
        space_id.clone(),
        &text_message(
            "msg_golden_1",
            1,
            MessageRole::User,
            "Plan my week.",
            user_id,
        ),
    );
    database.seed_message(
        space_id.clone(),
        &text_message(
            "msg_golden_2",
            2,
            MessageRole::User,
            "Summarise the last three commits.",
            user_id,
        ),
    );
    database.seed_message(
        space_id.clone(),
        &text_message(
            "msg_golden_3",
            3,
            MessageRole::Bot,
            "Three commits, one revert.",
            &bot.user_id,
        ),
    );
    database.seed_run(Run {
        id: RunId::new(RUN_ID).expect("the seed id is valid"),
        space_id: space_id.clone(),
        bot_id: bot.id.clone(),
        thread_id: thread.id.clone(),
        user_id: user_id.clone(),
        status: RunStatus::Running,
        trigger: RunTrigger::User,
        client_nonce: None,
        // The snippet the activity list shows is the message that started the run.
        source_message_id: Some(MessageId::new("msg_golden_2").expect("the seed id is valid")),
        created_at: SEED_INSTANT.to_owned(),
        started_at: Some(RUN_STARTED_AT.to_owned()),
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
    });
}

/// Seeds the three events the stream fixture captures.
fn seed_events(test_app: &TestApp, space_id: &SpaceId, _bot: &Bot, _thread: &Thread) {
    let database = test_app.database();
    database.seed_event(
        space_id.clone(),
        &Event {
            thread_id: ThreadId::new(THREAD_ID).expect("the seed id is valid"),
            seq: 4,
            kind: EventType::Run,
            payload: serde_json::json!({ "runId": RUN_ID, "status": "running" }),
            created_at: "2026-01-01T00:00:04Z".to_owned(),
        },
    );
    database.seed_event(
        space_id.clone(),
        &Event {
            thread_id: ThreadId::new(THREAD_ID).expect("the seed id is valid"),
            seq: 5,
            kind: EventType::MessageUpdate,
            payload: serde_json::json!({ "messageId": "msg_golden_5", "delta": "Three" }),
            created_at: "2026-01-01T00:00:05Z".to_owned(),
        },
    );
    database.seed_event(
        space_id.clone(),
        &Event {
            thread_id: ThreadId::new(THREAD_ID).expect("the seed id is valid"),
            seq: 6,
            kind: EventType::Heartbeat,
            payload: serde_json::json!({}),
            created_at: "2026-01-01T00:00:15Z".to_owned(),
        },
    );
}

/// Seeds the pending approval card the thread-answer fixture answers (REQ-017).
fn seed_card(test_app: &TestApp, space_id: &SpaceId, thread: &Thread) {
    // The run the card blocks is exactly the seeded run the history carries.
    let run_id = RunId::new(RUN_ID).expect("the seed id is valid");
    test_app.database().seed_card(
        space_id.clone(),
        Card {
            id: CardId::new("card_golden").expect("the seed id is valid"),
            space_id: space_id.clone(),
            thread_id: thread.id.clone(),
            run_id: Some(run_id),
            kind: CardKind::Approval,
            status: CardStatus::Pending,
            answer: None,
            created_at: SEED_INSTANT.to_owned(),
            answered_at: None,
        },
    );
}

/// Seeds a memory document with `revision` already written (REQ-022).
fn seed_memory_document(
    test_app: &TestApp,
    space_id: &SpaceId,
    bot_id: BotId,
    path: &str,
    content: &str,
    revision: u64,
) {
    test_app.database().seed_memory_document(&MemoryDocument {
        space_id: space_id.clone(),
        bot_id: Some(bot_id),
        path: path.to_owned(),
        content: content.to_owned(),
        revision,
        updated_at: SEED_INSTANT.to_owned(),
    });
}

/// Seeds the failed run the retry fixture re-queues (REQ-012).
fn seed_failed_run(
    test_app: &TestApp,
    space_id: &SpaceId,
    bot: &Bot,
    thread: &Thread,
    user_id: &UserId,
) {
    test_app.database().seed_run(Run {
        id: RunId::new("run_golden_failed").expect("the seed id is valid"),
        space_id: space_id.clone(),
        bot_id: bot.id.clone(),
        thread_id: thread.id.clone(),
        user_id: user_id.clone(),
        status: RunStatus::Failed,
        trigger: RunTrigger::User,
        client_nonce: None,
        source_message_id: None,
        created_at: SEED_INSTANT.to_owned(),
        started_at: Some(RUN_STARTED_AT.to_owned()),
        completed_at: Some(SEED_INSTANT.to_owned()),
        error: Some("the provider refused the request".to_owned()),
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
    });
}

/// Seeds a second space the caller does not belong to.
async fn seed_other_space(app: &App, _user_id: &UserId) {
    app.stores
        .spaces
        .create(
            NewSpace {
                id: SpaceId::new(OTHER_SPACE_ID).expect("the seed id is valid"),
                organization_id: OrganizationId::new(ORGANIZATION_ID)
                    .expect("the seed id is valid"),
                name: "Another space".to_owned(),
                is_default: false,
                created_at: SEED_INSTANT.to_owned(),
            },
            &UserId::new("usr_other_golden").expect("the seed id is valid"),
        )
        .await
        .expect("the other space is created");
}

/// Builds a message with one text block.
fn text_message(id: &str, seq: i64, role: MessageRole, text: &str, user_id: &UserId) -> Message {
    Message {
        id: MessageId::new(id).expect("the seed id is valid"),
        thread_id: ThreadId::new(THREAD_ID).expect("the seed id is valid"),
        seq,
        role,
        blocks: vec![Block::Text {
            text: text.to_owned(),
        }],
        bot_id: if role == MessageRole::Bot {
            Some(BotId::new(BOT_ID).expect("the seed id is valid"))
        } else {
            None
        },
        author_user_id: if role == MessageRole::User {
            Some(user_id.clone())
        } else {
            None
        },
        run_id: if role == MessageRole::Bot {
            Some(RunId::new(RUN_ID).expect("the seed id is valid"))
        } else {
            None
        },
        reply_to_message_id: None,
        reply_quote: None,
        client_nonce: None,
        created_at: created_at_of(seq),
        reactions: Vec::new(),
    }
}

/// The instant a seeded message carries.
fn created_at_of(seq: i64) -> String {
    format!("2026-01-01T00:00:0{seq}Z")
}

/// The role the seeded member has in the space.
pub const GOLDEN_ROLE: SpaceRole = SpaceRole::Owner;

/// The id of the invitation the invite fixture creates.
///
/// # Panics
///
/// Panics when the id is refused by the domain; a bug in the seed.
#[must_use]
pub fn invitation_id() -> InviteId {
    InviteId::new("inv_golden").expect("the seed id is valid")
}

/// The session the fixtures sign in with.
///
/// # Panics
///
/// Panics when the id is refused by the domain; a bug in the seed.
#[must_use]
pub fn session() -> Session {
    Session {
        id: SessionId::new(SESSION_ID).expect("the seed id is valid"),
        user_id: UserId::new(USER_ID).expect("the seed id is valid"),
        expires_at: "2026-02-01T00:00:00Z".to_owned(),
    }
}
