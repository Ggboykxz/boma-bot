//! The connector catalogue (REQ-032).
//!
//! A connector is a named, self-contained integration an installation of this deployment can
//! offer every space. The catalogue is code, not data: what these four tools can do is decided
//! here, reviewably. A space installs from the catalogue; bots of that space see the installed
//! connectors' tools (a bot's tool grant may exclude them); revoking the install removes the
//! tools everywhere and every later call is refused.
//!
//! Two entries ship, both self-contained (no external service, no keys needed for `rss`):
//!
//! * `rss` — reads a public RSS 2.0 or Atom feed and answers with its entries' titles, links and
//!   summaries. No credential. Deliberately minimal parsing: it is a reader, not an indexer.
//! * `http-json` — a guarded GET of a JSON or text endpoint with the installation's bearer
//!   credential, for integrations that already exist behind an API key.
//!
//! What does NOT ship yet: OAuth-based connectors, webhook-push connectors, the remote
//! marketplace of community connectors. Each arrives with the milestone that executes it.

use serde_json::Value;

use crate::DomainError;

/// How a connector authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorAuth {
    /// No credential; anyone in the space may read what the connector can read.
    None,
    /// One bearer API key, sealed per installation.
    ApiKey,
}

impl ConnectorAuth {
    /// The wire value stored in the `auth_kind` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ApiKey => "api_key",
        }
    }
}

/// One tool a connector exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorToolSpec {
    /// The tool's name as the catalogue key says it (`connector.<id>.<name>`).
    pub name: String,
    /// One line about what the tool does.
    pub description: String,
    /// The JSON schema of its arguments.
    pub input_schema: Value,
}

/// One catalogue entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorSpec {
    /// The catalogue id: lowercase-kebab, part of every tool key.
    pub id: String,
    /// The member's display name.
    pub name: String,
    /// One line about what installing it gives.
    pub description: String,
    /// How it authenticates.
    pub auth_kind: ConnectorAuth,
    /// The tools an installation exposes.
    pub tools: Vec<ConnectorToolSpec>,
}

/// The longest connector id accepted.
const CONNECTOR_ID_MAX: usize = 64;

/// Checks a connector id: lowercase-kebab, no double dash.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the id is refused.
pub fn validate_connector_id(id: &str) -> Result<(), DomainError> {
    let valid = !id.is_empty()
        && id.len() <= CONNECTOR_ID_MAX
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--");
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidName(format!(
            "the connector id `{id}` must be lowercase-kebab (a-z, 0-9, `-`)"
        )))
    }
}

/// The catalogue this deployment offers every space.
#[must_use]
pub fn builtin_connectors() -> Vec<ConnectorSpec> {
    vec![
        ConnectorSpec {
            id: "rss".to_owned(),
            name: "RSS / Atom reader".to_owned(),
            description: "Read public RSS 2.0 and Atom feeds: titles, links and summaries.".to_owned(),
            auth_kind: ConnectorAuth::None,
            tools: vec![ConnectorToolSpec {
                name: "read".to_owned(),
                description: "Read the entries of one public feed, newest first.".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "The HTTPS URL of the feed." },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 20 }
                    },
                    "required": ["url"],
                    "additionalProperties": false
                }),
            }],
        },
        ConnectorSpec {
            id: "http-json".to_owned(),
            name: "HTTP JSON endpoint".to_owned(),
            description:
                "GET any JSON or text endpoint with the installation's bearer key, responses capped."
                    .to_owned(),
            auth_kind: ConnectorAuth::ApiKey,
            tools: vec![ConnectorToolSpec {
                name: "get".to_owned(),
                description: "GET a URL with the installation's credential; answers up to 16 KiB.".to_owned(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "The HTTPS URL to GET." }
                    },
                    "required": ["url"],
                    "additionalProperties": false
                }),
            }],
        },
    ]
}

impl ConnectorSpec {
    /// The tool key a bot calls: `connector.<id>.<name>`.
    #[must_use]
    pub fn tool_key(&self, tool: &ConnectorToolSpec) -> String {
        format!("connector.{}.{}", self.id, tool.name)
    }
}

#[cfg(test)]
mod tests {
    use super::{builtin_connectors, validate_connector_id};

    #[test]
    fn req_032_the_builtin_catalogue_is_well_formed_and_unique() {
        let connectors = builtin_connectors();
        assert_eq!(connectors.len(), 2);
        let ids: Vec<&str> = connectors
            .iter()
            .map(|connector| connector.id.as_str())
            .collect();
        assert_eq!(ids, ["rss", "http-json"]);
        for connector in &connectors {
            validate_connector_id(&connector.id).expect("the builtin id is valid");
            assert!(
                !connector.tools.is_empty(),
                "every connector exposes at least one tool"
            );
        }
    }

    #[test]
    fn req_032_a_valid_connector_id_is_accepted() {
        assert!(validate_connector_id("rss").is_ok());
        assert!(validate_connector_id("http-json").is_ok());
    }

    #[test]
    fn req_032_an_invalid_connector_id_is_refused() {
        for id in [
            "",
            "Upper",
            "has space",
            "has_underscore",
            "-a",
            "a-",
            "double--dash",
            "dot.name",
        ] {
            assert!(validate_connector_id(id).is_err(), "{id}");
        }
    }
}
