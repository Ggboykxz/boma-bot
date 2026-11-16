//! Local ACP CLI executors as a provider behind the same port every run sees (REQ-061).
//!
//! One subprocess per turn: spawn the known executor on a fresh per-run workspace, complete the
//! `initialize` handshake, open `session/new` there, send `session/prompt`, collect the
//! `agent_message_chunk` updates into one answer, `session/close`, shut down. The run lifecycle,
//! the approval gate and the event stream are untouched — this is one more `ModelProvider`.
//!
//! The executor's own sign-in (if any) lives in the CLI's login state on this machine; nothing
//! credential-shaped crosses this boundary. A missing or misbehaving executor surfaces as a
//! legible [`AppError::Unavailable`] before the run writes anything, which the worker records
//! as a failed run (REQ-061, second acceptance criterion).

use std::{path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;

use crate::acp::{AcpSession, PermissionDecider as AcpPermissionDecider};

use crate::{
    App, AppError,
    ports::{ModelProvider, ProviderRequest, ProviderTurn},
};

/// How long a handshake or session setup may take; a turn outlives it and the process is killed.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// What one known executor is; the registry validates the operator's choice.
#[derive(Debug, Clone, Copy)]
pub struct ExecutorSpec {
    /// The stable id the operator stores (e.g. `opencode`).
    pub id: &'static str,
    /// The command searched on the search path.
    pub command: &'static str,
    /// The arguments it is spawned with.
    pub args: &'static [&'static str],
}

/// The known executors this build can speak to. Adding one is a compiled change with a wire
/// test against a stub — never a string copied from the network.
#[must_use]
pub const fn executors() -> &'static [ExecutorSpec] {
    &[ExecutorSpec {
        id: "opencode",
        command: "opencode",
        args: &["acp"],
    }]
}

/// A local ACP executor, stateless per turn.
#[derive(Clone)]
pub struct AcpProvider {
    spec: ExecutorSpec,
    app: Arc<App>,
    context: boma_domain::TenantContext,
    workspace_root: PathBuf,
    prompt_timeout: Duration,
    permission_timeout: Duration,
}

/// The default deadline for a permission card to be answered before it is answered `cancelled`.
const DEFAULT_PERMISSION_TIMEOUT: Duration = Duration::from_hours(24);

impl AcpProvider {
    /// Builds a provider over `spec`. `workspace_root` defaults to a per-process directory under
    /// the system temp dir when the operator does not set `BOMA_ACP_WORKSPACES`.
    #[must_use]
    pub fn new(
        spec: ExecutorSpec,
        app: Arc<App>,
        context: boma_domain::TenantContext,
        workspace_root: Option<PathBuf>,
        prompt_timeout: Duration,
        permission_timeout: Duration,
    ) -> Self {
        let workspace_root = workspace_root.unwrap_or_else(|| {
            std::env::var_os("BOMA_ACP_WORKSPACES").map_or_else(
                || std::env::temp_dir().join(format!("boma-acp-{}", std::process::id())),
                PathBuf::from,
            )
        });
        Self {
            spec,
            app,
            context,
            workspace_root,
            prompt_timeout,
            permission_timeout,
        }
    }

    /// The permission timeout used when a test does not pin one.
    #[must_use]
    pub const fn default_permission_timeout() -> Duration {
        DEFAULT_PERMISSION_TIMEOUT
    }

    fn fresh_workspace(&self, thread_id: &str) -> Result<PathBuf, AppError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| AppError::Unavailable)?
            .as_nanos();
        let workspace = self.workspace_root.join(format!("run-{thread_id}-{nanos}"));
        std::fs::create_dir_all(&workspace).map_err(|_| AppError::Unavailable)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| AppError::Unavailable)?;
        }
        Ok(workspace)
    }
}

/// Waits for the person to answer the approval card parked for an ACP permission, then composes
/// the permission response the agent expects.
struct PermissionDecider {
    app: Arc<App>,
    context: boma_domain::TenantContext,
    run_id: boma_domain::RunId,
    bot: boma_domain::Bot,
    thread: boma_domain::Thread,
    timeout: Duration,
}

