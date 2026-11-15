//! Audit events (defect D-05, M9 for the rest).
//!
//! One kind today: a private MCP endpoint was called. Kept tiny on purpose — every future
//! escape hatch adds a variant here, never a free-text kind.

/// Why an audit row exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    /// A private MCP endpoint was reached (D-05).
    McpPrivateCall,
    /// An unverified sender's mail was dropped by policy (REQ-047).
    MailDropped,
    /// A private endpoint was reached for an installation (REQ-048, D-05).
    MessagingPrivateEndpoint,
    /// A password sign-in attempt failed (REQ-045).
    SignInFailed,
    /// A browser reported a CSP violation (REQ-045, REQ-054).
    CspViolation,
    /// The account was deleted (REQ-044, REQ-045).
    AccountDeleted,
    /// An account archive was downloaded (REQ-044).
    ExportDownloaded,
    /// An account archive was imported (REQ-044).
    ImportCompleted,
    /// A SCIM-provisioned account was created (REQ-045).
    ScimProvisioned,
    /// A SCIM-deprovisioned account was disabled (REQ-045).
    ScimDeprovisioned,
    /// An admin changed a member's role in a space (REQ-058).
    AdminMemberRoleChanged,
    /// An admin removed a member from a space (REQ-058).
    AdminMemberRemoved,
    /// An admin created an access grant (REQ-058).
    AdminGrantCreated,
    /// An admin revoked an access grant (REQ-058).
    AdminGrantRevoked,
    /// An admin reviewed a plugin submission (REQ-058).
    AdminPluginReviewed,
}

impl AuditKind {
    /// The wire value stored in the `kind` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::McpPrivateCall => "mcp.private_call",
            Self::MailDropped => "mail.dropped",
            Self::MessagingPrivateEndpoint => "messaging.private_endpoint",
            Self::SignInFailed => "security.sign_in_failed",
            Self::CspViolation => "security.csp_violation",
            Self::AccountDeleted => "account.deleted",
            Self::ExportDownloaded => "export.downloaded",
            Self::ImportCompleted => "import.completed",
            Self::ScimProvisioned => "scim.provisioned",
            Self::ScimDeprovisioned => "scim.deprovisioned",
            Self::AdminMemberRoleChanged => "admin.member_role_changed",
            Self::AdminMemberRemoved => "admin.member_removed",
            Self::AdminGrantCreated => "admin.grant_created",
            Self::AdminGrantRevoked => "admin.grant_revoked",
            Self::AdminPluginReviewed => "admin.plugin_reviewed",
        }
    }
}
