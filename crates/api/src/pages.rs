//! The sandboxed rendering of a shared page (REQ-039), and the pinned runtime asset.
//!
//! A shared page is served two ways from one route. JSON (`Accept: application/json`) is the
//! contract body the goldens pin. HTML (`Accept: text/html`) is a document that runs under a
//! strict content policy: no script of its own, no network, every block rendered through the
//! pinned, same-origin page runtime. The app's own policy is untouched; the document never
//! leaves its sandbox.

use std::sync::OnceLock;

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// The CSP the shared HTML document carries (REQ-039).
///
/// `sandbox allow-scripts` without `allow-same-origin` makes the document an opaque origin:
/// no cookies, no storage, no reach into anything of this deployment. `script-src 'self'` is the
/// pinned runtime and nothing else; `connect-src 'none'` closes the network even if a script ran.
pub const SHARED_PAGE_CSP: &str = "default-src 'none'; sandbox allow-scripts; script-src 'self'; style-src 'unsafe-inline'; img-src data: https:; connect-src 'none'; form-action 'none'; base-uri 'none'";

/// The pinned runtime asset, embedded at compile time.
const PAGE_RUNTIME_JS: &str = include_str!("../assets/page-runtime.js");

/// Escapes text for inclusion in HTML markup or the `<title>` element.
fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Renders the shared page document. The block payload rides in an `application/json` island
/// (inert data, never executed); `</script>` inside the JSON is neutralised.
#[must_use]
pub fn render_shared_page_html(title: &str, source: &serde_json::Value) -> String {
    let island = serde_json::to_string(source)
        .unwrap_or_else(|_| "{}".to_owned())
        .replace("</", "<\\/");
    let title = html_escape(title);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta name=\"referrer\" content=\"no-referrer\">\n<title>{title}</title>\n<style>body{{font-family:system-ui,sans-serif;margin:2rem auto;max-width:48rem;padding:0 1rem;color:#111;background:#fff}}main{{display:flex;flex-direction:column;gap:1rem}}p{{line-height:1.5}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #ddd;padding:.4rem .6rem;text-align:left}}.block-chart .chart-bars{{list-style:none;padding:0;display:flex;flex-direction:column;gap:.3rem}}.chart-bar-row{{display:flex;align-items:center;gap:.5rem}}.chart-bar-label{{min-width:8rem}}.chart-bar{{display:inline-block;height:1rem;background:#4f46e5;max-width:100%}}.chart-bar-value{{min-width:3rem}}.block-image{{max-width:100%}}</style>\n</head>\n<body>\n<main id=\"app\"></main>\n<script id=\"page-data\" type=\"application/json\">{island}</script>\n<script src=\"/api/v1/assets/page-runtime.js\" defer></script>\n</body>\n</html>\n"
    )
}

/// The weak-free ETag the runtime asset is served under, computed once.
fn runtime_etag() -> &'static str {
    static ETAG: OnceLock<String> = OnceLock::new();
    ETAG.get_or_init(|| {
        let digest = Sha256::digest(PAGE_RUNTIME_JS.as_bytes());
        let mut hex = String::with_capacity(64);
        for byte in digest {
            let _ = write!(hex, "{byte:02x}");
        }
        format!("\"{hex}\"")
    })
}

/// `GET /api/v1/assets/page-runtime.js` — the pinned renderer (REQ-039).
///
/// The document is rebuilt from this exact file at compile time; the ETag changes with it.
/// A matching `If-None-Match` answers 304 with the same headers and no body.
///
/// # Panics
///
/// Never: the response builder for a static asset cannot fail. The lint is silenced by the
/// two `expect`s below being definitionally unreachable.
#[allow(clippy::expect_used)]
pub async fn page_runtime_js(headers: HeaderMap) -> Response {
    let etag = runtime_etag();
    let matches = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == etag || value == "*");
    let mut response = if matches {
        Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .body(Body::empty())
            .expect("the empty 304 builds")
    } else {
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(PAGE_RUNTIME_JS))
            .expect("the asset body builds")
    };
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/javascript; charset=utf-8"),
    );
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(etag).expect("the etag is a valid header value"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::{SHARED_PAGE_CSP, render_shared_page_html};

    #[test]
    fn req_039_the_document_has_no_inline_script_and_neutralises_the_island() {
        let source = serde_json::json!({
            "title": "t</title><script>alert(1)</script>",
            "blocks": [{ "type": "text", "text": "break the island: </script><script>alert(1)</script>" }],
        });
        let html = render_shared_page_html("Weekly report", &source);
        assert!(html.contains("<title>Weekly report</title>"));
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(
            html.contains("<\\/script>"),
            "the island cannot be broken out of"
        );
        // The only <script> tags allowed: the inert island and the pinned same-origin runtime.
        for needle in [
            "<script id=\"page-data\" type=\"application/json\">",
            "<script src=\"/api/v1/assets/page-runtime.js\" defer></script>",
        ] {
            assert!(html.contains(needle), "{needle}");
        }
        assert_eq!(
            html.matches("</script>").count(),
            2,
            "exactly the island and the runtime close"
        );
        assert!(SHARED_PAGE_CSP.contains("sandbox allow-scripts"));
        assert!(SHARED_PAGE_CSP.contains("connect-src 'none'"));
        assert!(!SHARED_PAGE_CSP.contains("allow-same-origin"));
    }
}
