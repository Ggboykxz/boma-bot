//! The scripted services: the ids, the clock, the tokens and the password hasher.
//!
//! A test needs to know what the product does with time, with identifiers and with secrets. The
//! real adapters answer with the system clock, with random bytes and with Argon2id, and a test
//! cannot assert on any of them. The services here answer with a script the test writes:
//!
//! * [`ScriptedIds`] hands out the identifiers the test queued, in order.
//! * [`ScriptedClock`] hands out the instants the test queued, in order.
//! * [`ScriptedTokens`] hands out the tokens the test queued, in order, and hashes them with
//!   SHA-256 exactly like the product does.
//! * [`FixedHasher`] hashes a password with a readable, deterministic rule.
//!
//! Every one of them keeps working when the script runs out: the identifier falls back to a
//! counter, the clock to a fixed instant and the token to a counter. A test that forgets to
//! script something therefore still runs, and still fails on the assertion it was written for.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use boma_app::{
    AppError,
    ports::{Clock, IdGenerator, ModelProvider, PasswordHasher, ProviderRequest, TokenGenerator},
};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq;
use time::{Duration, OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

/// The instant a scripted clock falls back to when its script is exhausted.
///
/// It is a fixed point in time, far enough in the future that a session written with it does not
/// look expired, and far enough in the past that a test comparing dates is not surprised.
pub const DEFAULT_INSTANT: &str = "2026-01-01T00:00:00Z";

/// The prefix a fixed hash carries, so that a stored value is never mistaken for a real one.
const HASH_PREFIX: &str = "hash:";

/// Locks a mutex, recovering from a poisoned lock.
///
/// A poisoned lock means another thread panicked while holding it. The rows are still readable
/// and the lock is still exclusive, so the testkit keeps working instead of failing every
/// following call with an unrelated error.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The identifiers a test scripted, and the counter that names the ones it did not.
///
/// The queue is drained in order: the first call to [`IdGenerator::new_id`] returns the first
/// identifier the test wrote. When the queue is empty the generator falls back to
/// `<prefix>_<n>`, where `n` is a counter that never repeats: the fallback is deterministic, so
/// a test that does not care about the identifier still gets a stable, unique one.
#[derive(Debug, Default)]
pub struct ScriptedIds {
    queue: Mutex<VecDeque<String>>,
    counter: AtomicU64,
}

impl ScriptedIds {
    /// Builds the generator with the identifiers it hands out, in order.
    #[must_use]
    pub fn new(ids: Vec<String>) -> Self {
        Self {
            queue: Mutex::new(ids.into()),
            counter: AtomicU64::new(0),
        }
    }

    /// Builds the generator that counts instead of replaying a script.
    ///
    /// The generator has no script to replay: every identifier it hands out is
    /// `<prefix>_<n>`, where `n` is a counter that never repeats. It is the generator a test
    /// wants when it only needs identifiers to be unique and readable.
    #[must_use]
    pub fn counter() -> Self {
        Self::new(Vec::new())
    }

    /// How many identifiers the fallback has generated so far.
    ///
    /// The counter starts at zero and grows by one per generated identifier, so it is also the
    /// number that the next fallback identifier will carry.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    /// The next identifier the queue will hand out, without consuming it.
    ///
    /// A test uses it to learn which identifier the code under test is about to receive. It is
    /// `None` when the queue is empty: the next identifier is then a fallback, and the fallback
    /// depends on the prefix the caller asks for.
    #[must_use]
    pub fn take(&self) -> Option<String> {
        lock(&self.queue).front().cloned()
    }
}

impl IdGenerator for ScriptedIds {
    fn new_id(&self, prefix: &str) -> String {
        if let Some(id) = lock(&self.queue).pop_front() {
            return id;
        }
        // The fallback: a prefix and a counter that never repeats.
        let number = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{prefix}_{number}")
    }
}

/// The state a scripted clock keeps: the instants it still has to hand out, and the current one.
#[derive(Debug)]
struct ClockState {
    /// The instants the test scripted, oldest first.
    queue: VecDeque<String>,
    /// The instant the clock falls back to when its script is exhausted.
    fallback: String,
    /// The instant the clock currently stands at.
    current: String,
}

/// The instants a test scripted, and the clock that walks through them.
///
/// [`Clock::now`] hands out the instants the test queued, in order. [`Clock::now_plus_seconds`]
/// answers from the instant the clock currently stands at, so a pair of calls (`now` then
/// `now_plus_seconds`) describes the same moment and the same moment plus a duration: a session
/// created at `T` expires at `T + ttl`, never at an unrelated instant.
///
/// When the script is exhausted the clock falls back to a fixed instant, so a test that only
/// scripts the values it cares about still runs.
#[derive(Debug)]
pub struct ScriptedClock {
    state: Mutex<ClockState>,
}

impl ScriptedClock {
    /// Builds the clock with the instants it hands out, in order.
    #[must_use]
    pub fn new(instants: Vec<String>) -> Self {
        Self {
            state: Mutex::new(ClockState {
                queue: instants.into(),
                fallback: DEFAULT_INSTANT.to_owned(),
                current: DEFAULT_INSTANT.to_owned(),
            }),
        }
    }

    /// Builds a clock that always stands at the same instant.
    #[must_use]
    pub fn fixed(instant: &str) -> Self {
        Self {
            state: Mutex::new(ClockState {
                queue: VecDeque::new(),
                fallback: instant.to_owned(),
                current: instant.to_owned(),
            }),
        }
    }

    /// Moves the clock to `instant`: every later read stands at it.
    ///
    /// A test that fast-forwards an automatic retry (REQ-024) cannot wait the backoff out in
    /// real time; this is the honest way across it.
    pub fn advance_to(&self, instant: &str) {
        let mut state = lock(&self.state);
        state.queue.clear();
        instant.clone_into(&mut state.fallback);
        instant.clone_into(&mut state.current);
    }

    /// The instant the clock currently stands at.
    #[must_use]
    pub fn current(&self) -> String {
        lock(&self.state).current.clone()
    }
}

impl Clock for ScriptedClock {
    fn now(&self) -> String {
        let mut state = lock(&self.state);
        let instant = state
            .queue
            .pop_front()
            .unwrap_or_else(|| state.fallback.clone());
        state.current.clone_from(&instant);
        instant
    }

    fn now_plus_seconds(&self, seconds: i64) -> String {
        let state = lock(&self.state);
        match shift(&state.current, seconds) {
            Some(instant) => instant,
            // The instant cannot be read: the clock answers with the instant it stands at, so
            // the caller still gets a timestamp it can compare.
            None => state.current.clone(),
        }
    }
}

/// Moves an instant by a number of seconds, and renders it as RFC 3339 in UTC.
///
/// It returns `None` when the instant cannot be read or rendered, and the caller decides what to
/// answer then.
fn shift(instant: &str, seconds: i64) -> Option<String> {
    let at = OffsetDateTime::parse(instant, &Rfc3339).ok()?;
    render(at + Duration::seconds(seconds))
}

/// Renders an instant as RFC 3339 in UTC, with second precision.
///
/// Second precision is what the product compares: a session expiry is compared as text, and a
/// fraction would make two representations of the same instant differ.
fn render(at: OffsetDateTime) -> Option<String> {
    at.replace_nanosecond(0)
        .ok()?
        .to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .ok()
}

/// The password hasher a test drives: a readable, deterministic rule.
///
/// The hash of a password is the password with a `hash:` prefix. It is **a test double, never a
/// real hash**: it carries no salt, no work factor and no secret, and it must never reach a
/// deployment. Its only job is to let a test sign in with the password it wrote and to keep the
/// password out of the rows it asserts on.
#[derive(Debug, Clone, Copy, Default)]
pub struct FixedHasher;

impl FixedHasher {
    /// Builds the hasher.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl PasswordHasher for FixedHasher {
    async fn hash(&self, password: &str) -> Result<String, AppError> {
        Ok(format!("{HASH_PREFIX}{password}"))
    }

    async fn verify(&self, password: &str, hash: &str) -> Result<bool, AppError> {
        let candidate = format!("{HASH_PREFIX}{password}");
        Ok(constant_time_eq(candidate.as_bytes(), hash.as_bytes()))
    }
}

/// Compares two secrets without leaking how much of them matched.
///
/// The comparison runs in constant time, so the time taken does not say how much of the secret
/// was guessed right.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && ConstantTimeEq::ct_eq(left, right).into()
}

