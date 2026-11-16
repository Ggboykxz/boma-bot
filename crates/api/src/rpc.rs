//! The RPC surface: `POST /rpc/{namespace}/{procedure}` (ADR-003).
//!
//! One route, one dispatcher. The namespace and the procedure come from the path, the body is
//! parsed by the procedure's own request type, and the reply is the procedure's own response
//! type. A procedure that does not exist is *not found*, a body that does not match the schema is
//! *bad request* — both in the stable error envelope (spec/03-api-contract.md §3.1).
//!
//! The procedure names are the reference's, so the golden fixtures replay unchanged.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::post,
};
use base64::Engine as _;
use boma_app::{AppError, usecases};
use boma_domain::SpaceRole;
use serde::Deserialize;

use crate::{
    auth::AuthContext,
    error::ApiError,
    error::ErrorCode,
    state::AppState,
    wire::{
        AccountDeleteReply, AccountDeleteRequest, AcpAgentConnectedReply,
        AcpAgentDisconnectedReply, AdminGrantCreateReply, AdminGrantCreateRequest,
        AdminGrantRevokeReply, AdminGrantRevokeRequest, AdminGrantView, AdminGrantsReply,
        AdminInvitationRevokeReply, AdminInvitationRevokeRequest, AdminInvitationView,
        AdminInvitationsReply, AdminMemberView, AdminMembersReply, AdminPluginDecideReply,
        AdminPluginDecideRequest, AdminPluginSubmissionView, AdminPluginsListRequest,
        AdminPluginsReply, AnswerReply, ArtifactCreateReply, ArtifactGetReply,
        ArtifactListSpaceReply, ArtifactRemoveReply, ArtifactVersionView, ArtifactVersionsReply,
        ArtifactView, AuditEventView, AuditExportReply, BillingCheckoutReply, BillingStatusReply,
        BoardsGetReply, BotReply, BotsListReply, BotsSignInsReply, CancelReply, CatalogModelView,
        CatalogProviderView, ComputerOpenScreenReply, ComputerProviderInfo, ComputerProviderReply,
        ComputerSettingsReply, ComputerStatusReply, ComputerTakeoverReply, ConnectionsInstallReply,
        ConnectionsListReply, ConnectionsRemoveReply, ConnectorView, ConversationView,
        ConversationsReply, ExportCountsView, ExportDownloadReply, ExportIdRequest,
        ExportStartReply, ExportStatusReply, ExternalAgentConnectedReply,
        ExternalAgentDisconnectedReply, ExternalAgentStatusReply, GroupBotView, GroupGetReply,
        GroupListView, GroupMessagesReply, GroupsCreateReply, GroupsCreateRequest, GroupsListReply,
        HeldDropReply, HeldListReply, HeldMailView, HeldReleaseReply, InstallGenericReply,
        InstallTelegramReply, InstallUrlReply, InstallationsReply, InvitationAcceptReply,
        InvitationAcceptRequest, InvitationReply, LocalDiscoveredModelView, MailPolicyReply,
        MailPolicyView, MarketplaceInstallReply, MarketplaceListReply, MarketplaceSubmitReply,
        McpOAuthStartReply, McpServerReply, McpServerView, McpServersListReply,
        McpServersRemoveReply, MemoryDocumentReply, MemoryDocumentsReply, MemoryProviderReply,
        MemoryRemoveReply, MemoryWriteReply, MessagesReply, MessagingInstallationView,
        MessagingRemoveReply, ModelConnectReply, ModelCredentialView, ModelCredentialsReply,
        ModelPreferenceReply, ModelsCatalogReply, ModelsDiscoveredReply,
        NotificationDestinationView, NotificationsDestinationsCreateReply,
        NotificationsDestinationsDeleteReply, NotificationsDestinationsListReply,
        NotificationsPolicyGetRequest, NotificationsPolicySetRequest, NotificationsPolicyView,
        NotificationsStatusReply, NotificationsSubscribeReply, NotificationsUnsubscribeReply,
        PagesGetReply, PagesRevokeShareReply, PagesSaveReply, PagesShareReply, PluginInstallView,
        ReactReply, RosterBotView, RoutineRemoveReply, RoutineRunNowReply, RoutineView,
        RoutineWriteReply, RoutinesReply, RunRetryReply, RunView, RunsReply, SecretForgetReply,
        SecretSetReply, SecretView, SecretsReply, SecurityEventsReply, SendReply,
        SignInRequestView, SkillReply, SkillView, SkillsReply, SnoozeReply, SpaceReply,
        SpendingLimitReply, TeachSkillReply, ThreadHeadReply, TicketCommentReply,
        TicketCommentView, TicketReply, TicketView,
    },
};

/// The namespaces this milestone serves.
pub mod namespaces {
    /// Conversations: sending, reading, subscribing.
    pub const THREADS: &str = "threads";
    /// Bots and their conversations.
    pub const BOTS: &str = "bots";
    /// Spaces and their members.
    pub const SPACES: &str = "spaces";
    /// Runs and their activity.
    pub const RUNS: &str = "runs";
    /// Memory documents (REQ-022).
    pub const MEMORY: &str = "memory";
    /// Routines: cron definitions that wake a bot (REQ-023).
    pub const ROUTINES: &str = "routines";
    /// The computer: a bot's sandbox (REQ-026..031).
    pub const COMPUTER: &str = "computer";
    /// Secrets for bots, write-only (REQ-046).
    pub const BOT_SECRETS: &str = "botSecrets";
    /// Skills: named instruction bundles a bot loads (REQ-035).
    pub const SKILLS: &str = "skills";
    /// MCP servers: the integration registry of the space (REQ-033).
    pub const MCP: &str = "mcp";
    /// Connectors: the builtin catalogue of integrations (REQ-032).
    pub const CONNECTIONS: &str = "connections";
    /// Marketplace: submissions and installs of plugins (REQ-034).
    pub const MARKETPLACE: &str = "marketplace";
    /// Artifacts: files with versions (REQ-036).
    pub const ARTIFACTS: &str = "artifacts";
    /// Pages: block documents, shared by link (REQ-037).
    pub const PAGES: &str = "pages";
    /// Boards: the ticket board (REQ-038). Absent when the deployment does not enable it.
    pub const BOARDS: &str = "boards";
    /// Tickets: the tickets of the board (REQ-038).
    pub const TICKETS: &str = "tickets";
    /// Mail: the bot mail rules and the held queue (REQ-047).
    pub const MAIL: &str = "mail";
    /// Messaging: the platform installations and their conversations (REQ-048).
    pub const MESSAGING: &str = "messaging";
    /// Models: the catalogue, BYOK credentials and the person's preference (REQ-052).
    pub const MODELS: &str = "models";
    /// Notifications: Web Push subscriptions and per-bot policy (REQ-064).
    pub const NOTIFICATIONS: &str = "notifications";
    /// Billing: the plan, the checkout, the spending limit (REQ-018, REQ-051). Absent when the
    /// deployment does not offer billing.
    pub const BILLING: &str = "billing";
    /// Audit: the export of the space's trail (REQ-045).
    pub const AUDIT: &str = "audit";
    /// Security: the recent security events of the space (REQ-045).
    pub const SECURITY: &str = "security";
    /// Export: the account archive (REQ-044).
    pub const EXPORT: &str = "export";
    /// Account: the account lifecycle (REQ-044).
    pub const ACCOUNT: &str = "account";
    /// Admin: members, invitations, server-level access grants and the plugin-submission review (REQ-058).
    pub const ADMIN: &str = "admin";
    /// Invitations: the token-gated acceptance path (REQ-042).
    pub const INVITATIONS: &str = "invitations";
    /// Groups: the shared rooms the bots work in (REQ-010).
    pub const GROUPS: &str = "groups";
}

/// Every procedure this deployment serves.
///
/// The list is the contract: a procedure that is not here is not reachable.
#[must_use]
#[allow(clippy::too_many_lines)] // the surface list mirrors the spec; the length is the point
pub fn procedures() -> Vec<(&'static str, &'static str)> {
    vec![
        (namespaces::THREADS, "send"),
        (namespaces::THREADS, "messages"),
        (namespaces::THREADS, "subscribe"),
        (namespaces::THREADS, "answer"),
        (namespaces::THREADS, "react"),
        (namespaces::THREADS, "head"),
        (namespaces::THREADS, "markRead"),
        (namespaces::THREADS, "markUnread"),
        (namespaces::THREADS, "snooze"),
        (namespaces::THREADS, "unsnooze"),
        (namespaces::BOTS, "create"),
        (namespaces::BOTS, "get"),
        (namespaces::BOTS, "list"),
        (namespaces::BOTS, "connectExternal"),
        (namespaces::BOTS, "disconnectExternal"),
        (namespaces::BOTS, "connectAcp"),
        (namespaces::BOTS, "disconnectAcp"),
        (namespaces::BOTS, "signIns"),
        (namespaces::BOTS, "externalAgentStatus"),
        (namespaces::NOTIFICATIONS, "status"),
        (namespaces::NOTIFICATIONS, "subscribe"),
        (namespaces::NOTIFICATIONS, "unsubscribe"),
        (namespaces::NOTIFICATIONS, "policyGet"),
        (namespaces::NOTIFICATIONS, "policySet"),
        (namespaces::NOTIFICATIONS, "destinationsList"),
        (namespaces::NOTIFICATIONS, "destinationsCreate"),
        (namespaces::NOTIFICATIONS, "destinationsDelete"),
        (namespaces::SPACES, "create"),
        (namespaces::SPACES, "invite"),
        (namespaces::RUNS, "list"),
        (namespaces::RUNS, "cancel"),
        (namespaces::RUNS, "retry"),
        (namespaces::RUNS, "pause"),
        (namespaces::RUNS, "resume"),
        (namespaces::MEMORY, "write"),
        (namespaces::MEMORY, "read"),
        (namespaces::MEMORY, "list"),
        (namespaces::MEMORY, "remove"),
        (namespaces::MEMORY, "provider"),
        (namespaces::ROUTINES, "create"),
        (namespaces::ROUTINES, "list"),
        (namespaces::ROUTINES, "remove"),
        (namespaces::ROUTINES, "runNow"),
        (namespaces::COMPUTER, "status"),
        (namespaces::COMPUTER, "start"),
        (namespaces::COMPUTER, "stop"),
        (namespaces::COMPUTER, "openScreen"),
        (namespaces::COMPUTER, "takeover"),
        (namespaces::COMPUTER, "release"),
        (namespaces::COMPUTER, "settings"),
        (namespaces::COMPUTER, "provider"),
        (namespaces::BOT_SECRETS, "set"),
        (namespaces::BOT_SECRETS, "list"),
        (namespaces::BOT_SECRETS, "forget"),
        (namespaces::SKILLS, "list"),
        (namespaces::SKILLS, "get"),
        (namespaces::SKILLS, "teach"),
        (namespaces::MCP, "servers.list"),
        (namespaces::MCP, "servers.add"),
        (namespaces::MCP, "servers.remove"),
        (namespaces::MCP, "servers.probe"),
        (namespaces::MCP, "oauth.start"),
        (namespaces::MCP, "oauth.complete"),
        (namespaces::CONNECTIONS, "list"),
        (namespaces::CONNECTIONS, "install"),
        (namespaces::CONNECTIONS, "remove"),
        (namespaces::MARKETPLACE, "list"),
        (namespaces::MARKETPLACE, "submit"),
        (namespaces::MARKETPLACE, "install"),
        (namespaces::ARTIFACTS, "create"),
        (namespaces::ARTIFACTS, "get"),
        (namespaces::ARTIFACTS, "getById"),
        (namespaces::ARTIFACTS, "listVersions"),
        (namespaces::ARTIFACTS, "listSpace"),
        (namespaces::ARTIFACTS, "remove"),
        (namespaces::PAGES, "save"),
        (namespaces::PAGES, "get"),
        (namespaces::PAGES, "share"),
        (namespaces::PAGES, "revokeShare"),
        (namespaces::BOARDS, "get"),
        (namespaces::TICKETS, "create"),
        (namespaces::TICKETS, "move"),
        (namespaces::TICKETS, "comment"),
        (namespaces::MAIL, "policyGet"),
        (namespaces::MAIL, "policySet"),
        (namespaces::MAIL, "heldList"),
        (namespaces::MAIL, "heldRelease"),
        (namespaces::MAIL, "heldDrop"),
        (namespaces::MESSAGING, "installUrl"),
        (namespaces::MESSAGING, "installTelegram"),
        (namespaces::MESSAGING, "installGeneric"),
        (namespaces::MESSAGING, "list"),
        (namespaces::MESSAGING, "remove"),
        (namespaces::MESSAGING, "conversations"),
        (namespaces::MODELS, "catalog"),
        (namespaces::MODELS, "discovered"),
        (namespaces::MODELS, "credentials"),
        (namespaces::MODELS, "connect"),
        (namespaces::MODELS, "disconnect"),
        (namespaces::MODELS, "preferences.get"),
        (namespaces::MODELS, "preferences.set"),
        (namespaces::BILLING, "status"),
        (namespaces::BILLING, "checkout"),
        (namespaces::BILLING, "limit.get"),
        (namespaces::BILLING, "limit.set"),
        (namespaces::AUDIT, "export"),
        (namespaces::SECURITY, "events.list"),
        (namespaces::EXPORT, "start"),
        (namespaces::EXPORT, "status"),
        (namespaces::EXPORT, "download"),
        (namespaces::ACCOUNT, "delete"),
        (namespaces::ADMIN, "members.list"),
        (namespaces::ADMIN, "invitations.list"),
        (namespaces::ADMIN, "invitations.revoke"),
        (namespaces::ADMIN, "grants.list"),
        (namespaces::ADMIN, "grants.create"),
        (namespaces::ADMIN, "grants.revoke"),
        (namespaces::ADMIN, "plugins.list"),
        (namespaces::ADMIN, "plugins.decide"),
        (namespaces::INVITATIONS, "accept"),
        (namespaces::GROUPS, "create"),
        (namespaces::GROUPS, "get"),
        (namespaces::GROUPS, "list"),
        (namespaces::GROUPS, "messages"),
    ]
}

/// Builds the RPC routes.
pub fn router() -> Router<AppState> {
    Router::new()
        // The only procedure with a larger body cap: artifacts carry bytes (defect D-07).
        .route(
            "/rpc/artifacts/create",
            post(artifacts_create_upload).route_layer(axum::extract::DefaultBodyLimit::max(
                boma_domain::ARTIFACT_MAX_BYTES * 2 + 1024,
            )),
        )
        .route("/rpc/{namespace}/{procedure}", post(dispatch))
}

