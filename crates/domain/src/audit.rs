//! Audit events (defect D-05, M9 for the rest).
//!
//! One kind today: a private MCP endpoint was called. Kept tiny on purpose — every future
//! escape hatch adds a variant here, never a free-text kind.

/// Why an audit row exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    /// A private MCP endpoint was reached (D-05).
    McpPrivateCall,
}

impl AuditKind {
    /// The wire value stored in the `kind` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::McpPrivateCall => "mcp.private_call",
        }
    }
}
