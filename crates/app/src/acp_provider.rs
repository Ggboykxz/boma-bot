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

use std::{path::PathBuf, time::Duration};

use async_trait::async_trait;

use crate::acp::AcpSession;

use crate::{
    AppError,
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
#[derive(Debug, Clone)]
pub struct AcpProvider {
    spec: ExecutorSpec,
    workspace_root: PathBuf,
    prompt_timeout: Duration,
}

impl AcpProvider {
    /// Builds a provider over `spec`. `workspace_root` defaults to a per-process directory under
    /// the system temp dir when the operator does not set `BOMA_ACP_WORKSPACES`.
    #[must_use]
    pub fn new(
        spec: ExecutorSpec,
        workspace_root: Option<PathBuf>,
        prompt_timeout: Duration,
    ) -> Self {
        let workspace_root = workspace_root.unwrap_or_else(|| {
            std::env::var_os("BOMA_ACP_WORKSPACES").map_or_else(
                || std::env::temp_dir().join(format!("boma-acp-{}", std::process::id())),
                PathBuf::from,
            )
        });
        Self {
            spec,
            workspace_root,
            prompt_timeout,
        }
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

#[async_trait]
impl ModelProvider for AcpProvider {
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderTurn, AppError> {
        let workspace = self.fresh_workspace(request.thread.id.as_str())?;
        let cwd = workspace
            .to_str()
            .map(str::to_owned)
            .ok_or(AppError::Unavailable)?;

        let prompt = render_prompt(&request);
        let mut session = AcpSession::spawn(
            self.spec.command,
            self.spec.args,
            &workspace,
            HANDSHAKE_TIMEOUT,
        )?;
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
}