/// The artifact-creation upload, byte cap of its own (defect D-07).
async fn artifacts_create_upload(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    rpc_inner(
        state,
        "artifacts".to_owned(),
        "create".to_owned(),
        &headers,
        body,
    )
    .await
}

/// Dispatches one procedure call.
///
/// The caller must be signed in: every procedure in the spec acts for a member of a space
/// (REQ-041, REQ-043).
#[allow(clippy::too_many_lines)] // the match is the procedure table; splitting hides the contract
async fn dispatch(
    State(state): State<AppState>,
    Path((namespace, procedure)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    rpc_inner(state, namespace, procedure, &headers, body).await
}

/// Everything after the routes: auth, procedure table, error envelope.
#[allow(clippy::too_many_lines)] // the match is the procedure table; splitting hides the contract
async fn rpc_inner(
    state: AppState,
    namespace: String,
    procedure: String,
    headers: &axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match state.authenticate(headers).await {
        Ok(auth) => auth,
        Err(error) => return error.into_response(),
    };

    // The board surface is absent unless the deployment enabled it (REQ-038): 404, not 403,
    // indistinguishable from a procedure that does not exist.
    if matches!(namespace.as_str(), namespaces::BOARDS | namespaces::TICKETS)
        && !state.app().services.board_enabled
    {
        return ApiError::not_found().into_response();
    }
    // The messaging surface is absent unless the deployment enabled it (REQ-048): 404, not 403.
    if namespace.as_str() == namespaces::MESSAGING && !state.app().services.messaging_enabled {
        return ApiError::not_found().into_response();
    }
    // So is the billing surface (REQ-051): a deployment without a payment provider sells nothing.
    if namespace.as_str() == namespaces::BILLING && !state.app().services.billing_enabled {
        return ApiError::not_found().into_response();
    }

    let outcome: Result<Response, ApiError> = match (namespace.as_str(), procedure.as_str()) {
        (namespaces::THREADS, "send") => send(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "messages") => messages(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "subscribe") => crate::realtime::subscribe(state, auth, body).await,
        (namespaces::THREADS, "answer") => threads_answer(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "react") => threads_react(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "head") => threads_head(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "markRead") => threads_mark_read(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "markUnread") => threads_mark_unread(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "snooze") => threads_snooze(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "unsnooze") => threads_unsnooze(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "create") => bots_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "get") => bots_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "list") => bots_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "connectExternal") => bots_connect_external(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "disconnectExternal") => bots_disconnect_external(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "connectAcp") => bots_connect_acp(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "disconnectAcp") => bots_disconnect_acp(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "signIns") => bots_sign_ins(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "externalAgentStatus") => bots_external_agent_status(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::NOTIFICATIONS, "status") => notifications_status(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::NOTIFICATIONS, "subscribe") => notifications_subscribe(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::NOTIFICATIONS, "unsubscribe") => notifications_unsubscribe(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::NOTIFICATIONS, "policyGet") => notifications_policy_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::NOTIFICATIONS, "policySet") => notifications_policy_set(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::NOTIFICATIONS, "destinationsList") => {
            notifications_destinations_list(state, auth, body)
                .await
                .map(|reply| Json(reply).into_response())
        }
        (namespaces::NOTIFICATIONS, "destinationsCreate") => {
            notifications_destinations_create(state, auth, body)
                .await
                .map(|reply| Json(reply).into_response())
        }
        (namespaces::NOTIFICATIONS, "destinationsDelete") => {
            notifications_destinations_delete(state, auth, body)
                .await
                .map(|reply| Json(reply).into_response())
        }
        (namespaces::SPACES, "create") => spaces_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SPACES, "invite") => spaces_invite(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::RUNS, "list") => runs_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::RUNS, "cancel") => runs_cancel(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::RUNS, "retry") => runs_retry(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::RUNS, "pause") => runs_pause(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::RUNS, "resume") => runs_resume(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "write") => memory_write(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "read") => memory_read(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "list") => memory_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "remove") => memory_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "provider") => Ok(Json(MemoryProviderReply {
            provider: "builtin".to_owned(),
        })
        .into_response()),
        (namespaces::ROUTINES, "create") => routines_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ROUTINES, "list") => routines_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ROUTINES, "remove") => routines_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ROUTINES, "runNow") => routines_run_now(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "status") => computer_status(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "start") => computer_start(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "stop") => computer_stop(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "openScreen") => computer_open_screen(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "takeover") => computer_takeover(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "release") => computer_release(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "settings") => computer_settings(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "provider") => {
            Ok::<_, ApiError>(Json(computer_provider(state.app())).into_response())
        }
        (namespaces::BOT_SECRETS, "set") => secrets_set(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOT_SECRETS, "list") => secrets_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOT_SECRETS, "forget") => secrets_forget(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SKILLS, "list") => skills_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SKILLS, "get") => skills_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SKILLS, "teach") => skills_teach(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "servers.list") => mcp_servers_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "servers.add") => mcp_servers_add(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "servers.remove") => mcp_servers_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "servers.probe") => mcp_servers_probe(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "oauth.start") => mcp_oauth_start(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "oauth.complete") => mcp_oauth_complete(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::CONNECTIONS, "list") => connections_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::CONNECTIONS, "install") => connections_install(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::CONNECTIONS, "remove") => connections_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MARKETPLACE, "list") => marketplace_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MARKETPLACE, "submit") => marketplace_submit(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MARKETPLACE, "install") => marketplace_install(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ARTIFACTS, "create") => artifacts_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ARTIFACTS, "get") => artifacts_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ARTIFACTS, "getById") => artifacts_get_by_id(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ARTIFACTS, "listVersions") => artifacts_list_versions(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ARTIFACTS, "listSpace") => artifacts_list_space(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ARTIFACTS, "remove") => artifacts_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::PAGES, "save") => pages_save(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::PAGES, "get") => pages_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::PAGES, "share") => pages_share(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::PAGES, "revokeShare") => pages_revoke_share(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOARDS, "get") => boards_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::TICKETS, "create") => tickets_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::TICKETS, "move") => tickets_move(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::TICKETS, "comment") => tickets_comment(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MAIL, "policyGet") => mail_policy_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MAIL, "policySet") => mail_policy_set(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MAIL, "heldList") => mail_held_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MAIL, "heldRelease") => mail_held_release(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MAIL, "heldDrop") => mail_held_drop(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MESSAGING, "installUrl") => messaging_install_url(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MESSAGING, "installTelegram") => messaging_install_telegram(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MESSAGING, "installGeneric") => messaging_install_generic(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MESSAGING, "list") => messaging_list(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MESSAGING, "remove") => messaging_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MESSAGING, "conversations") => messaging_conversations(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MODELS, "catalog") => models_catalog(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MODELS, "discovered") => models_discovered(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MODELS, "credentials") => models_credentials(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MODELS, "connect") => models_connect(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MODELS, "disconnect") => models_disconnect(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MODELS, "preferences.get") => models_preferences_get(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MODELS, "preferences.set") => models_preferences_set(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BILLING, "status") => billing_status(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BILLING, "checkout") => billing_checkout(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BILLING, "limit.get") => billing_limit_get(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BILLING, "limit.set") => billing_limit_set(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::AUDIT, "export") => audit_export(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SECURITY, "events.list") => security_events_list(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::EXPORT, "start") => export_start(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::EXPORT, "status") => export_status(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::EXPORT, "download") => export_download(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ACCOUNT, "delete") => account_delete(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ADMIN, "members.list") => admin_members_list(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ADMIN, "invitations.list") => admin_invitations_list(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ADMIN, "invitations.revoke") => admin_invitations_revoke(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ADMIN, "grants.list") => admin_grants_list(state, auth)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ADMIN, "grants.create") => admin_grants_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ADMIN, "grants.revoke") => admin_grants_revoke(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ADMIN, "plugins.list") => admin_plugins_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ADMIN, "plugins.decide") => admin_plugins_decide(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::INVITATIONS, "accept") => invitations_accept(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::GROUPS, "create") => groups_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::GROUPS, "get") => groups_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::GROUPS, "list") => groups_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::GROUPS, "messages") => groups_messages(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        _ => Err(ApiError::not_found()),
    };

    match outcome {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

/// `threads.send` — write a message and start the run that answers it.
async fn send(state: AppState, auth: AuthContext, body: Bytes) -> Result<SendReply, ApiError> {
    let request: SendRequest = parse(&body)?;
    let outcome = usecases::send_message(
        state.app(),
        &auth.actor.context,
        &usecases::SendMessage {
            bot_id: request.bot_id,
            text: request.text,
            client_nonce: request.client_nonce,
            reply_to_message_id: request.reply_to_message_id,
            reply_quote: request.reply_quote,
        },
    )
    .await?;

    let first = outcome.runs.first().ok_or(ApiError::unavailable())?;
    Ok(SendReply {
        message_id: outcome.message.id.as_str().to_owned(),
        run_id: first.id.as_str().to_owned(),
        status: first.status.as_str().to_owned(),
        run_ids: if outcome.runs.len() > 1 {
            Some(
                outcome
                    .runs
                    .iter()
                    .map(|run| run.id.as_str().to_owned())
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        },
    })
}

/// The body of `threads.send`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SendRequest {
    /// The bot that receives the message.
    bot_id: String,
    /// What the person wrote.
    text: String,
    /// The nonce that makes the send idempotent.
    #[serde(default)]
    client_nonce: Option<String>,
    /// The message this one answers, when it is a reply.
    #[serde(default)]
    reply_to_message_id: Option<String>,
    /// A short quote of the message being answered.
    #[serde(default)]
    reply_quote: Option<String>,
}

/// `threads.messages` — a page of the thread's history, newest first.
async fn messages(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MessagesReply, ApiError> {
    let request: MessagesRequest = parse(&body)?;
    let page = usecases::page_messages(
        state.app(),
        &auth.actor.context,
        &request.thread_id,
        request.cursor.as_deref(),
        request.limit,
    )
    .await?;

    Ok(MessagesReply {
        messages: page
            .messages
            .iter()
            .map(crate::wire::MessageView::from)
            .collect(),
        next_cursor: page.next_cursor,
    })
}

/// The body of `threads.messages`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessagesRequest {
    /// The thread to read.
    thread_id: String,
    /// The cursor of the older page.
    #[serde(default)]
    cursor: Option<String>,
    /// How many messages the caller wants.
    #[serde(default)]
    limit: Option<usize>,
}

/// `bots.create` — create a bot and its thread in one transaction.
async fn bots_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<BotReply, ApiError> {
    let request: BotCreateRequest = parse(&body)?;
    let created = usecases::create_bot(
        state.app(),
        &auth.actor.context,
        &request.name,
        request.description,
        request.color,
    )
    .await?;

    Ok(BotReply {
        id: created.bot.id.as_str().to_owned(),
        name: created.bot.name,
        thread_id: created.thread.id.as_str().to_owned(),
        archived_at: created.bot.archived_at,
        created_at: created.bot.created_at,
    })
}

/// The body of `bots.create`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotCreateRequest {
    /// The name a person sees.
    name: String,
    /// What the bot is for.
    #[serde(default)]
    description: Option<String>,
    /// The colour a person sees.
    #[serde(default)]
    color: Option<String>,
}

/// `bots.get` — read one bot.
async fn bots_get(state: AppState, auth: AuthContext, body: Bytes) -> Result<BotReply, ApiError> {
    let request: BotGetRequest = parse(&body)?;
    let bot = usecases::find_bot(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(BotReply {
        id: bot.id.as_str().to_owned(),
        name: bot.name,
        thread_id: bot.thread_id.as_str().to_owned(),
        archived_at: bot.archived_at,
        created_at: bot.created_at,
    })
}

/// The body of `bots.get`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotGetRequest {
    /// The bot to read.
    bot_id: String,
}

/// `spaces.create` — create a space and join it as its owner.
async fn spaces_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SpaceReply, ApiError> {
    let request: SpaceCreateRequest = parse(&body)?;
    let created = usecases::create_space(state.app(), &auth.actor.context, &request.name).await?;
    Ok(SpaceReply {
        id: created.space.id.as_str().to_owned(),
        name: created.space.name,
        is_default: created.space.is_default,
        role: created.role.as_str().to_owned(),
    })
}

/// The body of `spaces.create`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceCreateRequest {
    /// The name a person sees.
    name: String,
}

/// `spaces.invite` — invite somebody into the caller's space.
async fn spaces_invite(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<InvitationReply, ApiError> {
    let request: SpaceInviteRequest = parse(&body)?;
    let invitation = usecases::invite(
        state.app(),
        &auth.actor.context,
        &request.email,
        request.role,
        state.invitation_ttl_seconds(),
    )
    .await?;
    Ok(InvitationReply {
        invitation_id: invitation.id.as_str().to_owned(),
        expires_at: invitation.expires_at,
        status: "pending".to_owned(),
    })
}

/// The body of `spaces.invite`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceInviteRequest {
    /// Who is invited.
    email: String,
    /// The role the invitee will have.
    #[serde(default = "default_role")]
    role: SpaceRole,
}

/// The role an invitation grants when the caller does not say.
fn default_role() -> SpaceRole {
    SpaceRole::Member
}

/// `threads.answer` — answer a card and resume the run it blocked (REQ-017).
async fn threads_answer(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AnswerReply, ApiError> {
    let request: ThreadAnswerRequest = parse(&body)?;
    let outcome = usecases::answer_card(
        state.app(),
        &auth.actor.context,
        &request.card_id,
        &request.answer,
    )
    .await?;
    Ok(AnswerReply {
        ok: true,
        run_id: outcome.card.run_id.map(|run| run.as_str().to_owned()),
    })
}

/// The body of `threads.answer`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadAnswerRequest {
    /// The card to answer.
    card_id: String,
    /// The answer a member gives.
    answer: String,
}

/// The body of `threads.head`, `threads.markRead` and `threads.markUnread` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadHeadRequest {
    /// The thread whose head the caller wants.
    thread_id: String,
}