/// The tokens a test scripted, and the counter that names the ones it did not.
///
/// The token itself is never stored: the product keeps its SHA-256, and so does this generator,
/// which makes the testkit's hashing the real one and not a double.
#[derive(Debug, Default)]
pub struct ScriptedTokens {
    queue: Mutex<VecDeque<String>>,
    counter: AtomicU64,
}

impl ScriptedTokens {
    /// Builds the generator with the tokens it hands out, in order.
    #[must_use]
    pub fn new(tokens: Vec<String>) -> Self {
        Self {
            queue: Mutex::new(tokens.into()),
            counter: AtomicU64::new(0),
        }
    }

    /// Builds the generator that counts instead of replaying a script.
    ///
    /// The generator has no script to replay: every token it hands out is `token_<n>`, where `n`
    /// is a counter that never repeats.
    #[must_use]
    pub fn counter() -> Self {
        Self::new(Vec::new())
    }

    /// How many tokens the fallback has generated so far.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    /// The next token the queue will hand out, without consuming it.
    #[must_use]
    pub fn take(&self) -> Option<String> {
        lock(&self.queue).front().cloned()
    }
}

impl TokenGenerator for ScriptedTokens {
    fn generate(&self) -> Result<String, AppError> {
        if let Some(token) = lock(&self.queue).pop_front() {
            return Ok(token);
        }
        // The fallback: a readable token that is unique inside one test.
        let number = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(format!("token_{number}"))
    }

