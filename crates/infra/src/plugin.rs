//! The GitHub fetcher for plugin sources (REQ-034).
//!
//! `api.github.com` resolves the commit SHA and the tree; `raw.githubusercontent.com` serves
//! the component files. Only public `github.com` sources are supported in this build (a source
//! the reference rejects at review time is refused here at fetch time), HTTPS only, no
//! redirects, no credential material in any URL. When the ref is a full SHA, the fetched SHA
//! must equal it: a plugin can never be swapped out from under the member who reviewed it.
//!
//! Limits, fail-closed: 5000 tree entries, 4 MiB of tree JSON, 40 files fetched, 100 KiB per
//! file. A symlink in the tree is never followed.
//!
//! The `with_loopback_endpoints` constructor re-points the same logic at a loopback wire
//! server (`wiremock`); it exists for the integration tests and nothing else.

use boma_app::{
    AppError,
    ports::{FetchedPlugin, PluginFetcher, PluginRepoFile},
};
use serde_json::Value;

/// The maximum size of the tree document: 4 MiB.
const MAX_TREE_BYTES: usize = 4 * 1024 * 1024;
/// The maximum number of entries in a tree document.
const MAX_TREE_ENTRIES: usize = 5000;
/// The maximum number of component files fetched per plugin.
const MAX_FILES: usize = 40;
/// The maximum size of one component file.
const MAX_FILE_BYTES: usize = 100 * 1024;
/// The extensions of skill companion files that are kept (text-like only).
const COMPANION_EXTENSIONS: &[&str] = &["md", "mdx", "txt", "json", "yml", "yaml", "csv"];
/// Whether a reference is a full commit SHA: 40 hex characters.
fn is_full_sha(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The GitHub plugin fetcher.
#[derive(Debug, Clone)]
pub struct GitHubPluginFetcher {
    http: reqwest::Client,
    api_base: String,
    raw_base: String,
    loopback_only: bool,
}

impl GitHubPluginFetcher {
    /// Builds the fetcher against the public GitHub API with HTTPS and no redirects.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the TLS backend cannot be initialised.
    pub fn new() -> Result<Self, AppError> {
        Self::with_loopback_endpoints(
            "https://api.github.com",
            "https://raw.githubusercontent.com",
        )
    }

    /// A fetcher against arbitrary (loopback) endpoints. For tests only.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the default reqwest client cannot be built.
    pub fn with_loopback_endpoints(api_base: &str, raw_base: &str) -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AppError::Unavailable)?;
        Ok(Self {
            http,
            api_base: api_base.trim_end_matches('/').to_owned(),
            raw_base: raw_base.trim_end_matches('/').to_owned(),
            loopback_only: !api_base.starts_with("https://"),
        })
    }

    /// One server call, bounded and HTTPS-only outside loopback mode.
    async fn get(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, AppError> {
        if !self.loopback_only && !url.starts_with("https://") {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "plugin fetches use HTTPS".to_owned(),
            )));
        }
        let response = self
            .http
            .get(url)
            .header(reqwest::header::USER_AGENT, "boma-bot")
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, "a plugin fetch failed");
                AppError::Unavailable
            })?;
        if !response.status().is_success() {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                format!("the fetch answered with status {}", response.status()),
            )));
        }
        let bytes = response.bytes().await.map_err(|_| AppError::Unavailable)?;
        if bytes.len() > max_bytes {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the fetched document is too large".to_owned(),
            )));
        }
        Ok(bytes.to_vec())
    }
}

