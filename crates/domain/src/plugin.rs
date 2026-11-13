//! Plugin source references (REQ-034).
//!
//! A plugin is installed from a source reference: an HTTPS git URL, optionally pinned to a
//! ref and optionally limited to a subdirectory. Nothing in this module touches the network:
//! it exists so every caller — the public submission endpoint, the marketplace installer and
//! the review path — validates the same grammar before any bytes are fetched.
//!
//! The grammar is deliberately minimal: the submission endpoint accepts any syntactically valid
//! HTTPS source; whether the repository exists is the installer's problem, not the parser's.
//! The golden fixture pins the acceptance of a synthetic single-segment host path
//! (`https://example.test/plugin.git#main`), so an owner/repository shape is not required at
//! submission time.

use std::fmt;

/// Where a plugin lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSourceRef {
    /// The HTTPS git URL, without a trailing slash, without credentials and without a fragment.
    pub git_url: String,
    /// The branch, tag or full SHA the install should resolve, when the ref carries one.
    pub ref_: Option<String>,
    /// The directory inside the repository the plugin lives in, when the ref carries one.
    pub subdir: Option<String>,
}

impl PluginSourceRef {
    /// The canonical rendering used as the install key: `url`, plus `@ref` and `#subdir`.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut rendered = self.git_url.clone();
        if let Some(reference) = &self.ref_ {
            rendered.push('@');
            rendered.push_str(reference);
        }
        if let Some(subdir) = &self.subdir {
            rendered.push('#');
            rendered.push_str(subdir);
        }
        rendered
    }
}

/// Why a source reference was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSourceError {
    /// The value was empty, too long, or not a URL at all.
    NotAUrl,
    /// The URL does not use HTTPS.
    NotHttps,
    /// The URL carries credentials (`user:password@host`).
    WithCredentials,
    /// The URL carries a query string or a custom port.
    WithQueryOrPort,
    /// The path is empty or a segment is unsafe.
    NotOwnerRepo,
    /// The `@ref` is empty or unsafe.
    BadRef,
    /// The `#subdir` is unsafe.
    BadSubdir,
}

impl fmt::Display for PluginSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotAUrl => "Plugin source must be an HTTPS git URL",
            Self::NotHttps => "Plugin source must use HTTPS",
            Self::WithCredentials => "Plugin source must not contain credentials",
            Self::WithQueryOrPort => "Plugin source must not contain a query or custom port",
            Self::NotOwnerRepo => "Plugin source must identify an owner and repository",
            Self::BadRef => "Plugin ref is invalid",
            Self::BadSubdir => "Plugin subdirectory must stay inside the plugin directory",
        };
        f.write_str(message)
    }
}

impl std::error::Error for PluginSourceError {}

/// The longest source reference accepted.
const MAX_SOURCE_LENGTH: usize = 2048;

/// The longest ref accepted.
const MAX_REF_LENGTH: usize = 200;

/// Validates a `safe path` relative path: segments of safe characters, never escaping upwards.
fn safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') {
        return false;
    }
    if ['\\', '%', '#', '?']
        .iter()
        .any(|char| path.contains(*char))
    {
        return false;
    }
    path.split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && !segment.chars().any(char::is_control)
    })
}

/// Parses and validates a plugin source reference.
///
/// Grammar: `https://<host>/<path...>[.git][@ref][#subdir]` with at least one safe path segment
/// (`[A-Za-z0-9-_.]`); a `.git` suffix is dropped. The `@ref` and `#subdir` markers are the same
/// as git's. A trailing slash is tolerated. Anything else — HTTP, credentials, ports, queries,
/// or a ref/subdir with control characters or `..` — is refused with the reason.
///
/// # Errors
///
/// Returns [`PluginSourceError`] with the reason the source was refused.
pub fn parse_plugin_source(source: &str) -> Result<PluginSourceRef, PluginSourceError> {
    let input = source.trim();
    if input.is_empty() || input.len() > MAX_SOURCE_LENGTH {
        return Err(PluginSourceError::NotAUrl);
    }
    let parsed = url::Url::parse(input).map_err(|_| PluginSourceError::NotAUrl)?;
    if parsed.scheme() != "https" {
        return Err(PluginSourceError::NotHttps);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(PluginSourceError::WithCredentials);
    }
    if parsed.query().is_some() || parsed.port().is_some() {
        return Err(PluginSourceError::WithQueryOrPort);
    }
    let subdir = parsed
        .fragment()
        .map(str::to_owned)
        .filter(|s| !s.is_empty());
    let mut path = parsed.path().trim_end_matches('/').to_owned();
    // `@ref` rides inside the path, after the repository segment.
    let mut ref_ = None;
    if let Some(at) = path.find('@') {
        ref_ = Some(path[at + 1..].to_owned());
        path = path[..at].to_owned();
    }
    if let Some(suffix) = path.strip_suffix(".git") {
        path = suffix.to_owned();
    }
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let safe = !segments.is_empty()
        && segments.iter().all(|segment| {
            segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                && *segment != "."
                && *segment != ".."
        });
    if !safe {
        return Err(PluginSourceError::NotOwnerRepo);
    }
    if let Some(reference) = &ref_ {
        let valid = !reference.is_empty()
            && reference.len() <= MAX_REF_LENGTH
            && !reference.contains("..")
            && !reference.chars().any(|c| {
                matches!(c, '%' | '#' | '?' | '\\') || c.is_whitespace() || c.is_control()
            });
        if !valid {
            return Err(PluginSourceError::BadRef);
        }
    }
    if let Some(subdir) = &subdir
        && !safe_relative_path(subdir)
    {
        return Err(PluginSourceError::BadSubdir);
    }
    let path = format!("/{}", segments.join("/"));
    let mut cleaned = parsed.clone();
    cleaned.set_fragment(None);
    cleaned.set_path(&path);
    Ok(PluginSourceRef {
        git_url: cleaned.to_string().trim_end_matches('/').to_owned(),
        ref_,
        subdir,
    })
}