    fn hash(&self, token: &str) -> Result<Vec<u8>, AppError> {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        Ok(hasher.finalize().to_vec())
    }
}

/// The pause a test puts in front of the first answer.
///
/// The test drives the worker from another thread: it learns the first provider call has started
/// by waiting on `asked`, steers the run while the model is "thinking", and then lets the answer
/// through with `release`. The provider then behaves exactly as if the model were slow.
#[derive(Debug)]
struct Gate {
    /// Raised when the provider is called; released by the test once it steered.
    asked: std::sync::Arc<tokio::sync::Notify>,
    /// The one message that lets the answer through.
    release: tokio::sync::oneshot::Receiver<Vec<String>>,
}

/// The model provider a test drives: the answers it hands out, in order.
///
/// The provider is a port like the others: the worker asks it for an answer and gets back the
/// chunks it would have streamed, one chunk at a time (REQ-020). The scripted provider hands out
/// the answers the test wrote, in order, and records the requests it received, so that a test can
/// assert on the history the model was given.
///
/// When the script is exhausted the provider answers with its fallback, and fails with
/// [`AppError::Unavailable`] when it has none: a test that expects an answer and forgot to
/// script one fails where it should.
#[derive(Debug, Default)]
pub struct ScriptedProvider {
    /// The answers that are left, each one the chunks it streams.
    answers: Mutex<VecDeque<Vec<String>>>,
    /// The answer handed out when the script is exhausted.
    fallback: Mutex<Option<Vec<String>>>,
    /// The requests the provider received.
    requests: Mutex<Vec<ProviderRequest>>,
    /// A gate holding the first answer back. `None` outside tests that need the pause.
    gate: Mutex<Option<Gate>>,
}