#[async_trait]
impl AcpPermissionDecider for PermissionDecider {
    async fn decide(&mut self, params: &serde_json::Value) -> serde_json::Value {
        use serde_json::json;
        let tool_call = params
            .get("toolCall")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let options = params.get("options").cloned().unwrap_or_else(|| json!([]));
        let call_id = tool_call
            .get("toolCallId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("acp-permission")
            .to_owned();
        let parked = crate::usecases::park_acp_permission(
            &self.app,
            &self.context,
            crate::usecases::AcpPark {
                run_id: &self.run_id,
                bot: &self.bot,
                thread: &self.thread,
            },
            &tool_call,
            &options,
            &call_id,
        )
        .await;
        let card_id = match parked {
            Ok(parked) => parked.card_id,
            Err(error) => {
                tracing::warn!(%error, "the ACP permission could not be parked; the call is refused");
                return json!({ "outcome": { "outcome": "cancelled" } });
            }
        };
        let Some(answer) = self.wait_for_answer(&card_id).await else {
            tracing::warn!("the ACP permission card timed out unanswered; the call is refused");
            return json!({ "outcome": { "outcome": "cancelled" } });
        };
        if answer == "approve"
            && let Some(option_id) = choose_option(&options)
        {
            return json!({ "outcome": { "outcome": "selected", "optionId": option_id } });
        }
        json!({ "outcome": { "outcome": "cancelled" } })
    }
}

impl PermissionDecider {
    async fn wait_for_answer(&self, card_id: &boma_domain::CardId) -> Option<String> {
        let started = std::time::Instant::now();
        loop {
            if started.elapsed() >= self.timeout {
                return None;
            }
            // A person may cancel the run while it waits on the card; the turn must end
            // rather than leak a blocked executor forever.
            match self.app.stores.runs.find(&self.context, &self.run_id).await {
                Ok(Some(run)) if run.status.is_terminal() => {
                    tracing::info!(run = %run.id, status = ?run.status, "the run went terminal while parked on the ACP permission; the turn is cancelled");
                    return None;
                }
                Ok(Some(_) | None) => {}
                Err(error) => {
                    tracing::warn!(%error, "the run could not be checked while parked; treating the wait as live");
                }
            }
            if let Ok(Some(card)) = self.app.stores.cards.find(&self.context, card_id).await
                && card.status != boma_domain::CardStatus::Pending
            {
                return card.answer;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

/// Chooses the option for an approved permission, preferring `allow_once` (least persistence),
/// then any allow-flavoured option, else the first one.
fn choose_option(options: &serde_json::Value) -> Option<String> {
    let options = options.as_array()?;
    let pick = options
        .iter()
        .find(|option| option.get("kind").and_then(serde_json::Value::as_str) == Some("allow_once"))
        .or_else(|| {
            options.iter().find(|option| {
                option
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|kind| kind.starts_with("allow"))
            })
        })
        .or_else(|| options.first())?;
    pick.get("optionId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// The turn context the executor sees: prior messages, memory, skills, then the prompt.
fn render_prompt(request: &ProviderRequest) -> String {
    use std::fmt::Write as _;
    let mut prompt = String::new();
    if !request.history.is_empty() || !request.memory.is_empty() || !request.skills.is_empty() {
        prompt.push_str("Continue the conversation below. Do not repeat what has already been said.\n\n<context>\n");
        for message in &request.history {
            let role = match message.role {
                boma_domain::MessageRole::User => "person",
                boma_domain::MessageRole::Bot => "assistant",
                boma_domain::MessageRole::Tool => "tool",
                boma_domain::MessageRole::System => "system",
            };
            let text = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    boma_domain::Block::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let _ = writeln!(prompt, "{role}: {text}");
        }
        for memory in &request.memory {
            let origin = match memory.scope {
                boma_domain::MemoryScope::Bot => "this bot",
                boma_domain::MemoryScope::Shared => "the space",
            };
            let _ = writeln!(
                prompt,
                "memory ({origin}: {}): {}",
                memory.path, memory.content
            );
        }
        for skill in &request.skills {
            let _ = writeln!(prompt, "skill: {} — {}", skill.name, skill.description);
        }
        prompt.push_str("</context>\n\n");
    }
    prompt.push_str(&request.prompt);
    prompt
}

impl AcpProvider {
    async fn turn_inner(
        &self,
        request: &ProviderRequest,
        workspace: &std::path::Path,
    ) -> Result<ProviderTurn, AppError> {
        let cwd = workspace
            .to_str()
            .map(str::to_owned)
            .ok_or(AppError::Unavailable)?;

        let prompt = render_prompt(request);
        let mut session = AcpSession::spawn(
            self.spec.command,
            self.spec.args,
            workspace,
            HANDSHAKE_TIMEOUT,
        )?;
        session.set_permission_decider(Box::new(PermissionDecider {
            app: Arc::clone(&self.app),
            context: self.context.clone(),
            run_id: request.run_id.clone(),
            bot: request.bot.clone(),
            thread: request.thread.clone(),
            timeout: self.permission_timeout,
        }));
        let _stderr = session.drain_stderr();
        session.initialize().await?;
        let session_id = session.session_new(&cwd).await?;
        let prompt_fut = session.session_prompt(&session_id, &prompt);
        let Ok(result) = tokio::time::timeout(self.prompt_timeout, prompt_fut).await else {
            session.session_close(&session_id).await;
            session.shutdown().await;
            return Err(AppError::Unavailable);
        };
        let (text, _stop_reason) = result?;
        session.session_close(&session_id).await;
        session.shutdown().await;

        if text.is_empty() {
            // An empty answer is a legible failure, never an empty assistant bubble.
            return Err(AppError::Unavailable);
        }
        Ok(ProviderTurn::Chunks(vec![text]))
    }

    fn cleanup_policy(workspace: &std::path::Path, outcome_ok: bool) {
        match std::env::var("BOMA_ACP_KEEP_WORKSPACES").as_deref() {
            Ok("all" | "failures") if !outcome_ok => {}
            Ok("none" | "failures") => {
                let _ = std::fs::remove_dir_all(workspace);
            }
            Ok("all") => {}
            _ => {
                // default: keep only failing workspaces for debugging
                if outcome_ok {
                    let _ = std::fs::remove_dir_all(workspace);
                }
            }
        }
    }
}

#[async_trait]
impl ModelProvider for AcpProvider {
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderTurn, AppError> {
        let workspace = self.fresh_workspace(request.thread.id.as_str())?;
        let result = self.turn_inner(&request, &workspace).await;
        Self::cleanup_policy(&workspace, result.is_ok());
        result
    }
}
