//! Artifacts (REQ-036): name rules, and the content sniff D-03 demands.
//!
//! An artifact never lands under a lie: the bytes decide the type a space is shown. The
//! declared type is a hint; when it is specific, it must agree with the content (or be refused);
//! when it is generic (`application/octet-stream`), the sniffed type is stored instead; when the
//! content is HTML but the member declared anything else in the text family, it is refused —
//! HTML-as-PNG is the canonical abuse of this endpoint.

use crate::DomainError;

/// The longest artifact name accepted.
pub const ARTIFACT_NAME_MAX: usize = 120;
/// The longest description accepted.
pub const ARTIFACT_DESCRIPTION_MAX: usize = 1000;
/// The raw byte cap for one artifact (≈5.46 MB of base64 on the wire).
pub const ARTIFACT_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Checks a file name: trimmed, small, no path separators, no control characters.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the name is refused.
pub fn validate_artifact_name(name: &str) -> Result<(), DomainError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidName(
            "the artifact name must not be empty".to_owned(),
        ));
    }
    if trimmed.chars().count() > ARTIFACT_NAME_MAX {
        return Err(DomainError::InvalidName(format!(
            "the artifact name is longer than {ARTIFACT_NAME_MAX} characters"
        )));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(DomainError::InvalidName(
            "the artifact name must not contain control characters or path separators".to_owned(),
        ));
    }
    Ok(())
}

/// Checks a description: bounded, no exotic control characters.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the description is refused.
pub fn validate_artifact_description(description: &str) -> Result<(), DomainError> {
    if description.chars().count() > ARTIFACT_DESCRIPTION_MAX {
        return Err(DomainError::InvalidName(format!(
            "the artifact description is longer than {ARTIFACT_DESCRIPTION_MAX} characters"
        )));
    }
    if description
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(DomainError::InvalidName(
            "the artifact description must not contain control characters".to_owned(),
        ));
    }
    Ok(())
}

/// The magic bytes of one content type, most common first.
fn signature(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf");
    }
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        return Some("application/zip");
    }
    None
}

/// Whether the first 4 KiB of `bytes` hold an HTML document (doctype or `<html`-ish marker).
fn looks_like_html(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    let lowered: String = head
        .iter()
        .map(|byte| {
            if byte.is_ascii() {
                byte.to_ascii_lowercase() as char
            } else {
                '\u{1}'
            }
        })
        .collect();
    ["<!doctype html", "<html", "<head", "<body", "<script"]
        .iter()
        .any(|marker| lowered.contains(marker))
}

/// Whether `bytes` are UTF-8 text with printable content (tab, newline and carriage return allowed).
fn is_plain_text(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(text) => !text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\u{0c}')),
        Err(_) => false,
    }
}

/// The honest content type of `bytes`, or `None` when nothing recognises them.
#[must_use]
pub fn detect_mime(bytes: &[u8]) -> Option<String> {
    if let Some(signature) = signature(bytes) {
        return Some(signature.to_owned());
    }
    if looks_like_html(bytes) {
        return Some("text/html".to_owned());
    }
    if is_plain_text(bytes) {
        return Some("text/plain".to_owned());
    }
    None
}

/// Whether a declared MIME type is well shaped: `type/subtype`, all ascii token chars.
fn declared_type_ok(declared: &str) -> bool {
    let Some((top, sub)) = declared.split_once('/') else {
        return false;
    };
    let ok = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '+' | '.' | '_'))
    };
    ok(top) && ok(sub)
}