#[async_trait::async_trait]
#[allow(clippy::too_many_lines)] // the fetch is four ordered phases; the comments are the phases
impl PluginFetcher for GitHubPluginFetcher {
    async fn fetch(
        &self,
        source: &boma_domain::PluginSourceRef,
    ) -> Result<FetchedPlugin, AppError> {
        let parsed = url::Url::parse(&source.git_url).map_err(|_| {
            AppError::Rule(boma_domain::DomainError::InvalidName(
                "the plugin source does not parse".to_owned(),
            ))
        })?;
        if !self.loopback_only && parsed.host_str() != Some("github.com") {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "only public github.com plugin sources are supported".to_owned(),
            )));
        }
        let repo = parsed.path().trim_start_matches('/').trim_end_matches('/');
        let reference = source.ref_.clone().unwrap_or_else(|| "HEAD".to_owned());

        // 1. Resolve the commit SHA of the ref.
        let commit_url = format!("{}/repos/{}/commits/{}", self.api_base, repo, reference);
        let commit: Value = serde_json::from_slice(&self.get(&commit_url, MAX_TREE_BYTES).await?)
            .map_err(|_| AppError::Unavailable)?;
        let sha = commit
            .get("sha")
            .and_then(Value::as_str)
            .filter(|sha| is_full_sha(sha))
            .ok_or_else(|| {
                AppError::Rule(boma_domain::DomainError::InvalidName(
                    "could not resolve the plugin commit".to_owned(),
                ))
            })?
            .to_owned();
        if let Some(requested) = &source.ref_
            && is_full_sha(requested)
            && requested != &sha
        {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the fetched commit does not match the requested pin".to_owned(),
            )));
        }

        // 2. The tree at that SHA.
        let tree_url = format!(
            "{}/repos/{}/git/trees/{}?recursive=1",
            self.api_base, repo, sha
        );
        let tree: Value = serde_json::from_slice(&self.get(&tree_url, MAX_TREE_BYTES).await?)
            .map_err(|_| AppError::Unavailable)?;
        if tree
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the plugin repository tree is incomplete".to_owned(),
            )));
        }
        let entries = tree
            .get("tree")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if entries.len() > MAX_TREE_ENTRIES {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the plugin repository has too many files".to_owned(),
            )));
        }

        // 3. Keep only the component files, honouring the subdirectory prefix.
        let prefix = source.subdir.as_ref().map_or_else(String::new, |subdir| {
            format!("{}/", subdir.trim_matches('/'))
        });
        let mut wanted: Vec<&Value> = Vec::new();
        for entry in &entries {
            let path = entry
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !path.starts_with(&prefix) {
                continue;
            }
            let relative = &path[prefix.len()..];
            let kind = entry.get("type").and_then(Value::as_str).unwrap_or("");
            let mode = entry.get("mode").and_then(Value::as_str).unwrap_or("");
            if kind != "blob" || mode == "120000" || relative.contains("..") {
                continue;
            }
            let size = entry
                .get("size")
                .and_then(Value::as_u64)
                .and_then(|size| usize::try_from(size).ok())
                .unwrap_or(usize::MAX);
            if size > MAX_FILE_BYTES {
                continue;
            }
            let is_manifest = relative == "plugin.json" || relative == ".mcp.json";
            let is_skill_md = relative.starts_with("skills/")
                && relative
                    .split('/')
                    .nth(1)
                    .is_some_and(|s| !s.is_empty() && !s.contains('.'))
                && relative.ends_with("/SKILL.md");
            let is_companion = relative.starts_with("skills/")
                && relative.split('/').count() >= 3
                && !relative.ends_with("/SKILL.md")
                && relative.rsplit('.').next().is_some_and(|ext| {
                    COMPANION_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
                });
            if is_manifest || is_skill_md || is_companion {
                wanted.push(entry);
                if wanted.len() > MAX_FILES {
                    return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the plugin has too many files".to_owned(),
                    )));
                }
            }
        }

        // 4. Fetch exactly those.
        let mut files = Vec::with_capacity(wanted.len());
        for entry in wanted {
            let path = entry
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let relative = &path[prefix.len()..];
            let raw_url = format!("{}/{}/{}/{}", self.raw_base, repo, sha, path);
            let content = self.get(&raw_url, MAX_FILE_BYTES).await?;
            files.push(PluginRepoFile {
                path: relative.to_owned(),
                content,
            });
        }
        Ok(FetchedPlugin {
            files,
            resolved_sha: Some(sha),
        })
    }
}
