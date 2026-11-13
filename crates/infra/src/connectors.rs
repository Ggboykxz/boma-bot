//! The connector tools executor (REQ-032).
//!
//! Two tools ship: `rss.read` (a public feed, parsed best-effort into entries of title, link and
//! summary — RSS 2.0 `item` and Atom `entry`) and `http-json.get` (a guarded GET with the
//! installation's bearer credential, responses capped at 16 KiB). Both go through the outbound
//! guard; neither ever sees a private address, not even with an allow-list — connectors do not
//! carry one. Parsing is deliberately simple and documented as such: entries have title and
//! link, and any text in between is the summary. Malformed feeds answer with what parsed.

use boma_app::{AppError, ports::ConnectorRunner};
use secrecy::{ExposeSecret, SecretString};

use crate::net_safety::check_outbound_url;

/// The maximum size of one response body: 16 KiB for http-json, 512 KiB for a feed.
const MAX_BODY_BYTES: usize = 512 * 1024;
/// The maximum size of one JSON answer: 16 KiB.
const MAX_JSON_BYTES: usize = 16 * 1024;

/// The HTTP connector runner.
#[derive(Debug, Clone)]
pub struct HttpConnectorRunner {
    http: reqwest::Client,
}

impl HttpConnectorRunner {
    /// Builds the runner with a 30-second timeout and no redirects.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the TLS backend cannot be initialised.
    pub fn new() -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AppError::Unavailable)?;
        Ok(Self { http })
    }

    /// Fetches a URL as bounded bytes.
    async fn fetch(&self, url: &str) -> Result<(String, Vec<u8>), AppError> {
        check_outbound_url(url, false).await.map_err(|reason| {
            AppError::Rule(boma_domain::DomainError::InvalidName(reason.to_string()))
        })?;
        let response = self.http.get(url).send().await.map_err(|error| {
            tracing::warn!(%error, "a connector request failed");
            AppError::Unavailable
        })?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        let bytes = response.bytes().await.map_err(|_| AppError::Unavailable)?;
        Ok((content_type, bytes.to_vec()))
    }
}

#[async_trait::async_trait]
impl ConnectorRunner for HttpConnectorRunner {
    async fn run(
        &self,
        connector_id: &str,
        tool: &str,
        arguments: &serde_json::Value,
        credential: Option<&SecretString>,
    ) -> Result<serde_json::Value, AppError> {
        match (connector_id, tool) {
            ("rss", "read") => {
                let Some(url) = arguments.get("url").and_then(serde_json::Value::as_str) else {
                    return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the tool needs a `url` argument".to_owned(),
                    )));
                };
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(20)
                    .clamp(1, 50) as usize;
                let (_, bytes) = self.fetch(url).await?;
                if bytes.len() > MAX_BODY_BYTES {
                    return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the feed is larger than 512 KiB".to_owned(),
                    )));
                }
                let body = String::from_utf8_lossy(&bytes);
                let entries = parse_feed(&body, limit);
                Ok(serde_json::json!({
                    "feedUrl": url,
                    "count": entries.len(),
                    "entries": entries,
                }))
            }
            ("http-json", "get") => {
                let Some(url) = arguments.get("url").and_then(serde_json::Value::as_str) else {
                    return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the tool needs a `url` argument".to_owned(),
                    )));
                };
                let Some(token) = credential else {
                    return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                        "this connector needs its `apiKey`".to_owned(),
                    )));
                };
                check_outbound_url(url, false).await.map_err(|reason| {
                    AppError::Rule(boma_domain::DomainError::InvalidName(reason.to_string()))
                })?;
                let response = self
                    .http
                    .get(url)
                    .header(
                        reqwest::header::AUTHORIZATION,
                        format!("Bearer {}", token.expose_secret()),
                    )
                    .send()
                    .await
                    .map_err(|error| {
                        tracing::warn!(%error, "the http-json request failed");
                        AppError::Unavailable
                    })?;
                let status = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("application/octet-stream")
                    .to_owned();
                let bytes = response.bytes().await.map_err(|_| AppError::Unavailable)?;
                if bytes.len() > MAX_JSON_BYTES {
                    return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the answer is larger than 16 KiB".to_owned(),
                    )));
                }
                let body = match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(parsed) => parsed,
                    Err(_) => serde_json::json!(String::from_utf8_lossy(&bytes)),
                };
                Ok(serde_json::json!({
                    "status": status,
                    "contentType": content_type,
                    "body": body,
                }))
            }
            _ => Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "this connector does not have that tool".to_owned(),
            ))),
        }
    }
}