/// Resolves the type an artifact is stored as, refusing a declared/content lie (defect D-03).
///
/// `declared` is what the client claimed; the returned string is what the bytes proved.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the content cannot be recognised or the declared
/// type disagrees with it beyond the text family.
pub fn sniff_artifact_mime(bytes: &[u8], declared: &str) -> Result<String, DomainError> {
    let declared = declared.trim().to_ascii_lowercase();
    if !declared_type_ok(&declared) {
        return Err(DomainError::InvalidName(
            "the declared MIME type is not a valid `type/subtype`".to_owned(),
        ));
    }
    let generic = declared == "application/octet-stream";
    let sniffed = detect_mime(bytes);
    let text_family = declared.starts_with("text/")
        || matches!(
            declared.as_str(),
            "application/json" | "application/xml" | "application/javascript"
        );
    if generic {
        return sniffed.ok_or_else(|| {
            DomainError::InvalidName("the content does not match any recognised type".to_owned())
        });
    }
    match sniffed.as_deref() {
        None => Err(DomainError::InvalidName(if text_family {
            "the content is not text".to_owned()
        } else {
            "the content does not match the declared type".to_owned()
        })),
        Some("text/plain") if text_family => Ok(declared),
        Some("text/plain") => Err(DomainError::InvalidName(format!(
            "the content is plain text, not the declared `{declared}`"
        ))),
        Some("text/html") if declared == "text/html" => Ok(declared),
        Some("text/html") if text_family => Err(DomainError::InvalidName(
            "the content is HTML, not the declared text type".to_owned(),
        )),
        Some(sniffed) if sniffed == declared => Ok(declared),
        Some(sniffed) => Err(DomainError::InvalidName(format!(
            "the declared type `{declared}` does not match the content ({sniffed})"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_mime, sniff_artifact_mime, validate_artifact_name};

    #[test]
    fn req_036_a_well_formed_name_is_accepted() {
        assert!(validate_artifact_name("report.md").is_ok());
        assert!(validate_artifact_name("  Quarterly report  ").is_ok());
    }

    #[test]
    fn req_036_a_path_like_or_empty_name_is_refused() {
        for name in ["", "   ", "../etc/passwd", "a/b.png", "a\\b.png"] {
            assert!(validate_artifact_name(name).is_err(), "{name:?}");
        }
    }

    #[test]
    fn d03_a_png_is_sniffed_however_it_is_declared() {
        let png = b"\x89PNG\r\n\x1a\nrest of the file";
        assert_eq!(
            sniff_artifact_mime(png, "image/png").as_deref(),
            Ok("image/png")
        );
        assert_eq!(
            sniff_artifact_mime(png, "application/octet-stream").as_deref(),
            Ok("image/png")
        );
        assert!(sniff_artifact_mime(png, "image/jpeg").is_err());
        assert!(sniff_artifact_mime(png, "text/markdown").is_err());
    }

    #[test]
    fn d03_html_declared_as_png_is_rejected() {
        let html = b"<!DOCTYPE html><html><body>hi</body></html>";
        let error = sniff_artifact_mime(html, "image/png").expect_err("the lie is refused");
        assert!(error.to_string().contains("does not match"), "{error}");
    }

    #[test]
    fn d03_html_declared_as_text_is_rejected_unless_html_is_declared() {
        let html = b"<html><body>hi</body></html>";
        assert!(sniff_artifact_mime(html, "text/markdown").is_err());
        assert_eq!(
            sniff_artifact_mime(html, "text/html").as_deref(),
            Ok("text/html")
        );
    }

    #[test]
    fn d03_a_generic_declaration_is_corrected() {
        let pdf = b"%PDF-1.7\nblob";
        assert_eq!(
            sniff_artifact_mime(pdf, "application/octet-stream").as_deref(),
            Ok("application/pdf")
        );
        assert_eq!(
            sniff_artifact_mime(b"plain words", "application/octet-stream").as_deref(),
            Ok("text/plain")
        );
        assert!(sniff_artifact_mime(b"\x00\x01\x02\x03", "application/octet-stream").is_err());
    }

    #[test]
    fn d03_a_plain_text_file_keeps_its_text_family() {
        assert_eq!(
            sniff_artifact_mime(b"# hello".as_slice(), "text/markdown").as_deref(),
            Ok("text/markdown")
        );
        assert_eq!(
            sniff_artifact_mime(b"{}".as_slice(), "application/json").as_deref(),
            Ok("application/json")
        );
        assert!(sniff_artifact_mime(b"\xff\xfe\xfd".as_slice(), "text/plain").is_err());
    }

    #[test]
    fn d03_detection_covers_the_signatures() {
        assert_eq!(
            detect_mime(b"\xff\xd8\xff\xe0").as_deref(),
            Some("image/jpeg")
        );
        assert_eq!(detect_mime(b"GIF89a").as_deref(), Some("image/gif"));
        assert_eq!(
            detect_mime(b"PK\x03\x04").as_deref(),
            Some("application/zip")
        );
        assert_eq!(
            detect_mime(b"<html>".as_slice()).as_deref(),
            Some("text/html")
        );
        assert_eq!(
            detect_mime(b"just words".as_slice()).as_deref(),
            Some("text/plain")
        );
        assert_eq!(detect_mime(b"\x00\x01"), None);
    }
}