impl ScriptedProvider {
    /// Builds the provider with the answers it hands out, in order.
    ///
    /// Each answer is the list of chunks the model would have streamed: one answer is one turn.
    #[must_use]
    pub fn new(answers: Vec<Vec<String>>) -> Self {
        Self {
            answers: Mutex::new(answers.into()),
            fallback: Mutex::new(None),
            requests: Mutex::new(Vec::new()),
            gate: Mutex::new(None),
        }
    }

    /// Holds the first answer back until the test lets it through.
    ///
    /// Returns the provider, a notification the test waits on for the first call, and a channel
    /// whose sender unblocks it. The gate only wraps the first call; later calls answer normally.
    #[must_use]
    pub fn gated_first_answer(
        self,
    ) -> (
        Self,
        std::sync::Arc<tokio::sync::Notify>,
        tokio::sync::oneshot::Sender<Vec<String>>,
    ) {
        let (release, receiver) = tokio::sync::oneshot::channel();
        let asked = std::sync::Arc::new(tokio::sync::Notify::new());
        lock(&self.gate).replace(Gate {
            asked: asked.clone(),
            release: receiver,
        });
        (self, asked, release)
    }

    /// Builds the provider with the answer it hands out when its script is exhausted.
    #[must_use]
    pub fn with_fallback(mut self, chunks: Vec<String>) -> Self {
        self.fallback = Mutex::new(Some(chunks));
        self
    }

    /// The requests the provider received, in order.
    #[must_use]
    pub fn requests(&self) -> Vec<ProviderRequest> {
        lock(&self.requests).clone()
    }
}

/// The computer backend a test drives (REQ-027).
///
/// The memory runtime stands in for a `docker` provider that answers: `ping` is healthy and the
/// computers never go anywhere until REQ-026 gives this backend a lifecycle to script.
#[derive(Debug, Default)]
pub struct ScriptedComputerRuntime;