/// Parses RSS 2.0 `item` and Atom `entry` blocks: title, link, summary. Best-effort, in order.
fn parse_feed(body: &str, limit: usize) -> Vec<serde_json::Value> {
    let mut entries = Vec::with_capacity(limit);
    for marker in ["<item", "<entry"] {
        let mut rest = body;
        while entries.len() < limit {
            let Some(start) = rest.find(marker) else {
                break;
            };
            let after = &rest[start..];
            let end = after.find("</item>").or_else(|| after.find("</entry>"));
            let Some(end) = end else { break };
            let block_end = end + "</entry>".len().min("</item>".len());
            let block = &after[..block_end];
            let title = tag_text(block, "title");
            let link = link_of(block);
            let summary = tag_text(block, "description")
                .or_else(|| tag_text(block, "summary"))
                .or_else(|| tag_text(block, "content"));
            entries.push(serde_json::json!({
                "title": title,
                "link": link,
                "summary": summary,
            }));
            rest = &after[block_end..];
        }
        if !entries.is_empty() {
            break;
        }
    }
    entries
}

/// The text inside the first `<tag>…</tag>` of a block, unescaped and stripped of nested tags.
fn tag_text(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = block.find(&open)?;
    let after_open = block[start..].find('>')? + start + 1;
    let close = format!("</{tag}>");
    let end = block[after_open..].find(&close)? + after_open;
    let raw = &block[after_open..end];
    let raw = raw.trim_start_matches("<![CDATA[").trim_end_matches("]]>");
    Some(clean_text(raw))
}

/// The link of a block: `<link>…</link>` for RSS, `<link href="…"/>` for Atom.
fn link_of(block: &str) -> Option<String> {
    if let Some(text) = tag_text(block, "link")
        && !text.is_empty()
    {
        return Some(text);
    }
    let marker = "<link";
    let start = block.find(marker)?;
    let after = &block[start..];
    let href = after.find("href=\"")? + "href=\"".len() + start;
    let end = block[href..].find('"')? + href;
    Some(block[href..end].to_owned())
}

/// Strips nested tags and unescapes the usual entities.
fn clean_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{clean_text, link_of, parse_feed};

    #[test]
    fn req_032_an_rss_feed_parses_its_entries() {
        let xml = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
<item><title>First</title><link>https://example.com/1</link><description><![CDATA[<p>One <b>post</b></p>]]></description></item>
<item><title>Second &amp; third</title><link>https://example.com/2</link></item>
</channel></rss>"#;
        let entries = parse_feed(xml, 10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["title"], "First");
        assert_eq!(entries[0]["link"], "https://example.com/1");
        assert_eq!(entries[0]["summary"], "One post");
        assert_eq!(entries[1]["title"], "Second & third");
    }

    #[test]
    fn req_032_an_atom_feed_parses_its_entries() {
        let xml = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<entry><title>Atom one</title><link href="https://example.com/a"/><summary>Summary A</summary></entry>
<entry><title>Atom two</title><link href="https://example.com/b"/></entry>
</feed>"#;
        let entries = parse_feed(xml, 10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["title"], "Atom one");
        assert_eq!(entries[0]["link"], "https://example.com/a");
        assert_eq!(entries[0]["summary"], "Summary A");
        assert_eq!(
            link_of("<entry><link href=\"https://example.com/x\"/></entry>"),
            Some("https://example.com/x".to_owned())
        );
    }

    #[test]
    fn req_032_clean_text_strips_tags_and_entities() {
        assert_eq!(
            clean_text("<p>Hello, <b>world</b> &amp; friends</p>"),
            "Hello, world & friends"
        );
    }
}