/// `threads.head` — the caller's view of the thread's tail and own read position (REQ-009).
async fn threads_head(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ThreadHeadReply, ApiError> {
    let request: ThreadHeadRequest = parse(&body)?;
    let head = usecases::thread_head(state.app(), &auth.actor.context, &request.thread_id).await?;
    Ok(ThreadHeadReply::from(&head))
}

/// `threads.markRead` — move the caller's read position to the newest message (REQ-009).
async fn threads_mark_read(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ThreadHeadReply, ApiError> {
    let request: ThreadHeadRequest = parse(&body)?;
    let head = usecases::mark_read(state.app(), &auth.actor.context, &request.thread_id).await?;
    Ok(ThreadHeadReply::from(&head))
}

/// `threads.markUnread` — move the caller's read position back to zero (REQ-009).
async fn threads_mark_unread(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ThreadHeadReply, ApiError> {
    let request: ThreadHeadRequest = parse(&body)?;
    let head = usecases::mark_unread(state.app(), &auth.actor.context, &request.thread_id).await?;
    Ok(ThreadHeadReply::from(&head))
}

/// The body of `bots.list` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotsListRequest {}

/// `bots.list` — the roster the rail renders from, with per-member unread and snooze (REQ-009).
async fn bots_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<BotsListReply, ApiError> {
    let _: BotsListRequest = parse(&body)?;
    let bots = usecases::list_bots(state.app(), &auth.actor.context).await?;
    Ok(BotsListReply {
        bots: bots.iter().map(RosterBotView::from).collect(),
    })
}

/// The body of `threads.snooze` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadSnoozeRequest {
    /// The thread to snooze.
    thread_id: String,
    /// Until when; absent leaves the chat snoozed until it is explicitly unsnoozed.
    #[serde(default)]
    until: Option<String>,
}

/// The body of `threads.unsnooze` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadUnsnoozeRequest {
    /// The thread to unsnooze.
    thread_id: String,
}

/// `threads.snooze` — hide the chat from the regular list until `until` (REQ-009).
async fn threads_snooze(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SnoozeReply, ApiError> {
    let request: ThreadSnoozeRequest = parse(&body)?;
    let outcome = usecases::snooze_thread(
        state.app(),
        &auth.actor.context,
        &request.thread_id,
        request.until.as_deref(),
    )
    .await?;
    Ok(SnoozeReply::from(&outcome))
}

/// `threads.unsnooze` — bring the snoozed chat back to the regular list (REQ-009).
async fn threads_unsnooze(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SnoozeReply, ApiError> {
    let request: ThreadUnsnoozeRequest = parse(&body)?;
    let outcome =
        usecases::snooze_thread(state.app(), &auth.actor.context, &request.thread_id, None).await?;
    Ok(SnoozeReply::from(&outcome))
}

/// The body of `memory.write` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MemoryWriteRequest {
    /// The owner bot, for scope `bot`.
    #[serde(default)]
    bot_id: Option<String>,
    /// `bot` or `shared`.
    scope: String,
    /// The path of the document.
    path: String,
    /// The content to write.
    content: String,
}

/// The body of `memory.read` and `memory.remove` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MemoryReadRequest {
    /// The owner bot, for scope `bot`.
    #[serde(default)]
    bot_id: Option<String>,
    /// `bot` or `shared`.
    scope: String,
    /// The path of the document.
    path: String,
}

/// The body of `memory.list` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MemoryListRequest {
    /// The owner bot, for scope `bot`.
    #[serde(default)]
    bot_id: Option<String>,
    /// `bot` or `shared`.
    scope: String,
}

/// `memory.write` — append a revision and return the projection (REQ-022).
async fn memory_write(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MemoryWriteReply, ApiError> {
    let request: MemoryWriteRequest = parse(&body)?;
    let outcome = usecases::memory_write(
        state.app(),
        &auth.actor.context,
        &request.scope,
        request.bot_id.as_deref(),
        &request.path,
        &request.content,
    )
    .await?;
    Ok(MemoryWriteReply::from(&outcome.document))
}

/// `memory.read` — read one document (REQ-022).
async fn memory_read(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MemoryDocumentReply, ApiError> {
    let request: MemoryReadRequest = parse(&body)?;
    let document = usecases::memory_read(
        state.app(),
        &auth.actor.context,
        &request.scope,
        request.bot_id.as_deref(),
        &request.path,
    )
    .await?;
    Ok(MemoryDocumentReply::from(&document))
}

/// `memory.list` — list the documents of a scope (REQ-022).
async fn memory_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MemoryDocumentsReply, ApiError> {
    let request: MemoryListRequest = parse(&body)?;
    let documents = usecases::memory_list(
        state.app(),
        &auth.actor.context,
        &request.scope,
        request.bot_id.as_deref(),
    )
    .await?;
    Ok(MemoryDocumentsReply {
        documents: documents.iter().map(MemoryDocumentReply::from).collect(),
    })
}

/// `memory.remove` — remove a document and its revision chain (REQ-022).
async fn memory_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MemoryRemoveReply, ApiError> {
    let request: MemoryReadRequest = parse(&body)?;
    let outcome = usecases::memory_remove(
        state.app(),
        &auth.actor.context,
        &request.scope,
        request.bot_id.as_deref(),
        &request.path,
    )
    .await?;
    Ok(MemoryRemoveReply {
        path: outcome.path,
        removed: outcome.removed,
    })
}

/// The default every routine gets: awake.
fn default_active() -> bool {
    true
}

/// The body of `routines.create` (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoutinesCreateRequest {
    /// The bot the routine wakes.
    bot_id: String,
    /// The name a person sees.
    name: String,
    /// The prompt the agent receives when the routine fires.
    prompt: String,
    /// The cron expressions, or `@once`.
    crons: Vec<String>,
    /// The IANA timezone the cron computes in.
    timezone: String,
    /// Whether the routine wakes; a routine a member created is awake by default.
    #[serde(default = "default_active")]
    active: bool,
    /// Whether the owner is told about each run.
    #[serde(default)]
    notify: bool,
    /// Whether repository events from the bot's webhook wake this routine.
    #[serde(default)]
    github_enabled: bool,
    /// The exact addresses admitted to wake this routine by mail.
    #[serde(default)]
    email_senders: Vec<String>,
}

/// The body of `routines.list` (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoutinesListRequest {
    /// The bot whose routines the caller lists.
    bot_id: String,
}

/// The body of `routines.remove` (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoutinesRemoveRequest {
    /// The routine to remove.
    routine_id: String,
}

/// `routines.create` — create a routine and compute its next wake (REQ-023).
async fn routines_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RoutineWriteReply, ApiError> {
    let request: RoutinesCreateRequest = parse(&body)?;
    let outcome = usecases::routines_create(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.name,
        &request.prompt,
        &request.crons,
        &request.timezone,
        request.active,
        request.notify,
        request.github_enabled,
        request.email_senders,
    )
    .await?;
    Ok(RoutineWriteReply::from(&outcome.routine))
}

/// `routines.list` — list a bot's routines (REQ-023).
async fn routines_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RoutinesReply, ApiError> {
    let request: RoutinesListRequest = parse(&body)?;
    let routines =
        usecases::routines_list(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(RoutinesReply {
        routines: routines.iter().map(RoutineView::from).collect(),
    })
}

/// `routines.remove` — remove a routine (REQ-023).
async fn routines_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RoutineRemoveReply, ApiError> {
    let request: RoutinesRemoveRequest = parse(&body)?;
    let outcome =
        usecases::routines_remove(state.app(), &auth.actor.context, &request.routine_id).await?;
    Ok(RoutineRemoveReply {
        routine_id: outcome.routine_id.as_str().to_owned(),
        removed: outcome.removed,
    })
}

/// The body shared by the bot-scoped computer procedures.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComputerRequest {
    /// The bot whose computer is asked about.
    bot_id: String,
}

