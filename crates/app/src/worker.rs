//! The worker: it claims queued runs and answers them (REQ-001, REQ-012, REQ-020).
//!
//! One executor at a time: the claim is atomic, the lease carries a fence, and a worker that was
//! slow past its lease cannot write. The answer is streamed: every chunk the provider produces is
//! published as a `message-update` event before the run completes, so a client sees the reply grow
//! (REQ-020).

use std::sync::Arc;

use boma_domain::{RunStatus, TenantContext};

use crate::{
    App, AppError, Failure,
    ports::{ModelProvider, ProviderRequest},
    usecases,
};

/// How many passes a worker may fold steering text into one run (REQ-003).
///
/// A steering message that arrives after the fourth pass is answered by the next turn instead of
/// being folded into this one: the loop terminates even if the member keeps typing.
const MAX_STEERING_PASSES: u32 = 4;

/// What a provider pass did, so the caller can steer the next one.
#[derive(Debug)]
enum PassOutcome {
    /// The chunks were streamed and appended; `latest_seq` is the newest message read.
    Answered { text: String, latest_seq: i64 },
    /// The person cancelled while the model was working.
    Cancelled,
    /// The provider did not answer: the run is already marked failed.
    Failed,
}

/// The name a worker claims runs under.
#[derive(Clone)]
pub struct Worker {
    /// The application the worker drives.
    app: Arc<App>,
    /// The provider that answers.
    provider: Arc<dyn ModelProvider>,
    /// The name this worker claims under.
    owner: String,
}

impl Worker {
    /// Builds a worker.
    #[must_use]
    pub fn new(app: Arc<App>, provider: Arc<dyn ModelProvider>, owner: String) -> Self {
        Self {
            app,
            provider,
            owner,
        }
    }

    /// Claims one queued run and answers it.
    ///
    /// Returns `false` when there was nothing to do: the queue is empty, or every queued run is
    /// already leased by another worker.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] when a store cannot answer. A run that fails is *failed*, not left
    /// running: the failure is recorded on the run so the chat can say why (REQ-021).
    pub async fn step(&self) -> Result<bool, AppError> {
        let Some(claimed) = usecases::claim_run(&self.app, &self.owner).await? else {
            return Ok(false);
        };

        let run = claimed.run.clone();
        let context = TenantContext::System {
            reason: boma_domain::SystemReason::ScheduledJob,
            user_id: Some(run.user_id.clone()),
        };

        // The lease is taken: from `leased` the run may only become `running` or go back to
        // `queued` (the state machine decides, the store only applies).
        usecases::set_run_status(
            &self.app,
            &context,
            run.id.as_str(),
            RunStatus::Running,
            Some(run.lease_fence),
            None,
        )
        .await?;

        // One answer per pass, and a fresh pass whenever the member steered while the model was
        // working (REQ-003): the newest text joins the current attempt instead of queueing a
        // second run. Bounded so a member cannot hold the worker in a loop forever.
        let mut text = String::new();
        let mut passes: u32 = 0;
        loop {
            match self.provider_pass(&context, &claimed).await? {
                PassOutcome::Answered {
                    text: chunk_text,
                    latest_seq,
                } => {
                    text.push_str(&chunk_text);
                    self.app
                        .stores
                        .runs
                        .set_steering_consumed(&context, &run.id, latest_seq, Some(run.lease_fence))
                        .await?;
                    let reloaded = self
                        .app
                        .stores
                        .runs
                        .find(&context, &run.id)
                        .await?
                        .ok_or(AppError::Unavailable)?;
                    if reloaded.steering_pending() && passes < MAX_STEERING_PASSES {
                        passes += 1;
                        continue;
                    }
                    break;
                }
                PassOutcome::Cancelled | PassOutcome::Failed => return Ok(true),
            }
        }

        if !self.still_live(&context, &claimed.run).await? {
            tracing::info!(run = %claimed.run.id, "the run was cancelled; the reply is not written and the run stays cancelled");
            return Ok(true);
        }
        usecases::write_reply(
            &self.app,
            &context,
            &claimed.thread,
            &claimed.bot,
            &claimed.run.id,
            &text,
        )
        .await?;
        usecases::set_run_status(
            &self.app,
            &context,
            run.id.as_str(),
            RunStatus::Completed,
            Some(run.lease_fence),
            None,
        )
        .await?;
        // A routine that woke this run: reset its failure counter, so one clean run unpauses
        // the routine at the next threshold crossing (REQ-024).
        usecases::record_routine_completed(&self.app, &run).await?;

        Ok(true)
    }

