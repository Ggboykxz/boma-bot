//! MCP servers (REQ-033).
//!
//! One server is one named endpoint per space. Transport is HTTPS (streamable HTTP) or stdio
//! (a local command the deployment allow-lists). What a server does with the tools it names is
//! its own business; what this module decides is who may name what: server names are
//! lowercase-kebab, tool names are sanitised, and the stdio decision is binary.

use std::{fmt, str::FromStr};

use crate::DomainError;

/// How an MCP server talks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransport {
    /// Streamable HTTP over HTTPS (the remote path).
    Http,
    /// A local command spawned with a minimal environment (allow-listed only).
    Stdio,
}

impl McpTransport {
    /// The wire value stored in the `transport` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Stdio => "stdio",
        }
    }
}

impl fmt::Display for McpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for McpTransport {
    type Err = DomainError;

    /// Parses a transport from the wire.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidName`] when the value is neither `http` nor `stdio`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "http" => Ok(Self::Http),
            "stdio" => Ok(Self::Stdio),
            other => Err(DomainError::InvalidName(format!(
                "the MCP transport `{other}` is not `http` or `stdio`"
            ))),
        }
    }
}

/// The longest server name accepted.
const SERVER_NAME_MAX: usize = 48;

/// Checks an MCP server name: lowercase-kebab, no dots (dots would collide with tool-key paths).
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the name is refused.
pub fn validate_mcp_server_name(name: &str) -> Result<(), DomainError> {
    let valid = !name.is_empty()
        && name.len() <= SERVER_NAME_MAX
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidName(format!(
            "the MCP server name `{name}` must be lowercase-kebab (a-z, 0-9, `-`)"
        )))
    }
}

/// The longest MCP tool key accepted.
const TOOL_KEY_MAX: usize = 64;

/// Sanitises a tool name coming from a server, or refuses it.
///
/// A malicious or sloppy server must not break the catalogue grammar, so characters outside
/// `[A-Za-z0-9_.-]` are refused outright rather than rewritten.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the tool name is empty or carries an unsafe byte.
pub fn sanitize_mcp_tool_name(name: &str) -> Result<String, DomainError> {
    let valid = !name.is_empty()
        && name.len() <= TOOL_KEY_MAX
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..");
    if valid {
        Ok(name.to_owned())
    } else {
        Err(DomainError::InvalidName(format!(
            "the MCP tool name `{name}` is refused: letters, digits, `_`, `-`, `.` only"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{McpTransport, sanitize_mcp_tool_name, validate_mcp_server_name};

    #[test]
    fn req_033_a_valid_server_name_is_accepted() {
        assert!(validate_mcp_server_name("github").is_ok());
        assert!(validate_mcp_server_name("my-server-2").is_ok());
    }

    #[test]
    fn req_033_an_invalid_server_name_is_refused() {
        for name in [
            "",
            "Upper",
            "has space",
            "has_underscore",
            "-lead",
            "trail-",
            "double--dash",
            "dot.name",
        ] {
            assert!(validate_mcp_server_name(name).is_err(), "{name}");
        }
    }

    #[test]
    fn req_033_a_transport_round_trips() {
        assert_eq!(McpTransport::Http.as_str(), "http");
        assert_eq!(McpTransport::Stdio.as_str(), "stdio");
        assert_eq!(
            "http".parse::<McpTransport>().expect("parses"),
            McpTransport::Http
        );
        assert!("grpc".parse::<McpTransport>().is_err());
    }

    #[test]
    fn req_033_a_tool_name_is_sanitised() {
        assert_eq!(
            sanitize_mcp_tool_name("list-repos").expect("ok"),
            "list-repos"
        );
        assert_eq!(
            sanitize_mcp_tool_name("issues.list").expect("ok"),
            "issues.list"
        );
        assert!(sanitize_mcp_tool_name("../etc/passwd").is_err());
        assert!(sanitize_mcp_tool_name("x").is_ok());
        assert!(sanitize_mcp_tool_name("has space").is_err());
        assert!(sanitize_mcp_tool_name("").is_err());
    }
}

/// Validates an MCP server URL at registration: shape only, never a network call.
///
/// `allow_private` is what releases plain HTTP (for a private LAN service); HTTPS is required
/// otherwise. Credentials, the metadata endpoints and an unparseable value are refused outright.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the URL is refused.
pub fn validate_mcp_server_url(url: &str, allow_private: bool) -> Result<(), DomainError> {
    let parsed = url::Url::parse(url.trim())
        .map_err(|_| DomainError::InvalidName("the URL does not parse".to_owned()))?;
    let scheme = parsed.scheme();
    if scheme != "https" && !(scheme == "http" && allow_private) {
        return Err(DomainError::InvalidName(
            "the URL must use HTTPS".to_owned(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(DomainError::InvalidName(
            "the URL must not contain credentials".to_owned(),
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| DomainError::InvalidName("the URL has no host".to_owned()))?;
    if [
        "169.254.169.254",
        "metadata.google.internal",
        "metadata",
        "fd00:ec2::254",
    ]
    .contains(&host)
    {
        return Err(DomainError::InvalidName(
            "the metadata endpoint is never an MCP server".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod url_tests {
    use super::validate_mcp_server_url;

    #[test]
    fn req_033_a_valid_url_is_accepted() {
        assert!(validate_mcp_server_url("https://mcp.example.com/sse", false).is_ok());
        assert!(validate_mcp_server_url("http://192.168.1.10:8080/mcp", true).is_ok());
    }

    #[test]
    fn req_033_an_unsafe_url_is_refused() {
        assert!(validate_mcp_server_url("http://mcp.example.com/sse", false).is_err());
        assert!(validate_mcp_server_url("https://user:pass@mcp.example.com/", false).is_err());
        assert!(validate_mcp_server_url("https://169.254.169.254/", false).is_err());
        assert!(validate_mcp_server_url("https://169.254.169.254/", true).is_err());
        assert!(validate_mcp_server_url("not a url", false).is_err());
    }
}
