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
    /// A password recovery link was issued (REQ-040).
    PasswordRecoveryIssued,
    /// A password recovery link was consumed and the password changed (REQ-040).
    PasswordRecoveryCompleted,
    /// An invitee accepted the invitation and joined the space (REQ-042).
    InviteAccepted,
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
            Self::PasswordRecoveryIssued => "security.password_recovery_issued",
            Self::PasswordRecoveryCompleted => "security.password_recovery_completed",
            Self::InviteAccepted => "security.invite_accepted",
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

/// Keys that look like a secret to an automated reviewer.
///
/// The export path replaces any string value matching one of these substrings with `[redacted]`,
/// so an account archive does not carry a live credential even when one was logged. Deliberately
/// conservative strings; a broader match would blank descriptions and test fixtures.
const SECRET_TOKENS: &[&str] = &[
    "sk-", "ghp_", "gho_", "xoxa-", "xoxb-", "xoxr-", "xoxs-", "whsec_", "AKIA", "AIza", "Bearer ",
];

/// Placeholder written in place of a redacted secret.
pub const REDACTED: &str = "[redacted]";

/// Whether a value contains one of the known secret key shapes.
#[must_use]
pub fn looks_like_secret(value: &str) -> bool {
    SECRET_TOKENS.iter().any(|token| value.contains(token))
}

/// Recursively replaces secret-shaped strings with `[redacted]`.
///
/// Objects are visited in key order, which makes the output deterministic for goldens. Arrays
/// and nested objects are walked too. Numbers and booleans pass through untouched.
#[must_use]
pub fn redact_audit_value(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::{Map, Value};
    match value {
        Value::String(text) => {
            if looks_like_secret(text) {
                Value::String(REDACTED.to_owned())
            } else {
                value.clone()
            }
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_audit_value).collect()),
        Value::Object(fields) => {
            let mut out = Map::with_capacity(fields.len());
            for (key, inner) in fields {
                out.insert(key.clone(), redact_audit_value(inner));
            }
            Value::Object(out)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::{REDACTED, redact_audit_value};

    #[test]
    fn req_045_redaction_walks_objects_and_arrays() {
        let value = serde_json::json!({
            "token": "sk-live-123456789012",
            "nested": {
                "headers": ["Bearer abcdef", "just a string", 42, true],
            },
            "n": 9000,
        });
        let redacted = redact_audit_value(&value);
        assert_eq!(redacted["token"], REDACTED);
        assert_eq!(redacted["nested"]["headers"][0], REDACTED);
        assert_eq!(redacted["nested"]["headers"][1], "just a string");
        assert_eq!(redacted["nested"]["headers"][2], 42);
        assert_eq!(redacted["n"], 9000);
    }

    #[test]
    fn req_045_redaction_leaves_non_secret_shapes_alone() {
        let value = serde_json::json!({
            "body": "the model replied",
            "url": "https://example.com/why_spelled_sk_like_but_not",
        });
        let redacted = redact_audit_value(&value);
        assert_eq!(redacted["url"], value["url"]);
    }
}