    /// One provider pass over the thread (REQ-003).
    ///
    /// Reads the newest page, calls the provider once and streams the chunks out. Returns what
    /// happened, so the caller can decide whether to wait for more steering or stop.
    async fn provider_pass(
        &self,
        context: &TenantContext,
        claimed: &usecases::ClaimedRun,
    ) -> Result<PassOutcome, AppError> {
        let run = claimed.run.clone();
        let history = self
            .app
            .stores
            .messages
            .page(context, &run.thread_id, None, 50)
            .await
            .map(|page| page.messages)
            .unwrap_or_default();
        let latest = history.iter().map(|message| message.seq).max().unwrap_or(0);
        let prompt = prompt_of(&history);
        // The catalogue is built per pass: the bot's grant and the space's installs can change
        // between one pass and the next (REQ-016).
        let tools = usecases::tool_catalogue(&self.app, context, claimed.bot.id.as_str()).await?;
        let request = ProviderRequest {
            bot: claimed.bot.clone(),
            thread: claimed.thread.clone(),
            prompt,
            history,
            tools,
        };

        match self.provider.complete(request).await {
            Ok(chunks) => {
                let mut text = String::new();
                for chunk in &chunks {
                    if !self.still_live(context, &claimed.run).await? {
                        tracing::info!(run = %claimed.run.id, "the run was cancelled mid-stream; the reply is not written");
                        return Ok(PassOutcome::Cancelled);
                    }
                    text.push_str(chunk);
                    self.app
                        .stores
                        .events
                        .append(
                            context,
                            crate::ports::NewEvent {
                                space_id: claimed.thread.space_id.clone(),
                                thread_id: claimed.thread.id.clone(),
                                kind: boma_domain::EventType::MessageUpdate,
                                payload: serde_json::json!({
                                    "messageId": claimed.run.id.as_str(),
                                    "delta": chunk,
                                }),
                                created_at: self.app.services.clock.now(),
                            },
                        )
                        .await?;
                }
                Ok(PassOutcome::Answered {
                    text,
                    latest_seq: latest,
                })
            }
            Err(error) => {
                tracing::warn!(run = %run.id, %error, "a run failed");
                self.fail_run(context, &run, error).await?;
                // A routine-woken run that failed counts toward the routine's failure budget;
                // past the threshold the routine pauses itself (REQ-024).
                usecases::record_routine_failed(&self.app, &run).await?;
                Ok(PassOutcome::Failed)
            }
        }
    }

    /// Whether the run has not reached a terminal state yet.
    async fn still_live(
        &self,
        context: &TenantContext,
        run: &boma_domain::Run,
    ) -> Result<bool, AppError> {
        Ok(self
            .app
            .stores
            .runs
            .find(context, &run.id)
            .await?
            .is_some_and(|run| !run.status.is_terminal()))
    }

    /// Records a failure on the run and stops it.
    async fn fail_run(
        &self,
        context: &TenantContext,
        run: &boma_domain::Run,
        error: AppError,
    ) -> Result<(), AppError> {
        let reason = Failure::from(error);
        tracing::warn!(run = %run.id, reason = reason.reason, detail = %reason.detail, "a run failed");
        self.app
            .stores
            .runs
            .set_status(
                context,
                &run.id,
                RunStatus::Failed,
                &self.app.services.clock.now(),
                Some(run.lease_fence),
                // The chat shows this to the person when a run fails (REQ-021): it names the rule,
                // never the value that broke it, and never a driver's internals.
                Some(reason.detail),
            )
            .await
    }
}

/// The prompt for the provider: the newest thing a person wrote, or the newest text of any
/// role when the thread only holds the system's words.
///
/// The history page is newest first, so the first match is the freshest one.
fn prompt_of(history: &[boma_domain::Message]) -> String {
    let text = |message: &boma_domain::Message| {
        message
            .blocks
            .iter()
            .find_map(boma_domain::Block::as_text)
            .map(str::to_owned)
    };
    history
        .iter()
        .find(|message| message.role == boma_domain::MessageRole::User)
        .and_then(text)
        .or_else(|| history.iter().find_map(text))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use boma_domain::{Block, Message, MessageId, MessageRole, ThreadId};

    use super::prompt_of;

    fn message(text: &str) -> Message {
        Message {
            id: MessageId::new("msg_1").expect("an id is accepted"),
            thread_id: ThreadId::new("thr_1").expect("an id is accepted"),
            seq: 1,
            role: MessageRole::User,
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
    fn req_020_the_prompt_is_the_last_thing_the_person_wrote() {
        // The history page is newest first: the fresh text sits at the front.
        let history = vec![message("second"), message("first")];
        assert_eq!(prompt_of(&history), "second");
    }

    #[test]
    fn req_020_the_prompt_is_the_newest_thing_the_person_wrote() {
        // The history page is newest first: the prompt must be the freshest user text, not the
        // oldest (a regression here lost every steering message at the provider, REQ-003).
        fn message(text: &str, role: MessageRole, seq: i64) -> Message {
            Message {
                id: MessageId::new(format!("msg_{seq}")).expect("an id is accepted"),
                thread_id: ThreadId::new("thr_1").expect("an id is accepted"),
                seq,
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
        let history = vec![
            message("third", MessageRole::User, 3),
            message("bot", MessageRole::Bot, 2),
            message("first", MessageRole::User, 1),
        ];
        assert_eq!(prompt_of(&history), "third");
    }

    #[test]
    fn req_020_an_empty_history_yields_an_empty_prompt() {
        assert_eq!(prompt_of(&[]), "");
    }
}