/// `computer.takeover` — take exclusive control of the bot's screen (REQ-029).
///
/// While a member holds the screen, computer-driven input refuses to act. The hold is
/// exclusive: only the holder may release it. Idempotent for the holder.
async fn computer_takeover(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerTakeoverReply, ApiError> {
    let request: ComputerRequest = parse(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let computer = usecases::computer_takeover(app, &auth.actor.context, &request.bot_id).await?;
    Ok(ComputerTakeoverReply::for_computer(&computer))
}

/// `computer.release` — hand the screen back (REQ-029).
///
/// Only the holder may release.
async fn computer_release(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerTakeoverReply, ApiError> {
    let request: ComputerRequest = parse(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let computer = usecases::computer_release(app, &auth.actor.context, &request.bot_id).await?;
    Ok(ComputerTakeoverReply::for_computer(&computer))
}

/// The body of `computer.settings` (REQ-026).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComputerSettingsRequest {
    /// The bot whose computer to configure.
    bot_id: String,
    /// The RAM cap, in megabytes, when the computer may use more.
    #[serde(default)]
    memory_limit_mb: Option<u64>,
    /// The CPU cap, in CPUs.
    #[serde(default)]
    cpu_limit: Option<f64>,
    /// Minutes of idleness before release.
    #[serde(default)]
    idle_minutes: Option<u64>,
}

/// `computer.settings` — read or adjust the computer's configuration (REQ-026).
///
/// With no fields beyond `botId` the call is a read. Fields present are applied, within the
/// deployment limits; anything outside is refused.
async fn computer_settings(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerSettingsReply, ApiError> {
    let request: ComputerSettingsRequest = parse(&body)?;
    let app = state.app();
    let context = &auth.actor.context;
    let bot_id =
        boma_domain::BotId::new(request.bot_id.clone()).map_err(|_| ApiError::not_found())?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(ApiError::not_found)?;
    let is_update = request.memory_limit_mb.is_some()
        || request.cpu_limit.is_some()
        || request.idle_minutes.is_some();
    if !is_update {
        let computer = usecases::computer_status(app, context, &request.bot_id).await?;
        return Ok(match computer {
            Some(computer) => ComputerSettingsReply {
                memory_limit_mb: computer.memory_limit_mb,
                cpu_limit: computer.cpu_limit,
                idle_minutes: computer.idle_minutes,
                mode: if computer.private {
                    "private".to_owned()
                } else {
                    "team".to_owned()
                },
            },
            None => ComputerSettingsReply {
                memory_limit_mb: 512,
                cpu_limit: 1.0,
                idle_minutes: 10,
                mode: "private".to_owned(),
            },
        });
    }
    let runtime = app
        .services
        .computer
        .as_deref()
        .filter(|runtime| runtime.provider() != "none")
        .ok_or_else(ApiError::computers_unavailable)?;
    let _ = runtime; // settings validate against the deployment limits regardless of provider
    let mut computer = usecases::computer_status(app, context, &request.bot_id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if let Some(mb) = request.memory_limit_mb {
        boma_domain::validate_memory_limit(mb).map_err(|_| ApiError::bad_request())?;
        computer.memory_limit_mb = mb;
    }
    if let Some(cpu) = request.cpu_limit {
        boma_domain::validate_cpu_limit(cpu).map_err(|_| ApiError::bad_request())?;
        computer.cpu_limit = cpu;
    }
    if let Some(minutes) = request.idle_minutes {
        boma_domain::validate_idle_minutes(minutes).map_err(|_| ApiError::bad_request())?;
        computer.idle_minutes = minutes;
    }
    computer.updated_at = app.services.clock.now();
    app.stores
        .computers
        .update(context, &computer)
        .await
        .map_err(ApiError::from)?;
    Ok(ComputerSettingsReply {
        memory_limit_mb: computer.memory_limit_mb,
        cpu_limit: computer.cpu_limit,
        idle_minutes: computer.idle_minutes,
        mode: if computer.private {
            "private".to_owned()
        } else {
            "team".to_owned()
        },
    })
}

/// `computer.provider` — which providers this deployment can run (REQ-027).
///
/// `frame` is true only when a frame command is configured for the image; `screenshot` is the
/// same capability under its wire name. `shell` is true for every non-`none` provider.
fn computer_provider(app: &boma_app::App) -> ComputerProviderReply {
    let active = app.services.computer.as_deref().map_or_else(
        || "none".to_owned(),
        |runtime| runtime.provider().to_owned(),
    );
    let (frame, screenshot) = app
        .services
        .computer
        .as_deref()
        .map_or((false, false), |runtime| {
            (
                runtime.frame_command().is_some(),
                runtime.frame_command().is_some(),
            )
        });
    let shell = app
        .services
        .computer
        .as_deref()
        .is_some_and(|runtime| runtime.provider() != "none");
    ComputerProviderReply {
        active,
        providers: vec![
            ComputerProviderInfo {
                name: "none".to_owned(),
                available: true,
                frame: false,
                shell: false,
                screenshot: false,
            },
            ComputerProviderInfo {
                name: "docker".to_owned(),
                available: app
                    .services
                    .computer
                    .as_deref()
                    .is_some_and(|runtime| runtime.provider() == "docker"),
                frame,
                shell,
                screenshot,
            },
        ],
    }
}

/// The body of `computer.status` (REQ-027).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComputerStatusRequest {
    /// The bot whose computer is asked about.
    bot_id: String,
    /// `private` (default) or `team` (only meaningful for `computer.start`).
    #[serde(default)]
    mode: Option<String>,
}

/// Validates the bot-scoped computer body, or answers the uniform not-found.
fn computer_request(body: &Bytes) -> Result<ComputerStatusRequest, ApiError> {
    parse(body)
}

/// `computer.status` — where this bot's computer is (REQ-026, REQ-027).
///
/// A deployment without a computer backend answers `COMPUTERS_UNAVAILABLE`; a bot whose computer
/// has never started answers `stopped`, never an error. Another space's bot is `NOT_FOUND`.
async fn computer_status(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerStatusReply, ApiError> {
    let request = computer_request(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let computer = usecases::computer_status(app, &auth.actor.context, &request.bot_id).await?;
    Ok(computer.map_or_else(
        || ComputerStatusReply {
            status: "stopped".to_owned(),
            provider: app.services.computer.as_deref().map_or_else(
                || "none".to_owned(),
                |runtime| runtime.provider().to_owned(),
            ),
            started_at: None,
            screen_generation: None,
            takeover_by_user: None,
            takeover_at: None,
        },
        |computer| ComputerStatusReply::for_computer(&computer),
    ))
}

/// `computer.start` — start (or reuse) the bot's computer (REQ-026).
///
/// The boot is lazy by design: this procedure is the explicit ask for one, and the same computer
/// is reused by every later call. A deployment without a backend answers `COMPUTERS_UNAVAILABLE`.
async fn computer_start(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerStatusReply, ApiError> {
    let request = computer_request(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let private = match request.mode.as_deref() {
        None | Some("private") => true,
        Some("team") => false,
        Some(_) => {
            return Err(ApiError::new(
                crate::error::ErrorCode::BadRequest,
                "mode must be \"private\" or \"team\".",
            ));
        }
    };
    let outcome =
        usecases::ensure_computer(app, &auth.actor.context, &request.bot_id, private).await?;
    Ok(ComputerStatusReply::for_computer(&outcome.computer))
}

/// `computer.stop` — stop the bot's computer, preserving its home (REQ-026).
///
/// Idempotent: nothing to stop answers `stopped` with no timestamps.
async fn computer_stop(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerStatusReply, ApiError> {
    let request = computer_request(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let computer = usecases::stop_computer(app, &auth.actor.context, &request.bot_id).await?;
    Ok(ComputerStatusReply::for_computer(&computer))
}

/// `computer.openScreen` — mint a capability that opens the bot's screen (REQ-030).
///
/// The capability is returned once and the server stores only its hash. The computer must be
/// running: without a screen there is nothing to open.
async fn computer_open_screen(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerOpenScreenReply, ApiError> {
    let request = computer_request(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let issued = usecases::issue_screen_capability(app, &auth.actor.context, &request.bot_id)
        .await
        .map_err(|error| match error {
            boma_app::AppError::Rule(_) => ApiError::new(
                crate::error::ErrorCode::Conflict,
                "The computer is not running.",
            ),
            other => crate::error::ApiError::from(other),
        })?;
    Ok(ComputerOpenScreenReply {
        capability: issued.token,
        screen_generation: issued.screen_generation,
    })
}

/// The body of `routines.runNow` (REQ-025).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoutinesRunNowRequest {
    /// The routine to fire, once, as a normal turn.
    routine_id: String,
}

/// `routines.runNow` — fire a routine once as a normal turn (REQ-025).
async fn routines_run_now(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RoutineRunNowReply, ApiError> {
    let request: RoutinesRunNowRequest = parse(&body)?;
    let outcome =
        usecases::routines_run_now(state.app(), &auth.actor.context, &request.routine_id).await?;
    Ok(RoutineRunNowReply::from(&outcome))
}

/// The body of `threads.react`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadsReactRequest {
    /// The message reacted to.
    message_id: String,
    /// The emoji to toggle.
    emoji: String,
    /// Whether to add or remove.
    action: ReactionActionRequest,
}

/// The two things a member may do to a reaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReactionActionRequest {
    /// Add the emoji for this member.
    Add,
    /// Remove the emoji for this member.
    Remove,
}

/// `threads.react` — add or remove an emoji reaction on a message (REQ-008).
async fn threads_react(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ReactReply, ApiError> {
    let request: ThreadsReactRequest = parse(&body)?;
    let action = match request.action {
        ReactionActionRequest::Add => boma_app::ports::ReactionAction::Add,
        ReactionActionRequest::Remove => boma_app::ports::ReactionAction::Remove,
    };
    let outcome = usecases::react_to_message(
        state.app(),
        &auth.actor.context,
        &request.message_id,
        &request.emoji,
        action,
    )
    .await?;
    Ok(ReactReply {
        message_id: outcome.message_id.as_str().to_owned(),
        reactions: outcome
            .reactions
            .iter()
            .map(crate::wire::ReactionView::from)
            .collect(),
    })
}

/// `runs.list` — the activity projection, newest first.
async fn runs_list(state: AppState, auth: AuthContext, body: Bytes) -> Result<RunsReply, ApiError> {
    let request: RunsListRequest = parse(&body)?;
    let page = usecases::list_runs(
        state.app(),
        &auth.actor.context,
        request.bot_id.as_deref(),
        request.cursor.as_deref(),
        request.limit,
    )
    .await?;

    Ok(RunsReply {
        runs: page
            .runs
            .iter()
            .map(|activity| RunView::new(&activity.run, activity.prompt_snippet.clone()))
            .collect(),
        next_cursor: page.next_cursor,
    })
}

/// `runs.cancel` — stop a run, wherever it is (REQ-006).
async fn runs_cancel(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<CancelReply, ApiError> {
    let request: RunsCancelRequest = parse(&body)?;
    let run = usecases::cancel_run(state.app(), &auth.actor.context, &request.run_id).await?;
    Ok(CancelReply {
        id: run.id.as_str().to_owned(),
        status: run.status.as_str().to_owned(),
    })
}

/// The body of `runs.cancel`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunsCancelRequest {
    /// The run the caller wants stopped.
    run_id: String,
}

/// The body of `runs.retry`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunsRetryRequest {
    /// The run to re-queue.
    run_id: String,
}

/// `runs.retry` — re-queue a failed run for one more attempt (REQ-012).
async fn runs_retry(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RunRetryReply, ApiError> {
    let request: RunsRetryRequest = parse(&body)?;
    let run = usecases::retry_run(state.app(), &auth.actor.context, &request.run_id).await?;
    Ok(RunRetryReply::from(&run))
}

/// The body of `runs.pause`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunsPauseRequest {
    /// The queued run the caller wants parked.
    run_id: String,
}

/// The body of `runs.resume`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunsResumeRequest {
    /// The paused run the caller wants back on the queue.
    run_id: String,
}

/// `runs.pause` — park a queued run; no worker claims it until the person resumes it (REQ-067).
async fn runs_pause(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<CancelReply, ApiError> {
    let request: RunsPauseRequest = parse(&body)?;
    let run = usecases::pause_run(state.app(), &auth.actor.context, &request.run_id).await?;
    Ok(CancelReply {
        id: run.id.as_str().to_owned(),
        status: run.status.as_str().to_owned(),
    })
}

/// `runs.resume` — return a paused run to the queue (REQ-067).
async fn runs_resume(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<CancelReply, ApiError> {
    let request: RunsResumeRequest = parse(&body)?;
    let run = usecases::resume_run(state.app(), &auth.actor.context, &request.run_id).await?;
    Ok(CancelReply {
        id: run.id.as_str().to_owned(),
        status: run.status.as_str().to_owned(),
    })
}

/// The body of `runs.list`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunsListRequest {
    /// The bot whose runs the caller wants.
    #[serde(default)]
    bot_id: Option<String>,
    /// The cursor of the older page.
    #[serde(default)]
    cursor: Option<String>,
    /// How many rows the caller wants.
    #[serde(default)]
    limit: Option<usize>,
}

/// Parses a procedure body.
///
/// An unknown field is refused: the schema is strict, so a client that sends a field the
/// deployment does not know about hears about it instead of having it silently dropped
/// (spec/03-api-contract.md §3.1).
fn parse<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|error| {
        tracing::debug!(%error, "a procedure body did not match its schema");
        ApiError::bad_request()
    })
}

/// The header a client sends to choose the space.
pub const SPACE_HEADER: &str = "x-space-id";

/// The header the space can also be named with, spelled with the product prefix.
pub const SPACE_HEADER_PREFIXED: &str = "x-boma-space-id";

/// The cookie that carries the session.
pub const SESSION_COOKIE: &str = "session";

/// Reads the space a caller acts in from the headers.
#[must_use]
pub fn requested_space(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(SPACE_HEADER)
        .or_else(|| headers.get(SPACE_HEADER_PREFIXED))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

// Secrets for bots (REQ-046). Member-side only: the value the member sends is write-only, it is
// sealed under the space's data-encryption key and is never echoed back — not in a reply, not in
// an error, not in a log line. The list answers names and timestamps only.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretsSetRequest {
    /// The bot the secret belongs to.
    bot_id: String,
    /// The name the secret is stored under; must look like an environment variable.
    name: String,
    /// The value to seal. Plaintext in the wire body, ciphertext at rest.
    value: String,
}

/// `botSecrets.set` — seal and store a value under a bot-scoped name (REQ-046).
async fn secrets_set(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SecretSetReply, ApiError> {
    let request: SecretsSetRequest = parse(&body)?;
    usecases::bot_secret_set(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.name,
        secrecy::SecretString::from(request.value),
    )
    .await?;
    Ok(SecretSetReply { name: request.name })
}

/// The body of `botSecrets.list` and of a bot-scoped forget (REQ-046).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretsBotRequest {
    /// The bot whose secrets to answer for.
    bot_id: String,
}

/// `botSecrets.list` — the names of the bot's secrets, newest first (REQ-046).
async fn secrets_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SecretsReply, ApiError> {
    let request: SecretsBotRequest = parse(&body)?;
    let summaries =
        usecases::bot_secret_list(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(SecretsReply {
        secrets: summaries
            .into_iter()
            .map(|summary| SecretView {
                name: summary.name,
                created_at: summary.created_at,
            })
            .collect(),
    })
}

/// The body of `botSecrets.forget` (REQ-046).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretsForgetRequest {
    /// The bot whose secret is forgotten.
    bot_id: String,
    /// The name to forget.
    name: String,
}

/// `botSecrets.forget` — remove a secret; forgetting a missing one succeeds (REQ-046).
async fn secrets_forget(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SecretForgetReply, ApiError> {
    let request: SecretsForgetRequest = parse(&body)?;
    let removed = usecases::bot_secret_forget(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.name,
    )
    .await?;
    Ok(SecretForgetReply {
        name: request.name,
        removed,
    })
}

/// The body of `skills.list` (REQ-035): nothing, `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkillsListRequest {}

/// `skills.list` — the built-ins and the space's own, by name.
async fn skills_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SkillsReply, ApiError> {
    let _: SkillsListRequest = parse(&body)?;
    let skills = usecases::skills_list(state.app(), &auth.actor.context).await?;
    Ok(SkillsReply {
        skills: skills
            .into_iter()
            .map(|summary| SkillView {
                name: summary.name,
                description: summary.description,
                origin: summary.origin,
                plugin_source: summary.plugin_source,
                created_at: summary.created_at,
            })
            .collect(),
    })
}

/// The body of `skills.get` (REQ-035).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkillsGetRequest {
    /// The wire name to fetch.
    name: String,
}

/// `skills.get` — one skill with its instructions.
async fn skills_get(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SkillReply, ApiError> {
    let request: SkillsGetRequest = parse(&body)?;
    let skill = usecases::skill_get(state.app(), &auth.actor.context, &request.name).await?;
    Ok(SkillReply {
        name: skill.name,
        description: skill.description,
        instructions: skill.instructions,
        origin: skill.origin,
        plugin_source: skill.plugin_source,
    })
}

/// The body of `skills.teach` (REQ-035).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkillsTeachRequest {
    /// The wire name the bot will call.
    name: String,
    /// One line a person reads.
    description: String,
    /// The instructions handed to the model.
    instructions: String,
}

/// `skills.teach` — teach the space a skill; a built-in's name is refused.
async fn skills_teach(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<TeachSkillReply, ApiError> {
    let request: SkillsTeachRequest = parse(&body)?;
    usecases::teach_skill(
        state.app(),
        &auth.actor.context,
        &request.name,
        &request.description,
        &request.instructions,
        boma_domain::SkillOrigin::Taught,
        None,
    )
    .await?;
    Ok(TeachSkillReply { name: request.name })
}

/// The body of `mcp.servers.list` (REQ-033): nothing, `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpServersListRequest {}

fn mcp_view(summary: boma_app::ports::McpServerSummary) -> McpServerView {
    McpServerView {
        id: summary.id,
        name: summary.name,
        transport: summary.transport,
        url: summary.url,
        command: summary.command,
        auth_state: summary.auth_state,
        allow_private: summary.allow_private,
        oauth_configured: summary.oauth_configured,
        tool_count: summary.tool_count,
        created_at: summary.created_at,
    }
}

/// `mcp.servers.list` — the servers of the space, name ascending.
async fn mcp_servers_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServersListReply, ApiError> {
    let _: McpServersListRequest = parse(&body)?;
    let servers = usecases::mcp_servers_list(state.app(), &auth.actor.context).await?;
    Ok(McpServersListReply {
        servers: servers.into_iter().map(mcp_view).collect(),
    })
}

/// The OAuth endpoints a member registers for a server (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpOAuthConfigInput {
    /// The authorization endpoint the member's browser opens.
    authorization_url: String,
    /// The token endpoint the exchange calls.
    token_url: String,
    /// The client id the member registered at the server.
    client_id: String,
    /// The client secret, when the server issues confidential ones.
    client_secret: Option<String>,
}

/// The body of `mcp.servers.add` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpServersAddRequest {
    /// The member's display name for the server.
    name: String,
    /// `http` or `stdio`. Local command servers are refused in this build.
    transport: String,
    /// The HTTPS URL, for `http`.
    url: Option<String>,
    /// The command, for `stdio`. Always refused by this build.
    command: Option<String>,
    /// The bearer token, sealed per space at rest.
    bearer_token: Option<String>,
    /// The member's explicit allowance for a private endpoint (defect D-05).
    #[serde(default)]
    allow_private: bool,
    /// The OAuth endpoints, when the server uses a registered client.
    oauth: Option<McpOAuthConfigInput>,
}

/// `mcp.servers.add` — register a server; a stdio or otherwise refused registration fails with why.
async fn mcp_servers_add(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServerReply, ApiError> {
    let request: McpServersAddRequest = parse(&body)?;
    let transport = request
        .transport
        .parse::<boma_domain::McpTransport>()
        .map_err(|error| ApiError::from(boma_app::AppError::from(error)))?;
    let summary = usecases::mcp_server_add(
        state.app(),
        &auth.actor.context,
        &request.name,
        transport,
        request.url,
        request.command,
        request.bearer_token.map(secrecy::SecretString::from),
        request.allow_private,
        request.oauth.map(|oauth| boma_app::ports::McpOAuthConfig {
            authorization_url: oauth.authorization_url,
            token_url: oauth.token_url,
            client_id: oauth.client_id,
            client_secret: oauth.client_secret.map(secrecy::SecretString::from),
        }),
    )
    .await?;
    Ok(McpServerReply {
        server: mcp_view(summary),
    })
}

/// The body of `mcp.servers.remove` and `mcp.servers.probe` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpServersServerRequest {
    /// The server's id.
    server_id: String,
}

/// `mcp.servers.remove` — remove a server; removing a missing one is a success.
async fn mcp_servers_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServersRemoveReply, ApiError> {
    let request: McpServersServerRequest = parse(&body)?;
    let removed =
        usecases::mcp_server_remove(state.app(), &auth.actor.context, &request.server_id).await?;
    Ok(McpServersRemoveReply { removed })
}

/// `mcp.servers.probe` — initialize and discover the server's tools.
async fn mcp_servers_probe(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServerReply, ApiError> {
    let request: McpServersServerRequest = parse(&body)?;
    let summary =
        usecases::mcp_server_probe(state.app(), &auth.actor.context, &request.server_id).await?;
    Ok(McpServerReply {
        server: mcp_view(summary),
    })
}

/// The body of `connections.list` (REQ-032): nothing, `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectionsListRequest {}

fn connector_view(summary: boma_app::ports::ConnectorSummary) -> ConnectorView {
    ConnectorView {
        connector_id: summary.connector_id,
        name: summary.name,
        description: summary.description,
        auth_kind: summary.auth_kind,
        installed: summary.installed,
    }
}

/// `connections.list` — the catalogue with this space's install state.
async fn connections_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ConnectionsListReply, ApiError> {
    let _: ConnectionsListRequest = parse(&body)?;
    let connectors = usecases::connections_list(state.app(), &auth.actor.context).await?;
    Ok(ConnectionsListReply {
        connections: connectors.into_iter().map(connector_view).collect(),
    })
}

/// The body of `connections.install` (REQ-032).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectionsInstallRequest {
    /// The catalogue id to install.
    connector_id: String,
    /// The bearer key for an `api_key` connector. Never returned by any API.
    #[serde(default)]
    api_key: Option<String>,
}

/// `connections.install` — install or re-key a connector.
async fn connections_install(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ConnectionsInstallReply, ApiError> {
    let request: ConnectionsInstallRequest = parse(&body)?;
    let connector = usecases::connections_install(
        state.app(),
        &auth.actor.context,
        &request.connector_id,
        request.api_key.map(secrecy::SecretString::from),
    )
    .await?;
    Ok(ConnectionsInstallReply {
        connector: connector_view(connector),
    })
}

/// The body of `connections.remove` (REQ-032).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectionsRemoveRequest {
    /// The catalogue id to revoke.
    connector_id: String,
}

/// `connections.remove` — revoke the installation; revoking a missing one is a success.
async fn connections_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ConnectionsRemoveReply, ApiError> {
    let request: ConnectionsRemoveRequest = parse(&body)?;
    let removed =
        usecases::connections_remove(state.app(), &auth.actor.context, &request.connector_id)
            .await?;
    Ok(ConnectionsRemoveReply { removed })
}

/// The body of `marketplace.list` (REQ-034): nothing, `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceListRequest {}

fn plugin_install_view(outcome: boma_app::ports::PluginInstallOutcome) -> PluginInstallView {
    PluginInstallView {
        name: outcome.name,
        source: outcome.source,
        resolved_sha: outcome.resolved_sha,
        skill_count: outcome.skill_count,
        mcp_server_count: outcome.mcp_server_count,
        skipped: outcome.skipped,
        created_at: String::new(),
    }
}

/// `marketplace.list` — the catalogue and this space's installs.
async fn marketplace_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MarketplaceListReply, ApiError> {
    let _: MarketplaceListRequest = parse(&body)?;
    let (connectors, installed) =
        usecases::marketplace_list(state.app(), &auth.actor.context).await?;
    Ok(MarketplaceListReply {
        connectors: connectors.into_iter().map(connector_view).collect(),
        installed_plugins: installed
            .into_iter()
            .map(|summary| PluginInstallView {
                name: summary.name,
                source: summary.source,
                resolved_sha: summary.resolved_sha,
                skill_count: summary
                    .parts
                    .get("skills")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(0, |count| u32::try_from(count).unwrap_or(u32::MAX)),
                mcp_server_count: summary
                    .parts
                    .get("mcpServers")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(0, |count| u32::try_from(count).unwrap_or(u32::MAX)),
                skipped: summary.skipped.as_array().cloned().unwrap_or_default(),
                created_at: summary.created_at,
            })
            .collect(),
    })
}

/// The body of `marketplace.submit` (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceSubmitRequest {
    /// The source reference; the reply carries an id and nothing about who submitted.
    source: String,
}

/// `marketplace.submit` — submit a plugin for review (authed, ten per hour per submitter).
async fn marketplace_submit(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MarketplaceSubmitReply, ApiError> {
    let request: MarketplaceSubmitRequest = parse(&body)?;
    let id =
        usecases::marketplace_submit(state.app(), &auth.actor.context, &request.source).await?;
    Ok(MarketplaceSubmitReply { ok: true, id })
}

/// The body of `marketplace.install` (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceInstallRequest {
    /// The source reference.
    source: String,
}

/// `marketplace.install` — fetch, validate and install; the no-local-code rule applies.
async fn marketplace_install(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MarketplaceInstallReply, ApiError> {
    let request: MarketplaceInstallRequest = parse(&body)?;
    let outcome =
        usecases::marketplace_install(state.app(), &auth.actor.context, &request.source).await?;
    Ok(MarketplaceInstallReply {
        install: plugin_install_view(outcome),
    })
}

/// The body of `mcp.oauth.start` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpOAuthStartRequest {
    /// The server to sign in to.
    server_id: String,
}

/// `mcp.oauth.start` — open the sign-in URL for a remote server with OAuth.
async fn mcp_oauth_start(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpOAuthStartReply, ApiError> {
    let request: McpOAuthStartRequest = parse(&body)?;
    let started =
        usecases::mcp_oauth_start(state.app(), &auth.actor.context, &request.server_id).await?;
    Ok(McpOAuthStartReply {
        authorization_url: started.authorization_url,
        state: started.state,
    })
}

/// The body of `mcp.oauth.complete` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpOAuthCompleteRequest {
    /// The server to sign in to.
    server_id: String,
    /// The `code` from the pasted callback URL.
    code: String,
    /// The `state` from the pasted callback URL.
    state: String,
}

/// `mcp.oauth.complete` — exchange the code for a token, seal it, discover the tools.
async fn mcp_oauth_complete(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServerReply, ApiError> {
    let request: McpOAuthCompleteRequest = parse(&body)?;
    let summary = usecases::mcp_oauth_complete(
        state.app(),
        &auth.actor.context,
        &request.server_id,
        &request.code,
        &request.state,
    )
    .await?;
    Ok(McpServerReply {
        server: mcp_view(summary),
    })
}

fn decode_base64(encoded: &str) -> Result<Vec<u8>, ()> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .map_err(|_| ())
}

fn artifact_view(summary: boma_app::ports::ArtifactSummary) -> ArtifactView {
    ArtifactView {
        id: summary.id,
        bot_id: summary.bot_id,
        group_id: summary.group_id,
        run_id: summary.run_id,
        name: summary.name,
        description: summary.description,
        declared_mime: summary.declared_mime,
        mime_type: summary.mime_type,
        size: summary.size,
        hash: summary.hash,
        root_artifact_id: summary.root_artifact_id,
        version: summary.version,
        created_at: summary.created_at,
    }
}

/// The body of `artifacts.create` (REQ-036).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactsCreateRequest {
    /// The bot that produced the artifact, when one did.
    #[serde(default)]
    bot_id: Option<String>,
    /// The group thread it belongs to, when one does.
    #[serde(default)]
    group_id: Option<String>,
    /// The member's file name.
    name: String,
    /// One line about what the file is.
    #[serde(default)]
    description: String,
    /// What the client claims the file is; the bytes get the final word (defect D-03).
    mime_type: String,
    /// The content, base64-encoded.
    content_base64: String,
    /// The family to version; omit to root a new one.
    #[serde(default)]
    root_artifact_id: Option<String>,
    /// The client's idempotency token.
    #[serde(default)]
    client_nonce: Option<String>,
}

/// `artifacts.create` — sniff, store, record. The reply never carries the bytes (the golden).
async fn artifacts_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ArtifactCreateReply, ApiError> {
    let request: ArtifactsCreateRequest = parse(&body)?;
    let content = decode_base64(&request.content_base64).map_err(|()| ApiError::bad_request())?;
    let created = usecases::artifact_create(
        state.app(),
        &auth.actor.context,
        usecases::ArtifactInput {
            bot_id: request.bot_id,
            group_id: request.group_id,
            run_id: None,
            name: request.name,
            description: request.description,
            declared_mime: request.mime_type,
            content,
            root_artifact_id: request.root_artifact_id,
            client_nonce: request.client_nonce,
        },
    )
    .await?;
    Ok(ArtifactCreateReply {
        id: created.id,
        version: created.version,
        root_artifact_id: created.root_artifact_id,
        size: created.size,
        hash: created.hash,
    })
}

/// The body of `artifacts.getById`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactsGetByIdRequest {
    /// The artifact; exactly this row.
    artifact_id: String,
}

/// The body of `artifacts.get`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactsGetRequest {
    /// The artifact; the family root works too.
    artifact_id: String,
    /// A specific version; `None` is the latest.
    #[serde(default)]
    version: Option<i32>,
}

/// `artifacts.get` — the artifact plus its bytes (contentBase64).
async fn artifacts_get(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ArtifactGetReply, ApiError> {
    let request: ArtifactsGetRequest = parse(&body)?;
    let (summary, bytes) = usecases::artifact_get(
        state.app(),
        &auth.actor.context,
        &request.artifact_id,
        request.version,
    )
    .await?;
    Ok(ArtifactGetReply {
        artifact: artifact_view(summary),
        content_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
    })
}

/// `artifacts.getById` — the artifact exactly as identified (no version pin, no family root).
async fn artifacts_get_by_id(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ArtifactGetReply, ApiError> {
    let request: ArtifactsGetByIdRequest = parse(&body)?;
    let (summary, bytes) =
        usecases::artifact_get_by_id(state.app(), &auth.actor.context, &request.artifact_id)
            .await?;
    Ok(ArtifactGetReply {
        artifact: artifact_view(summary),
        content_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
    })
}

/// The body of `artifacts.listVersions`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactsListVersionsRequest {
    /// The artifact; any version's id works, as does the family root.
    artifact_id: String,
}

/// `artifacts.listVersions` — the family, newest first (the golden).
async fn artifacts_list_versions(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ArtifactVersionsReply, ApiError> {
    let request: ArtifactsListVersionsRequest = parse(&body)?;
    let versions =
        usecases::artifact_versions(state.app(), &auth.actor.context, &request.artifact_id).await?;
    Ok(ArtifactVersionsReply {
        versions: versions
            .into_iter()
            .map(|summary| ArtifactVersionView {
                id: summary.id,
                version: summary.version,
                root_artifact_id: summary.root_artifact_id,
            })
            .collect(),
    })
}

/// The body of `artifacts.listSpace`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactsListSpaceRequest {
    /// Filter by producer bot, when given.
    #[serde(default)]
    bot_id: Option<String>,
    /// Filter by group thread, when given.
    #[serde(default)]
    group_id: Option<String>,
    /// The cap on the answer; 1..=100, default 50.
    #[serde(default)]
    limit: Option<i64>,
}

/// `artifacts.listSpace` — the latest version of every family in the space.
async fn artifacts_list_space(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ArtifactListSpaceReply, ApiError> {
    let request: ArtifactsListSpaceRequest = parse(&body)?;
    let artifacts = usecases::artifact_list_space(
        state.app(),
        &auth.actor.context,
        request.bot_id.as_deref(),
        request.group_id.as_deref(),
        request.limit,
    )
    .await?;
    Ok(ArtifactListSpaceReply {
        artifacts: artifacts.into_iter().map(artifact_view).collect(),
    })
}

/// The body of `artifacts.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArtifactsRemoveRequest {
    /// The artifact; any member may remove any family of the space.
    artifact_id: String,
}

/// `artifacts.remove` — the whole family, rows and unreferenced blobs.
async fn artifacts_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ArtifactRemoveReply, ApiError> {
    let request: ArtifactsRemoveRequest = parse(&body)?;
    let removed =
        usecases::artifact_remove(state.app(), &auth.actor.context, &request.artifact_id).await?;
    Ok(ArtifactRemoveReply { removed })
}

/// The body of `pages.save` (REQ-037).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PagesSaveRequest {
    /// The page to version; omit to create one.
    #[serde(default)]
    page_id: Option<String>,
    /// The library label; required on create.
    #[serde(default)]
    name: Option<String>,
    /// The bot that produced it, when one did.
    #[serde(default)]
    bot_id: Option<String>,
    /// The block document.
    source: serde_json::Value,
    /// The client's idempotency token.
    #[serde(default)]
    client_nonce: Option<String>,
}

/// `pages.save` — create or version a page.
async fn pages_save(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<PagesSaveReply, ApiError> {
    let request: PagesSaveRequest = parse(&body)?;
    let summary = usecases::pages_save(
        state.app(),
        &auth.actor.context,
        usecases::PageSaveInput {
            page_id: request.page_id,
            name: request.name,
            bot_id: request.bot_id,
            run_id: None,
            source: request.source,
            client_nonce: request.client_nonce,
        },
    )
    .await?;
    let (page, version) =
        usecases::pages_get(state.app(), &auth.actor.context, &summary.id, None).await?;
    Ok(PagesSaveReply {
        page_id: page.id,
        version: version.version,
        name: page.name,
    })
}

/// The body of `pages.get` (REQ-037).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PagesGetRequest {
    /// The page.
    page_id: String,
    /// A specific version; `None` is the latest.
    #[serde(default)]
    version: Option<i32>,
}

/// `pages.get` — the page plus its source.
async fn pages_get(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<PagesGetReply, ApiError> {
    let request: PagesGetRequest = parse(&body)?;
    let (page, version) = usecases::pages_get(
        state.app(),
        &auth.actor.context,
        &request.page_id,
        request.version,
    )
    .await?;
    Ok(PagesGetReply {
        page_id: page.id,
        name: page.name,
        visibility: page.visibility,
        current_version_id: page.current_version_id,
        version: version.version,
        source: version.source,
        updated_at: version.compiled_at,
    })
}

/// The body of `pages.share` and `pages.revokeShare`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PagesShareRequest {
    /// The page.
    page_id: String,
}

/// `pages.share` — mint the capability; it is returned once and never stored.
async fn pages_share(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<PagesShareReply, ApiError> {
    let request: PagesShareRequest = parse(&body)?;
    let token = usecases::pages_share(state.app(), &auth.actor.context, &request.page_id).await?;
    Ok(PagesShareReply {
        path: format!("/api/v1/pages/shared/{token}"),
        token,
    })
}

/// `pages.revokeShare` — the page is private again.
async fn pages_revoke_share(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<PagesRevokeShareReply, ApiError> {
    let request: PagesShareRequest = parse(&body)?;
    let revoked =
        usecases::pages_revoke_share(state.app(), &auth.actor.context, &request.page_id).await?;
    Ok(PagesRevokeShareReply { revoked })
}

fn ticket_view(summary: boma_app::ports::TicketSummary) -> TicketView {
    TicketView {
        id: summary.id,
        key: summary.key,
        title: summary.title,
        description: summary.description,
        state: summary.state,
        assignee_bot_id: summary.assignee_bot_id,
        comment_count: summary.comment_count,
        created_at: summary.created_at,
        updated_at: summary.updated_at,
    }
}

fn ticket_comment_view(comment: boma_app::ports::TicketComment) -> TicketCommentView {
    TicketCommentView {
        id: comment.id,
        ticket_id: comment.ticket_id,
        author_kind: comment.author_kind,
        author_id: comment.author_id,
        body: comment.body,
        created_at: comment.created_at,
    }
}

/// The body of `boards.get` (REQ-038): nothing, `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BoardsGetRequest {}

/// `boards.get` — the tickets of this space, in key order. Absent when the flag is off.
async fn boards_get(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<BoardsGetReply, ApiError> {
    let _: BoardsGetRequest = parse(&body)?;
    let tickets = usecases::boards_get(state.app(), &auth.actor.context).await?;
    Ok(BoardsGetReply {
        tickets: tickets.into_iter().map(ticket_view).collect(),
    })
}

/// The body of `tickets.create` (REQ-038).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TicketsCreateRequest {
    /// What to do.
    title: String,
    /// Longer context.
    #[serde(default)]
    description: String,
    /// The bot assigned to work it, when one is.
    #[serde(default)]
    assignee_bot_id: Option<String>,
}

/// `tickets.create` — a new ticket with the next key of the space.
async fn tickets_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<TicketReply, ApiError> {
    let request: TicketsCreateRequest = parse(&body)?;
    let ticket = usecases::tickets_create(
        state.app(),
        &auth.actor.context,
        &request.title,
        &request.description,
        request.assignee_bot_id.as_deref(),
    )
    .await?;
    Ok(TicketReply {
        ticket: ticket_view(ticket),
    })
}

/// The body of `tickets.move` (REQ-038).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TicketsMoveRequest {
    /// The ticket.
    ticket_id: String,
    /// The new column.
    state: String,
}

/// `tickets.move` — the ticket lands in another column.
async fn tickets_move(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<TicketReply, ApiError> {
    let request: TicketsMoveRequest = parse(&body)?;
    let ticket = usecases::tickets_move(
        state.app(),
        &auth.actor.context,
        &request.ticket_id,
        &request.state,
    )
    .await?;
    Ok(TicketReply {
        ticket: ticket_view(ticket),
    })
}

/// The body of `tickets.comment` (REQ-038).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TicketsCommentRequest {
    /// The ticket.
    ticket_id: String,
    /// The comment.
    body: String,
}

/// `tickets.comment` — append a comment; the counter bumps atomically.
async fn tickets_comment(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<TicketCommentReply, ApiError> {
    let request: TicketsCommentRequest = parse(&body)?;
    let comment = usecases::tickets_comment(
        state.app(),
        &auth.actor.context,
        &request.ticket_id,
        &request.body,
    )
    .await?;
    Ok(TicketCommentReply {
        comment: ticket_comment_view(comment),
    })
}

fn mail_policy_view(rule: boma_app::ports::MailRule) -> MailPolicyView {
    MailPolicyView {
        bot_id: rule.bot_id,
        policy: rule.policy,
        allowed_senders: rule.allowed_senders,
        updated_at: rule.updated_at,
    }
}

fn held_mail_view(held: boma_app::ports::HeldMail) -> HeldMailView {
    HeldMailView {
        id: held.id,
        sender: held.sender,
        subject: held.subject,
        body: held.body,
        reason: held.reason,
        created_at: held.created_at,
    }
}

/// The body of `mail.policyGet`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MailPolicyGetRequest {
    /// The bot whose policy to read.
    bot_id: String,
}

/// `mail.policyGet` — the bot's sender rule, `hold` by default.
async fn mail_policy_get(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MailPolicyReply, ApiError> {
    let request: MailPolicyGetRequest = parse(&body)?;
    let rule = usecases::mail_policy_get(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(MailPolicyReply {
        policy: mail_policy_view(rule),
    })
}

/// The body of `mail.policySet`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MailPolicySetRequest {
    /// The bot whose policy to set.
    bot_id: String,
    /// `hold` (the default) or `drop`.
    policy: String,
    /// Exact addresses admitted to start work.
    #[serde(default)]
    allowed_senders: Vec<String>,
}

/// `mail.policySet` — create or replace the sender rule.
async fn mail_policy_set(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MailPolicyReply, ApiError> {
    let request: MailPolicySetRequest = parse(&body)?;
    let rule = usecases::mail_policy_set(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.policy,
        &request.allowed_senders,
    )
    .await?;
    Ok(MailPolicyReply {
        policy: mail_policy_view(rule),
    })
}

/// The body of `mail.held.list`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MailHeldListRequest {
    /// The bot whose held messages to list.
    bot_id: String,
}

/// `mail.held.list` — the held queue, newest first.
async fn mail_held_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<HeldListReply, ApiError> {
    let request: MailHeldListRequest = parse(&body)?;
    let held = usecases::mail_held_list(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(HeldListReply {
        held: held.into_iter().map(held_mail_view).collect(),
    })
}

/// The body of `mail.held.release` and `mail.held.drop`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MailHeldRequest {
    /// The held message.
    held_id: String,
}

/// `mail.held.release` — admit the sender for this message and start the run.
async fn mail_held_release(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<HeldReleaseReply, ApiError> {
    let request: MailHeldRequest = parse(&body)?;
    let run_id =
        usecases::mail_held_release(state.app(), &auth.actor.context, &request.held_id).await?;
    Ok(HeldReleaseReply { run_id })
}

/// `mail.held.drop` — discard the held message.
async fn mail_held_drop(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<HeldDropReply, ApiError> {
    let request: MailHeldRequest = parse(&body)?;
    let dropped =
        usecases::mail_held_drop(state.app(), &auth.actor.context, &request.held_id).await?;
    Ok(HeldDropReply { dropped })
}

/// The body of `messaging.installUrl`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessagingInstallUrlRequest {
    /// The bot the installation will belong to.
    bot_id: String,
    /// The platform; only `slack` installs with a handshake.
    platform: String,
}

/// `messaging.installUrl` — the Slack OAuth URL the member opens (REQ-048).
async fn messaging_install_url(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<InstallUrlReply, ApiError> {
    let request: MessagingInstallUrlRequest = parse(&body)?;
    let outcome = usecases::messaging_install_url(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.platform,
    )
    .await?;
    Ok(InstallUrlReply {
        url: outcome.authorization_url,
    })
}

/// The body of `messaging.installTelegram`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessagingInstallTelegramRequest {
    /// The bot the Telegram bot will drive.
    bot_id: String,
    /// The token from BotFather.
    bot_token: String,
}

/// `messaging.installTelegram` — validate the token, seal everything, register the webhook (REQ-048).
async fn messaging_install_telegram(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<InstallTelegramReply, ApiError> {
    let request: MessagingInstallTelegramRequest = parse(&body)?;
    let (outcome, registered) = usecases::messaging_install_telegram(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.bot_token,
    )
    .await?;
    Ok(InstallTelegramReply {
        installation_id: outcome.installation_id,
        bot_username: outcome.bot_username,
        webhook_url: outcome.webhook_url,
        webhook_secret: outcome.webhook_secret,
        registered,
    })
}

/// The body of `messaging.installGeneric`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessagingInstallGenericRequest {
    /// The bot the installation will belong to.
    bot_id: String,
    /// The workspace name: lowercase letters, digits, hyphens, underscores.
    name: String,
    /// The HTTPS reply target.
    reply_url: String,
    /// Whether the reply target may be a private host (audited, D-05).
    #[serde(default)]
    allow_private: bool,
}

/// `messaging.installGeneric` — mint the secret, return it once (REQ-048).
async fn messaging_install_generic(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<InstallGenericReply, ApiError> {
    let request: MessagingInstallGenericRequest = parse(&body)?;
    let outcome = usecases::messaging_install_generic(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.name,
        &request.reply_url,
        request.allow_private,
    )
    .await?;
    Ok(InstallGenericReply {
        installation_id: outcome.installation_id,
        webhook_url: outcome.webhook_url,
        secret: outcome.secret,
    })
}

/// `messaging.list` — the installations of this space, newest first.
async fn messaging_list(
    state: AppState,
    auth: AuthContext,
) -> Result<InstallationsReply, ApiError> {
    let installations = usecases::messaging_list(state.app(), &auth.actor.context).await?;
    Ok(InstallationsReply {
        installations: installations
            .iter()
            .map(MessagingInstallationView::from)
            .collect(),
    })
}

/// The body of `messaging.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessagingRemoveRequest {
    /// The installation to remove.
    installation_id: String,
}

/// `messaging.remove` — stop the installation answering; history keeps (REQ-048).
async fn messaging_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MessagingRemoveReply, ApiError> {
    let request: MessagingRemoveRequest = parse(&body)?;
    let removed =
        usecases::messaging_remove(state.app(), &auth.actor.context, &request.installation_id)
            .await?;
    Ok(MessagingRemoveReply { removed })
}

/// The body of `messaging.conversations`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessagingConversationsRequest {
    /// The installation whose conversations to list.
    installation_id: String,
}

/// `messaging.conversations` — the external conversations of one installation (REQ-048).
async fn messaging_conversations(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ConversationsReply, ApiError> {
    let request: MessagingConversationsRequest = parse(&body)?;
    let conversations = usecases::messaging_conversations(
        state.app(),
        &auth.actor.context,
        &request.installation_id,
    )
    .await?;
    Ok(ConversationsReply {
        conversations: conversations
            .into_iter()
            .map(|conversation| ConversationView {
                thread_id: conversation.thread_id,
                conversation_id: conversation.conversation_id,
                created_at: conversation.created_at,
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// M8 — models (REQ-052)
// ---------------------------------------------------------------------------

/// `models.catalog` — what this deployment reaches, shaped exactly as the golden
/// (`spec/golden/models-catalog.json`): providers, models, context windows, roles. No keys, no
/// account identifiers: whether a provider is usable is `models.credentials` (REQ-052).
/// `models.discovered` — what local loopback servers offer (REQ-075).
///
/// Discovery is best effort and answered synchronously at call time: a deployment whose local
/// servers are on gets the picker-ready list, one whose servers died gets an empty list and no
/// error. The loopback is probed from this process; nothing reaches the network beyond it.
async fn models_discovered(
    _state: AppState,
    _auth: AuthContext,
) -> Result<ModelsDiscoveredReply, ApiError> {
    let discovered = boma_app::local_models::discover().await;
    Ok(ModelsDiscoveredReply {
        models: discovered
            .models
            .into_iter()
            .map(|model| LocalDiscoveredModelView {
                source: model.source,
                id: model.id,
            })
            .collect(),
    })
}

async fn models_catalog(
    state: AppState,
    auth: AuthContext,
) -> Result<ModelsCatalogReply, ApiError> {
    let catalog = usecases::models_catalog(state.app(), &auth.actor.context).await?;
    Ok(ModelsCatalogReply {
        providers: catalog
            .providers
            .iter()
            .map(|provider| CatalogProviderView {
                id: provider.id.clone(),
                name: provider.name.clone(),
                models: provider
                    .models
                    .iter()
                    .map(|model| CatalogModelView {
                        id: model.id.clone(),
                        name: model.name.clone(),
                        context_window: model.context_window,
                        roles: model.roles.clone(),
                    })
                    .collect(),
            })
            .collect(),
    })
}

/// `models.credentials` — the providers this space connected (REQ-052).
async fn models_credentials(
    state: AppState,
    auth: AuthContext,
) -> Result<ModelCredentialsReply, ApiError> {
    let credentials = usecases::models_credentials(state.app(), &auth.actor.context).await?;
    Ok(ModelCredentialsReply {
        credentials: credentials
            .into_iter()
            .map(|credential| ModelCredentialView {
                provider: credential.provider,
                status: credential.status,
                created_at: credential.created_at,
            })
            .collect(),
    })
}

/// The body of `models.connect`. No `Debug`: the key must never reach a log line.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelsConnectRequest {
    /// The provider to connect.
    provider: String,
    /// The key the member pasted from the provider's console.
    api_key: String,
}

/// `models.connect` — validate a key live, then seal it for the space (REQ-052).
///
/// A provider that refuses the key answers `400` with the provider's own reason: the member just
/// pasted it, so the reason is theirs to see, and nothing internal is added to it.
async fn models_connect(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ModelConnectReply, ApiError> {
    let request: ModelsConnectRequest = parse(&body)?;
    if request.api_key.trim().is_empty() {
        return Err(ApiError::explained(
            "the provider key cannot be empty".to_owned(),
        ));
    }
    match usecases::models_connect(
        state.app(),
        &auth.actor.context,
        &request.provider,
        secrecy::SecretString::from(request.api_key),
    )
    .await
    {
        Ok(outcome) => Ok(ModelConnectReply {
            provider: outcome.provider,
            connected: true,
        }),
        Err(AppError::Rule(error)) => Err(ApiError::explained(error.to_string())),
        Err(error) => Err(error.into()),
    }
}

/// The body of `models.disconnect`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelsDisconnectRequest {
    /// The provider to forget.
    provider: String,
}

/// `models.disconnect` — forget one provider's key (REQ-052). Absent is a success.
async fn models_disconnect(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ModelConnectReply, ApiError> {
    let request: ModelsDisconnectRequest = parse(&body)?;
    let removed =
        usecases::models_disconnect(state.app(), &auth.actor.context, &request.provider).await?;
    Ok(ModelConnectReply {
        provider: request.provider.trim().to_ascii_lowercase(),
        connected: !removed,
    })
}

/// `models.preferences.get` — the person's standing model choice (REQ-052).
async fn models_preferences_get(
    state: AppState,
    auth: AuthContext,
) -> Result<ModelPreferenceReply, ApiError> {
    let model_key = usecases::models_preference(state.app(), &auth.actor.context).await?;
    Ok(ModelPreferenceReply { model_key })
}

/// The body of `models.preferences.set`. A null clears the preference.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelsPreferenceRequest {
    /// The model key to pin, or `null` to return to the deployment default.
    model_key: Option<String>,
}

/// `models.preferences.set` — pin the model this person's runs execute on (REQ-052).
async fn models_preferences_set(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ModelPreferenceReply, ApiError> {
    let request: ModelsPreferenceRequest = parse(&body)?;
    let model_key = usecases::models_set_preference(
        state.app(),
        &auth.actor.context,
        request.model_key.as_deref(),
    )
    .await?;
    Ok(ModelPreferenceReply { model_key })
}

// ---------------------------------------------------------------------------
// M8 — billing (REQ-018, REQ-051)
// ---------------------------------------------------------------------------

/// `billing.status` — the plan, the credits, the renewal (REQ-051).
async fn billing_status(
    state: AppState,
    auth: AuthContext,
) -> Result<BillingStatusReply, ApiError> {
    let status = usecases::billing_status(state.app(), &auth.actor.context).await?;
    Ok(BillingStatusReply {
        plan: status.plan,
        status: status.status,
        credits_minor_units: status.credits_minor_units,
        current_period_end: status.current_period_end,
        checkout_allowed: status.checkout_allowed,
    })
}

/// The body of `billing.checkout`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BillingCheckoutRequest {
    /// The plan to buy.
    plan: String,
}

/// `billing.checkout` — open a checkout session for one plan (REQ-051).
async fn billing_checkout(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<BillingCheckoutReply, ApiError> {
    let request: BillingCheckoutRequest = parse(&body)?;
    let checkout =
        usecases::billing_checkout(state.app(), &auth.actor.context, &request.plan).await?;
    Ok(BillingCheckoutReply {
        url: checkout.url,
        provider_ref: checkout.provider_ref,
    })
}

/// `billing.limit.get` — the cap and the month-to-date spend (REQ-018).
async fn billing_limit_get(
    state: AppState,
    auth: AuthContext,
) -> Result<SpendingLimitReply, ApiError> {
    let limit = usecases::spending_limit_get(state.app(), &auth.actor.context).await?;
    Ok(SpendingLimitReply {
        enabled: limit.enabled,
        currency: limit.currency,
        cap_minor_units: limit.cap_minor_units,
        spent_minor_units: limit.spent_minor_units,
        remaining_minor_units: limit.remaining_minor_units,
    })
}

/// The body of `billing.limit.set`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BillingLimitRequest {
    /// Whether the cap is enforced.
    enabled: bool,
    /// The ISO currency of the cap.
    currency: String,
    /// The cap in minor units of that currency.
    cap_minor_units: i64,
}

/// `billing.limit.set` — create or replace the space's cap (REQ-018).
async fn billing_limit_set(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SpendingLimitReply, ApiError> {
    let request: BillingLimitRequest = parse(&body)?;
    let limit = usecases::spending_limit_set(
        state.app(),
        &auth.actor.context,
        request.enabled,
        &request.currency,
        request.cap_minor_units,
    )
    .await?;
    Ok(SpendingLimitReply {
        enabled: limit.enabled,
        currency: limit.currency,
        cap_minor_units: limit.cap_minor_units,
        spent_minor_units: limit.spent_minor_units,
        remaining_minor_units: limit.remaining_minor_units,
    })
}

// ---------------------------------------------------------------------------
// M9 — observability (REQ-045)
// ---------------------------------------------------------------------------

/// `audit.export` — the space's trail as JSONL (REQ-045).
///
/// The caller must be an owner or admin of the space the session acts in; everyone else gets
/// the same refusal a non-member sees (a `403`, never anything about another tenant).
async fn audit_export(state: AppState, auth: AuthContext) -> Result<AuditExportReply, ApiError> {
    let outcome = usecases::audit_export(state.app(), &auth.actor.context).await?;
    Ok(AuditExportReply {
        version: "1".to_owned(),
        events: u64::try_from(outcome.events).unwrap_or(u64::MAX),
        jsonl: outcome.jsonl,
    })
}

/// `security.events.list` — the space's recent security events (REQ-045).
async fn security_events_list(
    state: AppState,
    auth: AuthContext,
) -> Result<SecurityEventsReply, ApiError> {
    let outcome = usecases::security_events_list(state.app(), &auth.actor.context).await?;
    Ok(SecurityEventsReply {
        events: outcome
            .events
            .into_iter()
            .map(|event| AuditEventView {
                id: event.id,
                space_id: event.space_id,
                actor_user_id: event.actor_user_id,
                kind: event.kind,
                target: event.target,
                metadata: boma_domain::redact_audit_value(&event.metadata),
                created_at: event.created_at,
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// M9 — account export and deletion (REQ-044)
// ---------------------------------------------------------------------------

/// `export.start` — mints an id, marks pending; the archive renders on `export.status|download`.
///
/// The golden `export-start` pins the shape and the id minting (`exp_…`), not the fact that the
/// archive exists: archives are rendered on demand inside the member's session.
async fn export_start(state: AppState, auth: AuthContext) -> Result<ExportStartReply, ApiError> {
    let outcome =
        boma_app::account_export::account_export_start(state.app(), &auth.actor.context).await?;
    Ok(ExportStartReply {
        export_id: outcome.export_id,
        status: outcome.status,
    })
}

/// `export.status` — counts, readiness, and where to pick the bytes up.
async fn export_status(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ExportStatusReply, ApiError> {
    let request: ExportIdRequest = parse(&body)?;
    let outcome = boma_app::account_export::account_export_status(
        state.app(),
        &auth.actor.context,
        &request.export_id,
    )
    .await?;
    Ok(ExportStatusReply {
        export_id: outcome.export_id,
        status: outcome.status,
        counts: ExportCountsView {
            bots: outcome.counts.bots,
            threads: outcome.counts.threads,
            messages: outcome.counts.messages,
            memory: outcome.counts.memory,
            routines: outcome.counts.routines,
            artifacts: outcome.counts.artifacts,
        },
        download_url: outcome.download_url,
    })
}

/// `export.download` — the URL of the cookie-gated download route.
async fn export_download(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ExportDownloadReply, ApiError> {
    let request: ExportIdRequest = parse(&body)?;
    let outcome = boma_app::account_export::account_export_status(
        state.app(),
        &auth.actor.context,
        &request.export_id,
    )
    .await?;
    Ok(ExportDownloadReply {
        download_url: outcome.download_url,
    })
}

/// `account.delete` — the person is gone once the word `delete` is spelled exactly.
async fn account_delete(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AccountDeleteReply, ApiError> {
    let request: AccountDeleteRequest = parse(&body)?;
    if !boma_app::account_export::confirmation_is_literal(&request.confirmation) {
        return Err(ApiError::bad_request());
    }
    let outcome =
        boma_app::account_export::account_delete(state.app(), &auth.actor.context).await?;
    Ok(AccountDeleteReply {
        status: outcome.status,
    })
}

// ---------------------------------------------------------------------------
// M9 — admin surfaces (REQ-058)
// ---------------------------------------------------------------------------

/// `admin.members.list` — the members of the caller's space (REQ-058).
async fn admin_members_list(
    state: AppState,
    auth: AuthContext,
) -> Result<AdminMembersReply, ApiError> {
    let outcome = usecases::admin_members_list(state.app(), &auth.actor.context).await?;
    Ok(AdminMembersReply {
        members: outcome
            .members
            .iter()
            .map(|member| AdminMemberView {
                user_id: member.user_id.as_str().to_owned(),
                email: member.email.as_str().to_owned(),
                name: member.name.clone(),
                role: member.role.as_str().to_owned(),
                last_seen_at: member.last_seen_at.clone(),
            })
            .collect(),
    })
}

/// `admin.invitations.list` — the pending invitations of the caller's space (REQ-058).
async fn admin_invitations_list(
    state: AppState,
    auth: AuthContext,
) -> Result<AdminInvitationsReply, ApiError> {
    let outcome = usecases::admin_invitations_list(state.app(), &auth.actor.context).await?;
    Ok(AdminInvitationsReply {
        invitations: outcome
            .invitations
            .iter()
            .map(|invitation| AdminInvitationView {
                id: invitation.id.clone(),
                email: invitation.email.clone(),
                role: invitation.role.as_str().to_owned(),
                expires_at: invitation.expires_at.clone(),
            })
            .collect(),
    })
}

/// `admin.invitations.revoke` — kills one pending invitation (REQ-058).
async fn admin_invitations_revoke(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AdminInvitationRevokeReply, ApiError> {
    let request: AdminInvitationRevokeRequest = parse(&body)?;
    let revoked = usecases::admin_invitations_revoke(
        state.app(),
        &auth.actor.context,
        &request.invitation_id,
    )
    .await?;
    if !revoked {
        return Err(ApiError::not_found());
    }
    Ok(AdminInvitationRevokeReply { revoked: true })
}

/// `admin.grants.list` — every grant on the deployment (REQ-058).
async fn admin_grants_list(
    state: AppState,
    auth: AuthContext,
) -> Result<AdminGrantsReply, ApiError> {
    let outcome = usecases::admin_grants_list(state.app(), &auth.actor.context).await?;
    Ok(AdminGrantsReply {
        grants: outcome
            .grants
            .iter()
            .map(|grant| AdminGrantView {
                email: grant.email.clone(),
                reason: grant.reason.clone(),
                granted_by: grant.granted_by.clone(),
                created_at: grant.created_at.clone(),
            })
            .collect(),
    })
}

/// `admin.grants.create` — one server-level grant (REQ-058).
async fn admin_grants_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AdminGrantCreateReply, ApiError> {
    let request: AdminGrantCreateRequest = parse(&body)?;
    let grant = usecases::admin_grants_create(
        state.app(),
        &auth.actor.context,
        &request.email,
        &request.reason,
    )
    .await?;
    Ok(AdminGrantCreateReply {
        grant: AdminGrantView {
            email: grant.email,
            reason: grant.reason,
            granted_by: grant.granted_by,
            created_at: grant.created_at,
        },
    })
}

/// `admin.grants.revoke` — removes one grant by email (REQ-058).
async fn admin_grants_revoke(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AdminGrantRevokeReply, ApiError> {
    let request: AdminGrantRevokeRequest = parse(&body)?;
    let revoked =
        usecases::admin_grants_revoke(state.app(), &auth.actor.context, &request.email).await?;
    Ok(AdminGrantRevokeReply { revoked })
}

/// `admin.plugins.list` — the plugin-submission review queue (REQ-058).
async fn admin_plugins_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AdminPluginsReply, ApiError> {
    let request: AdminPluginsListRequest = parse(&body)?;
    let outcome =
        usecases::admin_plugins_list(state.app(), &auth.actor.context, request.status.as_deref())
            .await?;
    Ok(AdminPluginsReply {
        submissions: outcome
            .submissions
            .iter()
            .map(|submission| AdminPluginSubmissionView {
                id: submission.id.clone(),
                source: submission.source.clone(),
                name: submission.name.clone(),
                status: submission.status.clone(),
                created_at: submission.created_at.clone(),
            })
            .collect(),
    })
}

/// `admin.plugins.decide` — the reviewer's approve or reject (REQ-058).
async fn admin_plugins_decide(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AdminPluginDecideReply, ApiError> {
    let request: AdminPluginDecideRequest = parse(&body)?;
    usecases::admin_plugins_decide(
        state.app(),
        &auth.actor.context,
        &request.submission_id,
        &request.decision,
    )
    .await?;
    Ok(AdminPluginDecideReply {
        status: request.decision,
    })
}

// ---------------------------------------------------------------------------
// M10 — invitations (REQ-042)
// ---------------------------------------------------------------------------

/// `invitations.accept` — joins the space the token names (REQ-042).
///
/// A used, expired or malformed token answers the same refusal it always did; the invitee signs
/// in first, the link binds the caller's address. An already-member caller is accepted without
/// error, so a re-click shows the room rather than a puzzling refusal.
async fn invitations_accept(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<InvitationAcceptReply, ApiError> {
    let request: InvitationAcceptRequest = parse(&body)?;
    let outcome = usecases::invite_accept(state.app(), &auth.actor.context, &request.token).await?;
    Ok(InvitationAcceptReply {
        space_id: outcome.space_id.as_str().to_owned(),
    })
}

// ---------------------------------------------------------------------------
// M11.1 — groups (REQ-010)
// ---------------------------------------------------------------------------

/// `groups.create` — creates the shared room (REQ-010).
///
/// The golden (`spec/golden/group-create.json`) pins the reply shape: the group's id, the
/// shared thread's id, the member bot ids in the order the caller listed them.
async fn groups_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<GroupsCreateReply, ApiError> {
    let request: GroupsCreateRequest = parse(&body)?;
    let created = usecases::create_group(
        state.app(),
        &auth.actor.context,
        &usecases::GroupsCreate {
            name: request.name,
            bot_ids: request.bot_ids,
        },
    )
    .await?;
    Ok(GroupsCreateReply {
        id: created.id.as_str().to_owned(),
        name: created.name,
        thread_id: created.thread_id.as_str().to_owned(),
        member_bot_ids: created
            .member_bot_ids
            .into_iter()
            .map(|bot| bot.as_str().to_owned())
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// M11.3 — groups, read (REQ-010)
// ---------------------------------------------------------------------------

/// `groups.messages` — a page of the shared thread, attribution included.
///
/// The wire carries the same shape as `threads/messages` with `botId` on bot messages. The
/// member is never attributed; the auth context owns their rows.
async fn groups_messages(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<GroupMessagesReply, ApiError> {
    let request: MessagesRequest = parse(&body)?;
    let page = usecases::group_messages(
        state.app(),
        &auth.actor.context,
        &request.thread_id,
        request.cursor.as_deref(),
        request.limit,
    )
    .await?;

    Ok(GroupMessagesReply {
        messages: page
            .messages
            .iter()
            .map(crate::wire::GroupMessageView::from)
            .collect(),
        next_cursor: page.next_cursor,
    })
}

/// The body of `groups.get` (REQ-010).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GroupsGetRequest {
    /// The group to read.
    id: String,
}

/// `groups.get` — one group as its members see it.
///
/// The wire returns the shared thread id and the member bots with their names — enough for the
/// screen to render the shared room without ever touching another tenant's group (REQ-010,
/// REQ-043).
async fn groups_get(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<GroupGetReply, ApiError> {
    let request: GroupsGetRequest = parse(&body)?;
    let outcome = usecases::get_group(state.app(), &auth.actor.context, &request.id).await?;
    let group = outcome.group;
    Ok(GroupGetReply {
        id: group.id.as_str().to_owned(),
        name: group.name,
        thread_id: group.thread_id.as_str().to_owned(),
        member_bot_ids: group
            .bot_ids
            .iter()
            .map(|bot| bot.as_str().to_owned())
            .collect(),
        member_bots: outcome
            .member_bot_names
            .into_iter()
            .map(|(id, name)| GroupBotView {
                id: id.as_str().to_owned(),
                name,
            })
            .collect(),
        created_at: group.created_at,
    })
}

// ---------------------------------------------------------------------------
// M11.3 — groups, list (REQ-010)
// ---------------------------------------------------------------------------

/// `groups.list` — every group the caller belongs to, oldest first.
///
/// The wire carries the shared thread handle and the bot-count so the rail can render a row
/// per group without a second round-trip.
async fn groups_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<GroupsListReply, ApiError> {
    let _: serde_json::Value = parse(&body)?;
    let groups = usecases::list_groups(state.app(), &auth.actor.context).await?;
    Ok(GroupsListReply {
        groups: groups
            .into_iter()
            .map(|group| GroupListView {
                id: group.id.as_str().to_owned(),
                name: group.name,
                thread_id: group.thread_id.as_str().to_owned(),
                owner_bot_id: group.bot_ids[0].as_str().to_owned(),
                member_count: group.bot_ids.len().try_into().unwrap_or(u32::MAX),
                created_at: group.created_at,
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// M12.1 — external agents (REQ-059)
// ---------------------------------------------------------------------------

/// The body of `bots.connectExternal`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotsConnectExternalRequest {
    /// The bot to connect.
    bot_id: String,
    /// The executor base URL (https, or http on loopback).
    base_url: String,
    /// Which agent the executor should run, when it exposes several.
    #[serde(default)]
    agent_name: Option<String>,
    /// The space's bot-secret to use as the executor's credential.
    #[serde(default)]
    secret_name: Option<String>,
}

/// `bots.connectExternal` — point a bot at an external agent executor (REQ-059).
async fn bots_connect_external(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ExternalAgentConnectedReply, ApiError> {
    let request: BotsConnectExternalRequest = parse(&body)?;
    let outcome = usecases::connect_external_agent(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.base_url,
        request.agent_name.as_deref(),
        request.secret_name.as_deref(),
    )
    .await?;
    Ok(ExternalAgentConnectedReply {
        bot_id: outcome.bot_id.as_str().to_owned(),
        base_url: outcome.base_url,
        agent_name: outcome.agent_name,
        secret_name: outcome.secret_name,
        connected_at: outcome.connected_at,
    })
}

/// The body of `bots.disconnectExternal`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotsDisconnectExternalRequest {
    /// The bot to disconnect.
    bot_id: String,
}

/// `bots.disconnectExternal` — the bot falls back to the deployment's internal provider.
async fn bots_disconnect_external(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ExternalAgentDisconnectedReply, ApiError> {
    let request: BotsDisconnectExternalRequest = parse(&body)?;
    usecases::disconnect_external_agent(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(ExternalAgentDisconnectedReply {
        bot_id: request.bot_id,
        disconnected: true,
    })
}

/// The body of `bots.connectAcp`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotsConnectAcpRequest {
    /// The bot that gets a local executor.
    bot_id: String,
    /// The known executor id (this build only knows `opencode`).
    executor: String,
}

/// `bots.connectAcp` — the bot attaches to a local ACP executor (REQ-061).
async fn bots_connect_acp(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AcpAgentConnectedReply, ApiError> {
    let request: BotsConnectAcpRequest = parse(&body)?;
    let outcome = usecases::attach_acp_agent(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.executor,
    )
    .await?;
    Ok(AcpAgentConnectedReply {
        bot_id: outcome.bot_id.as_str().to_owned(),
        executor: outcome.executor,
        attached_at: outcome.attached_at,
    })
}

/// The body of `bots.disconnectAcp`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotsDisconnectAcpRequest {
    /// The bot whose executor attachment goes away.
    bot_id: String,
}

/// `bots.disconnectAcp` — the bot falls back to the deployment's internal provider.
async fn bots_disconnect_acp(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AcpAgentDisconnectedReply, ApiError> {
    let request: BotsDisconnectAcpRequest = parse(&body)?;
    usecases::detach_acp_agent(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(AcpAgentDisconnectedReply {
        bot_id: request.bot_id,
        detached: true,
    })
}

// ---------------------------------------------------------------------------
// M13.1 — sign-in requests (REQ-060)
// ---------------------------------------------------------------------------

async fn bots_sign_ins(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<BotsSignInsReply, ApiError> {
    let _: serde_json::Value = parse(&body)?;
    let requests = usecases::list_signin_requests(state.app(), &auth.actor.context, None).await?;
    Ok(BotsSignInsReply {
        requests: requests
            .into_iter()
            .map(|request| SignInRequestView {
                id: request.id,
                bot_id: request.bot_id.as_str().to_owned(),
                origin: request.origin,
                display_name: request.display_name,
                status: match request.status {
                    boma_domain::signin::SignInRequestStatus::Pending => "pending".to_string(),
                    boma_domain::signin::SignInRequestStatus::Granted => "granted".to_string(),
                    boma_domain::signin::SignInRequestStatus::Refused => "refused".to_string(),
                },
                created_at: request.created_at,
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// M12.2 — external agent status
// ---------------------------------------------------------------------------

/// The body of `bots.externalAgentStatus`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotsExternalAgentStatusRequest {
    /// The bot to ask about.
    bot_id: String,
}

/// `bots.externalAgentStatus` — whether the bot points at an external executor, and if so, which.
///
/// The reply carries the connection state and the executor's address; the secret never appears.
async fn bots_external_agent_status(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ExternalAgentStatusReply, ApiError> {
    let request: BotsExternalAgentStatusRequest = parse(&body)?;
    let outcome =
        usecases::external_agent_status(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(ExternalAgentStatusReply {
        connected: outcome.connected,
        base_url: outcome.base_url,
        agent_name: outcome.agent_name,
        secret_name: outcome.secret_name,
        connected_at: outcome.connected_at,
    })
}

// ---------------------------------------------------------------------------
// M15.1 — notifications (REQ-064)
// ---------------------------------------------------------------------------

/// The body of `notifications.status`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotificationsStatusBody {}

/// The reply of `notifications.status`.
///
/// The public key never leaves the server without the feature being enabled; a push-disabled
/// deployment advertises exactly that, nothing else.
#[allow(clippy::unused_async)]
async fn notifications_status(
    state: AppState,
    _auth: AuthContext,
    body: Bytes,
) -> Result<NotificationsStatusReply, ApiError> {
    let _: NotificationsStatusBody = parse(&body)?;
    let public_key = state
        .app()
        .services
        .vapid
        .as_ref()
        .map(|vapid| {
            vapid
                .public_key_base64url()
                .map_err(|_| ApiError::new(ErrorCode::Internal, "the push configuration is broken"))
        })
        .transpose()?;
    Ok(NotificationsStatusReply {
        enabled: state.app().services.vapid.is_some(),
        vapid_public_key: public_key,
    })
}

/// The body of `notifications.subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotificationsSubscribeBody {
    /// The push service endpoint the member's browser minted.
    endpoint: String,
    /// The client public key, base64url.
    p256dh: String,
    /// The client auth secret, base64url.
    auth: String,
    /// The browser, for display only.
    #[serde(default)]
    user_agent: Option<String>,
}

/// `notifications.subscribe` — registers a browser subscription for Web Push.
///
/// Refuses when VAPID is not configured, when the endpoint is not https-bare (no credentials,
/// no metadata host), or when the DNS-resolving checker refuses it.
async fn notifications_subscribe(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<NotificationsSubscribeReply, ApiError> {
    let request: NotificationsSubscribeBody = parse(&body)?;
    if state.app().services.vapid.is_none() {
        return Err(ApiError::new(
            ErrorCode::BadRequest,
            "push is not configured on this deployment",
        ));
    }
    let endpoint = request.endpoint.trim().to_owned();
    let parsed = url::Url::parse(&endpoint)
        .map_err(|_| ApiError::new(ErrorCode::BadRequest, "the endpoint is not a URL"))?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(ApiError::new(
            ErrorCode::BadRequest,
            "the endpoint must be an https URL",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ApiError::new(
            ErrorCode::BadRequest,
            "the endpoint carries no credentials",
        ));
    }
    if let Some(checker) = state.app().services.endpoint_safety.as_ref() {
        checker
            .check(&endpoint)
            .await
            .map_err(|error| match error {
                boma_app::AppError::Rule(_) => {
                    ApiError::new(ErrorCode::BadRequest, "the endpoint address is refused")
                }
                _ => ApiError::new(ErrorCode::BadRequest, "the endpoint address is refused"),
            })?;
    }
    if request.p256dh.trim().is_empty() || request.auth.trim().is_empty() {
        return Err(ApiError::new(
            ErrorCode::BadRequest,
            "the subscription keys are required",
        ));
    }
    let user_id = auth.actor.context.user_id().cloned().ok_or(ApiError::new(
        ErrorCode::Forbidden,
        "a subscription needs a member",
    ))?;
    let space_id = auth.actor.context.space_id().cloned().ok_or(ApiError::new(
        ErrorCode::BadRequest,
        "a subscription needs a space",
    ))?;
    let sub_id = state.app().services.ids.new_id("sub");
    state
        .app()
        .stores
        .notifications
        .subscribe(boma_domain::notification::NewNotificationSubscription {
            id: sub_id.clone(),
            space_id,
            user_id,
            endpoint,
            p256dh: request.p256dh,
            auth: request.auth,
            user_agent: request.user_agent,
            created_at: state.app().services.clock.now(),
        })
        .await?;
    Ok(NotificationsSubscribeReply { id: sub_id })
}

/// The body of `notifications.unsubscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotificationsUnsubscribeBody {
    /// The push service endpoint to drop.
    endpoint: String,
}

/// `notifications.unsubscribe` — drops a browser subscription. Idempotent.
async fn notifications_unsubscribe(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<NotificationsUnsubscribeReply, ApiError> {
    let request: NotificationsUnsubscribeBody = parse(&body)?;
    if request.endpoint.trim().is_empty() {
        return Err(ApiError::new(
            ErrorCode::BadRequest,
            "the endpoint is required",
        ));
    }
    let removed = state
        .app()
        .stores
        .notifications
        .unsubscribe(&auth.actor.context, request.endpoint.trim())
        .await?;
    Ok(NotificationsUnsubscribeReply { removed })
}

/// `notifications.policyGet` — the per-bot policy for this member.
async fn notifications_policy_get(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<NotificationsPolicyView, ApiError> {
    let request: NotificationsPolicyGetRequest = parse(&body)?;
    let bot_id = boma_domain::BotId::new(request.bot_id).map_err(|_| ApiError::not_found())?;
    check_bot_live(&state, &auth, &bot_id).await?;
    let policy = state
        .app()
        .stores
        .notifications
        .get_policy(&auth.actor.context, &bot_id)
        .await?;
    Ok(policy_view(&policy))
}

/// `notifications.policySet` — the same, with a patch of the members' choice.
///
/// `dailyCap` must be at most 10000; zero would cap someone into never being told again with a
/// silent success.
async fn notifications_policy_set(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<NotificationsPolicyView, ApiError> {
    let request: NotificationsPolicySetRequest = parse(&body)?;
    if request.daily_cap.is_some_and(|value| value > 10_000) {
        return Err(ApiError::new(
            ErrorCode::BadRequest,
            "the daily cap is unreasonably large",
        ));
    }
    let bot_id = boma_domain::BotId::new(request.bot_id).map_err(|_| ApiError::not_found())?;
    check_bot_live(&state, &auth, &bot_id).await?;
    let policy = state
        .app()
        .stores
        .notifications
        .set_policy(
            &auth.actor.context,
            boma_domain::notification::SetNotificationPolicy {
                bot_id,
                notify_waiting: request.notify_waiting,
                notify_completed: request.notify_completed,
                daily_cap: request.daily_cap,
                destination_slack_webhook: request.destination_slack_webhook,
                destination_teams_webhook: request.destination_teams_webhook,
            },
        )
        .await?;
    Ok(policy_view(&policy))
}

fn policy_view(policy: &boma_domain::notification::NotificationPolicy) -> NotificationsPolicyView {
    NotificationsPolicyView {
        bot_id: policy.bot_id.as_str().to_owned(),
        notify_waiting: policy.notify_waiting,
        notify_completed: policy.notify_completed,
        daily_cap: policy.daily_cap,
        destination_slack_webhook: policy.destination_slack_webhook,
        destination_teams_webhook: policy.destination_teams_webhook,
    }
}

/// Refetches the bot to refuse a missing or archived one with the same shape the other
/// bot-scoped RPCs use (REQ-001).
async fn check_bot_live(
    state: &AppState,
    auth: &AuthContext,
    bot_id: &boma_domain::BotId,
) -> Result<(), ApiError> {
    let bot = state
        .app()
        .stores
        .bots
        .find(&auth.actor.context, bot_id)
        .await
        .map_err(|_| ApiError::new(ErrorCode::Internal, "the bot could not be read"))?
        .ok_or(ApiError::not_found())?;
    if bot.is_archived() {
        return Err(ApiError::not_found());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// M15.2 — destinations
// ---------------------------------------------------------------------------

/// The body of `notifications.destinationsList`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotificationsDestinationsListBody {}

/// `notifications.destinationsList` — the space's channels, labels only.
async fn notifications_destinations_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<NotificationsDestinationsListReply, ApiError> {
    let _: NotificationsDestinationsListBody = parse(&body)?;
    let destinations = usecases::destinations_list(state.app(), &auth.actor.context, None).await?;
    Ok(NotificationsDestinationsListReply {
        destinations: destinations.into_iter().map(destination_view).collect(),
    })
}

/// The body of `notifications.destinationsCreate` — Owner/Admin only.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotificationsDestinationsCreateBody {
    /// slack_webhook | teams_webhook
    kind: String,
    /// A human label (`#product-releases`).
    label: String,
    /// The incoming-webhook URL; sealed on write and never returned again.
    webhook: String,
}

/// `notifications.destinationsCreate` — register a webhook; Owner/Admin only.
async fn notifications_destinations_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<NotificationsDestinationsCreateReply, ApiError> {
    let request: NotificationsDestinationsCreateBody = parse(&body)?;
    let kind = match request.kind.as_str() {
        "slack_webhook" => boma_domain::notification::NotificationDestinationKind::SlackWebhook,
        "teams_webhook" => boma_domain::notification::NotificationDestinationKind::TeamsWebhook,
        _ => {
            return Err(ApiError::new(
                ErrorCode::BadRequest,
                "the kind must be slack_webhook or teams_webhook",
            ));
        }
    };
    let outcome = usecases::destinations_create(
        state.app(),
        &auth.actor.context,
        kind,
        &request.label,
        &request.webhook,
    )
    .await
    .map_err(|error| match error {
        boma_app::AppError::Rule(_) => {
            ApiError::new(ErrorCode::BadRequest, "the webhook is refused")
        }
        boma_app::AppError::Forbidden => ApiError::new(
            ErrorCode::Forbidden,
            "an owner or admin can register a destination",
        ),
        other => ApiError::from(other),
    })?;
    Ok(NotificationsDestinationsCreateReply {
        destination: destination_view(outcome.destination),
    })
}

/// The body of `notifications.destinationsDelete` — Owner/Admin only.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NotificationsDestinationsDeleteBody {
    /// The destination to remove.
    id: String,
}

/// `notifications.destinationsDelete` — remove a destination; Owner/Admin only.
async fn notifications_destinations_delete(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<NotificationsDestinationsDeleteReply, ApiError> {
    let request: NotificationsDestinationsDeleteBody = parse(&body)?;
    if request.id.trim().is_empty() {
        return Err(ApiError::new(ErrorCode::BadRequest, "the id is required"));
    }
    let removed =
        usecases::destinations_delete(state.app(), &auth.actor.context, request.id.trim())
            .await
            .map_err(|error| match error {
                boma_app::AppError::Forbidden => ApiError::new(
                    ErrorCode::Forbidden,
                    "an owner or admin can remove a destination",
                ),
                other => ApiError::from(other),
            })?;
    Ok(NotificationsDestinationsDeleteReply { removed })
}

fn destination_view(
    destination: boma_domain::notification::NotificationDestination,
) -> NotificationDestinationView {
    NotificationDestinationView {
        id: destination.id,
        kind: destination.kind.as_str().to_owned(),
        label: destination.label,
        created_at: destination.created_at,
    }
}