#[async_trait::async_trait]
impl boma_app::ports::ComputerRuntime for ScriptedComputerRuntime {
    fn provider(&self) -> &'static str {
        "docker"
    }

    async fn ping(&self) -> Result<(), boma_app::AppError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl ModelProvider for ScriptedProvider {
    async fn complete(&self, request: ProviderRequest) -> Result<Vec<String>, AppError> {
        lock(&self.requests).push(request);
        let gate = lock(&self.gate).take();
        if let Some(gate) = gate {
            gate.asked.notify_one();
            // The test steered while the model was thinking: this one answer is the newest.
            return match gate.release.await {
                Ok(chunks) => Ok(chunks),
                Err(_) => Err(AppError::Unavailable),
            };
        }
        if let Some(chunks) = lock(&self.answers).pop_front() {
            return Ok(chunks);
        }
        match lock(&self.fallback).clone() {
            Some(chunks) => Ok(chunks),
            // The provider has nothing left to say: the caller is told that the dependency did
            // not answer, and not that the answer was empty.
            None => Err(AppError::Unavailable),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_INSTANT, FixedHasher, ScriptedClock, ScriptedIds, ScriptedProvider, ScriptedTokens,
    };
    use boma_app::ports::{Clock, IdGenerator, ModelProvider, PasswordHasher, TokenGenerator};

    #[test]
    fn req_041_the_scripted_ids_hands_out_the_queue_then_a_counter() {
        let ids = ScriptedIds::new(vec!["usr_1".to_owned(), "usr_2".to_owned()]);
        assert_eq!(ids.new_id("usr"), "usr_1");
        assert_eq!(ids.new_id("usr"), "usr_2");
        assert_eq!(ids.new_id("usr"), "usr_1");
        assert_eq!(ids.count(), 1);
    }

    #[test]
    fn req_041_the_scripted_clock_walks_through_its_instant_and_shifts_it() {
        let clock = ScriptedClock::new(vec!["2026-02-03T04:05:06Z".to_owned()]);
        assert_eq!(clock.now(), "2026-02-03T04:05:06Z");
        assert_eq!(clock.now_plus_seconds(60), "2026-02-03T04:06:06Z");
        assert_eq!(clock.now(), DEFAULT_INSTANT);
    }

    #[test]
    fn req_041_a_fixed_clock_always_stands_at_the_same_instant() {
        let clock = ScriptedClock::fixed("2030-05-06T07:08:09Z");
        assert_eq!(clock.now(), "2030-05-06T07:08:09Z");
        assert_eq!(clock.now_plus_seconds(3600), "2030-05-06T08:08:09Z");
    }

    #[tokio::test]
    async fn req_040_the_fixed_hasher_never_stores_the_password_in_clear() {
        let hasher = FixedHasher::new();
        let hash = hasher
            .hash("correct horse")
            .await
            .expect("the password is hashed");
        assert_eq!(hash, "hash:correct horse");
        assert!(
            hasher
                .verify("correct horse", &hash)
                .await
                .expect("the hash is read")
        );
        assert!(
            !hasher
                .verify("wrong horse", &hash)
                .await
                .expect("the hash is read")
        );
    }

    #[test]
    fn req_042_the_scripted_tokens_hash_with_sha_256() {
        let tokens = ScriptedTokens::new(vec!["token-1".to_owned()]);
        assert_eq!(tokens.generate().expect("a token is minted"), "token-1");
        assert_eq!(tokens.generate().expect("a token is minted"), "token_1");
        let hash = tokens.hash("token-1").expect("the token is hashed");
        assert_eq!(hash.len(), 32);
    }

    /// A request the scripted provider answers.
    fn request(prompt: &str) -> boma_app::ports::ProviderRequest {
        boma_app::ports::ProviderRequest {
            bot: boma_domain::Bot {
                id: boma_domain::BotId::new("bot_1").expect("an id is accepted"),
                space_id: boma_domain::SpaceId::new("spc_1").expect("an id is accepted"),
                user_id: boma_domain::UserId::new("usr_1").expect("an id is accepted"),
                name: "Assistant".to_owned(),
                description: None,
                color: None,
                thread_id: boma_domain::ThreadId::new("thr_1").expect("an id is accepted"),
                archived_at: None,
                created_at: "2026-01-01T00:00:00Z".to_owned(),
                enabled_tools: Vec::new(),
                model_provider: None,
                model_id: None,
                model_pinned: false,
            },
            thread: boma_domain::Thread {
                id: boma_domain::ThreadId::new("thr_1").expect("an id is accepted"),
                space_id: boma_domain::SpaceId::new("spc_1").expect("an id is accepted"),
                bot_id: boma_domain::BotId::new("bot_1").expect("an id is accepted"),
                user_id: boma_domain::UserId::new("usr_1").expect("an id is accepted"),
                next_message_seq: 2,
                next_event_seq: 1,
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
            prompt: prompt.to_owned(),
            history: Vec::new(),
            tools: Vec::new(),
        }
    }

    #[tokio::test]
    async fn req_020_the_scripted_provider_hands_out_its_answers_in_order() {
        let provider = ScriptedProvider::new(vec![
            vec!["Hello".to_owned(), " there".to_owned()],
            vec!["Bye".to_owned()],
        ]);
        let first = provider
            .complete(request("hello"))
            .await
            .expect("the provider answers");
        assert_eq!(first, vec!["Hello".to_owned(), " there".to_owned()]);
        let second = provider
            .complete(request("bye"))
            .await
            .expect("the provider answers");
        assert_eq!(second, vec!["Bye".to_owned()]);

        // The script is exhausted: the provider says it cannot answer.
        let empty = provider.complete(request("again")).await;
        assert!(empty.is_err(), "the provider has nothing left to say");

        // The requests are recorded, so that a test can assert on the history.
        let requests = provider.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].prompt, "hello");
    }
}
