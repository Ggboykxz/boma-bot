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
    ports::{ModelProvider, ProviderRequest, ProviderTurn},
    usecases,
};

/// How many passes a worker may fold steering text into one run (REQ-003).
///
/// A steering message that arrives after the fourth pass is answered by the next turn instead of
/// being folded into this one: the loop terminates even if the member keeps typing.
const MAX_STEERING_PASSES: u32 = 4;

/// How many rounds of tool calls a worker folds into one run (REQ-020).
///
/// The loop always terminates: after this many rounds of calls the run fails legibly instead of
/// looping forever. A turn that only reads tools counts as one round; a single round may carry
/// several calls.
const MAX_TOOL_CALLS: u32 = 8;

/// What a provider pass did, so the caller can steer the next one.
#[derive(Debug)]
enum PassOutcome {
    /// The chunks were streamed and appended; `latest_seq` is the newest message read.
    Answered { text: String, latest_seq: i64 },
    /// Read tools ran and the results are in the history; the model continues.
    ToolsDispatched,
    /// Write-class calls parked the run on an approval card; a person decides.
    Suspended,
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
        let mut tool_passes: u32 = 0;
        loop {
            let outcome = self.provider_pass(&context, &claimed).await?;
            match outcome {
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
                PassOutcome::ToolsDispatched => {
                    tool_passes += 1;
                    if tool_passes > MAX_TOOL_CALLS {
                        let error = AppError::Rule(boma_domain::DomainError::InvalidName(format!(
                            "the run asked for more than {MAX_TOOL_CALLS} rounds of tool calls"
                        )));
                        tracing::warn!(run = %claimed.run.id, %error, "the tool loop hit its cap");
                        self.fail_run(&context, &run, error).await?;
                        self.helper_finished(&context, &run, "the helper run failed")
                            .await;
                        return Ok(true);
                    }
                }
                PassOutcome::Suspended => return Ok(true),
                PassOutcome::Cancelled | PassOutcome::Failed => {
                    let outcome = if matches!(outcome, PassOutcome::Cancelled) {
                        "the helper run was stopped"
                    } else {
                        "the helper run failed"
                    };
                    self.helper_finished(&context, &run, outcome).await;
                    return Ok(true);
                }
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
        // A helper run reports its outcome to the parent thread as a tool message, and the
        // parent re-enters the worker (REQ-019).
        if run.is_helper() {
            usecases::complete_helper_run(&self.app, &context, &run, &text).await?;
        }

        Ok(true)
    }

    /// Builds the scoped memory context for one turn: bot documents first, then the
    /// space-shared ones (REQ-022).
    async fn memory_context(
        &self,
        context: &TenantContext,
        claimed: &usecases::ClaimedRun,
    ) -> Result<Vec<crate::ports::MemoryContext>, AppError> {
        let bot_memory = self
            .app
            .stores
            .memory
            .list(context, Some(&claimed.bot.id))
            .await?
            .into_iter()
            .map(|document| crate::ports::MemoryContext {
                path: document.path,
                content: document.content,
                scope: boma_domain::MemoryScope::Bot,
            });
        let shared_memory = self
            .app
            .stores
            .memory
            .list(context, None)
            .await?
            .into_iter()
            .map(|document| crate::ports::MemoryContext {
                path: document.path,
                content: document.content,
                scope: boma_domain::MemoryScope::Shared,
            });
        Ok(bot_memory.chain(shared_memory).collect())
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
        // A routine-woken run has no person message in the history: the prompt is the routine's.
        let prompt = match &run.routine_id {
            Some(routine_id) => self
                .app
                .stores
                .routines
                .find(context, routine_id)
                .await?
                .map_or_else(|| prompt_of(&history), |routine| routine.prompt),
            None => prompt_of(&history),
        };
        // The catalogue is built per pass: the bot's grant and the space's installs can change
        // between one pass and the next (REQ-016).
        let tools = usecases::tool_catalogue(&self.app, context, claimed.bot.id.as_str()).await?;
        // REQ-022: scoped memory rides into the turn. Bot documents first, then the
        // space-shared ones; the provider logs what it was given, so a test can assert it.
        let memory = self.memory_context(context, claimed).await?;
        let request = ProviderRequest {
            bot: claimed.bot.clone(),
            thread: claimed.thread.clone(),
            prompt,
            history,
            tools: tools.clone(),
            memory,
        };

        match self.provider.complete(request).await {
            Ok(ProviderTurn::Chunks(chunks)) => {
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
            // The loop: read tools execute and fold into the history, write/spend tools park
            // the run on an approval card (REQ-016, REQ-017).
            Ok(ProviderTurn::ToolCalls(calls)) => {
                self.dispatch_tool_calls(context, claimed, &tools, &run, calls)
                    .await
            }
            Err(error) => {
                let retryable = error.is_retryable();
                tracing::warn!(run = %run.id, %error, "a run failed");
                self.fail_run(context, &run, error).await?;
                // A routine-woken run whose failure is retryable is tried again with backoff
                // before it burns the schedule's attempts; only then does it count toward the
                // routine's failure budget (REQ-024).
                if run.trigger == boma_domain::RunTrigger::Routine
                    && retryable
                    && usecases::auto_retry_routine_run(&self.app, &run).await?
                {
                    tracing::warn!(run = %run.id, "the routine wake failed retryable; re-queued with backoff");
                    return Ok(PassOutcome::Failed);
                }
                // Past the threshold the routine pauses itself, and the owner is told;
                // otherwise a per-run notice rides the thread when the routine asks for one.
                usecases::record_routine_failed(&self.app, &run).await?;
                Ok(PassOutcome::Failed)
            }
        }
    }

    /// Executes a tool-call turn (REQ-016, REQ-017).
    ///
    /// Read tools dispatch inline and their results join the history as `role=tool` messages.
    /// Write/spend tools never execute: the calls are parked, one approval card covers the park,
    /// and the run waits on a person. A tool the catalogue does not hold is refused and recorded,
    /// never executed.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] when a store cannot answer. The run is already failed or parked with
    /// a person, never left dangling.
    async fn dispatch_tool_calls(
        &self,
        context: &TenantContext,
        claimed: &usecases::ClaimedRun,
        tools: &[boma_domain::ToolSpec],
        run: &boma_domain::Run,
        calls: Vec<crate::ports::ToolCallRequest>,
    ) -> Result<PassOutcome, AppError> {
        if calls.is_empty() {
            let error = AppError::Rule(boma_domain::DomainError::InvalidName(
                "the model returned an empty tool call turn".to_owned(),
            ));
            tracing::warn!(run = %run.id, %error, "the model returned no tool calls");
            self.fail_run(context, run, error).await?;
            return Ok(PassOutcome::Failed);
        }
        let mut read_calls: Vec<crate::ports::ToolCallRequest> = Vec::new();
        let mut parked: Vec<crate::ports::ToolCallRequest> = Vec::new();
        let mut refused: Vec<crate::ports::ToolCallRequest> = Vec::new();
        for call in calls {
            match tools
                .iter()
                .find(|tool| tool.key == call.name)
                .map(|tool| tool.effect)
            {
                Some(boma_domain::ToolEffect::Read) => read_calls.push(call),
                Some(boma_domain::ToolEffect::Write | boma_domain::ToolEffect::Spend) => {
                    parked.push(call);
                }
                None => refused.push(call),
            }
        }
        for call in read_calls {
            let result =
                usecases::dispatch_tool(&self.app, context, claimed.bot.id.as_str(), &call).await?;
            usecases::append_tool_result(
                &self.app,
                context,
                &claimed.thread,
                &claimed.bot.id,
                &run.id,
                &usecases::ToolResultReport {
                    id: result.id,
                    name: call.name,
                    ok: result.ok,
                    body: result.body,
                },
            )
            .await?;
        }
        for call in refused {
            let body = serde_json::json!({ "error": "the bot may not use this tool" });
            usecases::append_tool_result(
                &self.app,
                context,
                &claimed.thread,
                &claimed.bot.id,
                &run.id,
                &usecases::ToolResultReport {
                    id: call.id,
                    name: call.name,
                    ok: false,
                    body,
                },
            )
            .await?;
        }
        if !parked.is_empty() {
            usecases::park_tool_calls(&self.app, context, claimed, parked).await?;
            return Ok(PassOutcome::Suspended);
        }
        Ok(PassOutcome::ToolsDispatched)
    }

    /// Records a helper run's outcome on the parent thread (REQ-019).
    ///
    /// Never propagates: a failure to write the tool message is logged, the helper's own state is
    /// already recorded, and the parent stays suspended visibly rather than silently "fine".
    async fn helper_finished(
        &self,
        context: &TenantContext,
        run: &boma_domain::Run,
        outcome: &str,
    ) {
        if !run.is_helper() {
            return;
        }
        if let Err(error) = usecases::complete_helper_run(&self.app, context, run, outcome).await {
            tracing::warn!(run = %run.id, %error, "the helper outcome could not be recorded on the parent");
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
