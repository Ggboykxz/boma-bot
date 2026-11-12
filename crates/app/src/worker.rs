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

        let history = self
            .app
            .stores
            .messages
            .page(&context, &run.thread_id, None, 50)
            .await
            .map(|page| page.messages)
            .unwrap_or_default();

        let request = ProviderRequest {
            bot: claimed.bot.clone(),
            thread: claimed.thread.clone(),
            prompt: prompt_of(&history),
            history,
        };

        match self.provider.complete(request).await {
            Ok(chunks) => {
                self.write_answer(&context, &claimed, &chunks).await?;
                usecases::set_run_status(
                    &self.app,
                    &context,
                    run.id.as_str(),
                    RunStatus::Completed,
                    Some(run.lease_fence),
                    None,
                )
                .await?;
            }
            Err(error) => {
                tracing::warn!(run = %run.id, %error, "a run failed");
                self.fail_run(&context, &run, error).await?;
            }
        }

        Ok(true)
    }

    /// Writes the answer into the thread, chunk by chunk.
    async fn write_answer(
        &self,
        context: &TenantContext,
        claimed: &usecases::ClaimedRun,
        chunks: &[String],
    ) -> Result<(), AppError> {
        let mut text = String::new();
        for chunk in chunks {
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

        usecases::write_reply(
            &self.app,
            context,
            &claimed.thread,
            &claimed.bot,
            &claimed.run.id,
            &text,
        )
        .await?;
        Ok(())
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

/// The last thing a person wrote, as the prompt for the provider.
fn prompt_of(history: &[boma_domain::Message]) -> String {
    history
        .iter()
        .rev()
        .find_map(|message| message.blocks.iter().find_map(boma_domain::Block::as_text))
        .unwrap_or_default()
        .to_owned()
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
            client_nonce: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn req_020_the_prompt_is_the_last_thing_the_person_wrote() {
        let history = vec![message("first"), message("second")];
        assert_eq!(prompt_of(&history), "second");
    }

    #[test]
    fn req_020_an_empty_history_yields_an_empty_prompt() {
        assert_eq!(prompt_of(&[]), "");
    }
}