#[cfg(test)]
mod tests {
    use super::{PluginSourceError, parse_plugin_source};

    #[test]
    fn req_034_a_plain_https_source_parses() {
        let parsed =
            parse_plugin_source("https://example.test/acme/widget.git").expect("the source parses");
        assert_eq!(parsed.git_url, "https://example.test/acme/widget");
        assert_eq!(parsed.ref_, None);
        assert_eq!(parsed.subdir, None);
    }

    #[test]
    fn req_034_a_source_with_a_ref_and_a_subdir_parses() {
        // The fixture pins the acceptance of the fragment form (`#` is the subdir marker).
        let parsed = parse_plugin_source("https://example.test/plugin.git#main")
            .expect("the golden's example parses");
        assert_eq!(parsed.git_url, "https://example.test/plugin");
        assert_eq!(parsed.subdir.as_deref(), Some("main"));

        let parsed = parse_plugin_source("https://example.test/owner/repo@v1.2.3#packages/plugin")
            .expect("the source parses");
        assert_eq!(parsed.git_url, "https://example.test/owner/repo");
        assert_eq!(parsed.ref_.as_deref(), Some("v1.2.3"));
        assert_eq!(parsed.subdir.as_deref(), Some("packages/plugin"));
        assert_eq!(
            parsed.canonical(),
            "https://example.test/owner/repo@v1.2.3#packages/plugin"
        );
    }

    #[test]
    fn req_034_a_source_that_is_not_a_url_is_refused() {
        assert_eq!(
            parse_plugin_source("not a repository").unwrap_err(),
            PluginSourceError::NotAUrl
        );
        assert_eq!(
            parse_plugin_source("").unwrap_err(),
            PluginSourceError::NotAUrl
        );
    }

    #[test]
    fn req_034_a_non_https_source_is_refused() {
        assert_eq!(
            parse_plugin_source("http://example.test/owner/repo").unwrap_err(),
            PluginSourceError::NotHttps
        );
        assert_eq!(
            parse_plugin_source("ssh://git@example.test/owner/repo").unwrap_err(),
            PluginSourceError::NotHttps
        );
    }

    #[test]
    fn req_034_a_source_with_credentials_is_refused() {
        assert_eq!(
            parse_plugin_source("https://user:pass@example.test/owner/repo").unwrap_err(),
            PluginSourceError::WithCredentials
        );
    }

    #[test]
    fn req_034_a_source_with_a_query_or_custom_port_is_refused() {
        assert_eq!(
            parse_plugin_source("https://example.test/owner/repo?x=1").unwrap_err(),
            PluginSourceError::WithQueryOrPort
        );
        assert_eq!(
            parse_plugin_source("https://example.test:8443/owner/repo").unwrap_err(),
            PluginSourceError::WithQueryOrPort
        );
    }

    #[test]
    fn req_034_a_source_with_an_empty_or_unsafe_path_is_refused() {
        assert_eq!(
            parse_plugin_source("https://example.test/").unwrap_err(),
            PluginSourceError::NotOwnerRepo
        );
        assert_eq!(
            parse_plugin_source("https://example.test/%20").unwrap_err(),
            PluginSourceError::NotOwnerRepo
        );
    }

    #[test]
    fn req_034_an_unsafe_ref_is_refused() {
        assert_eq!(
            parse_plugin_source("https://example.test/owner/repo@").unwrap_err(),
            PluginSourceError::BadRef
        );
        assert_eq!(
            parse_plugin_source("https://example.test/owner/repo@a..b").unwrap_err(),
            PluginSourceError::BadRef
        );
        assert_eq!(
            parse_plugin_source("https://example.test/owner/repo@v1%20x").unwrap_err(),
            PluginSourceError::BadRef
        );
    }

    #[test]
    fn req_034_an_unsafe_subdir_is_refused() {
        for subdir in ["..", "a/../b", "/abs", "a//b", "./x"] {
            let source = format!("https://example.test/owner/repo#{subdir}");
            assert_eq!(
                parse_plugin_source(&source).unwrap_err(),
                PluginSourceError::BadSubdir,
                "{subdir}"
            );
        }
    }

    #[test]
    fn req_034_a_trailing_slash_is_tolerated() {
        let parsed =
            parse_plugin_source("https://example.test/owner/repo/").expect("the source parses");
        assert_eq!(parsed.git_url, "https://example.test/owner/repo");
    }
}
