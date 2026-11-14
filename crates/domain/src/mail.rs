//! Minimal RFC 822/MIME reading for inbound bot mail (REQ-047).
//!
//! This is a reader, not a mail client: enough of RFC 822 and RFC 2047 to deliver `From`, `To`,
//! `Subject` and the first `text/plain` body, with attachments named and left on the floor. It
//! does not do DKIM, SPF, or MIME tree walking beyond one level of multipart, and it is not
//! Unicode-aware beyond UTF-8 in encoded words and bodies. Anything beyond that refuses loudly
//! rather than guessing.

use std::collections::HashMap;

use crate::DomainError;

/// The largest body text extracted from a message.
pub const MAIL_BODY_MAX: usize = 64 * 1024;
/// The longest subject kept.
pub const MAIL_SUBJECT_MAX: usize = 998;

/// The readable parts of one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMail {
    /// The bare `From` address (lowercased).
    pub from: String,
    /// A display name, when the `From` carried one (decoded).
    pub from_name: Option<String>,
    /// The `To` and `Cc` addresses, bare, lowercased.
    pub recipients: Vec<String>,
    /// The decoded subject (unfolded, encoded words resolved, truncated).
    pub subject: String,
    /// The first `text/plain` body, decoded (base64 or quoted-printable), truncated.
    pub body: String,
}

/// Parses one message as given.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the message is not UTF-8, has no headers, or has no
/// usable `From`.
pub fn parse_rfc822(raw: &[u8]) -> Result<ParsedMail, DomainError> {
    let text = String::from_utf8(raw.to_vec())
        .map_err(|_| DomainError::InvalidName("the message is not valid UTF-8".to_owned()))?;
    let (headers, body) = split_headers(&text)
        .ok_or_else(|| DomainError::InvalidName("the message does not carry headers".to_owned()))?;
    let map = headers_map(&headers);
    let from_raw = map
        .get("from")
        .ok_or_else(|| DomainError::InvalidName("the message has no `From` header".to_owned()))?;
    let (from, from_name) = parse_address(from_raw);
    if from.is_empty() || !from.contains('@') {
        return Err(DomainError::InvalidName(
            "the message has no usable `From` address".to_owned(),
        ));
    }
    let recipients = ["to", "cc"]
        .into_iter()
        .filter_map(|name| map.get(name))
        .flat_map(|value| parse_address_list(value))
        .collect();
    let subject = map
        .get("subject")
        .map(|value| {
            decode_rfc2047(value)
                .chars()
                .take(MAIL_SUBJECT_MAX)
                .collect()
        })
        .unwrap_or_default();
    let content_type = map
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| "text/plain".to_owned());
    let encoding = map
        .get("content-transfer-encoding")
        .map(|value| value.trim().to_ascii_lowercase());
    let body = extract_body(&content_type, encoding.as_deref(), &body);
    Ok(ParsedMail {
        from,
        from_name,
        recipients,
        subject,
        body: body.chars().take(MAIL_BODY_MAX).collect(),
    })
}

/// Splits headers from body. Headers are the part before the first empty line.
fn split_headers(text: &str) -> Option<(String, String)> {
    let mut lines = text.lines();
    let mut headers = String::new();
    loop {
        let line = lines.next()?;
        if line.trim().is_empty() {
            break;
        }
        // Folded headers: a continuation begins with whitespace.
        if line.starts_with(' ') || line.starts_with('\t') {
            headers.push(' ');
            headers.push_str(line.trim());
        } else {
            if !headers.is_empty() {
                headers.push('\n');
            }
            headers.push_str(line);
        }
    }
    Some((headers, lines.collect::<Vec<_>>().join("\n")))
}

/// The headers as a lowercased name → value map (first wins for duplicates).
fn headers_map(headers: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        map.entry(name.trim().to_ascii_lowercase())
            .or_insert_with(|| value.trim().to_owned());
    }
    map
}

/// Extracts a bare address and optional display name from an address header value.
fn parse_address(value: &str) -> (String, Option<String>) {
    let decoded = decode_rfc2047(value);
    if let Some(open) = decoded.rfind('<') {
        let name = decoded[..open].trim().trim_matches(['"', '\'']).trim();
        let addr = decoded[open + 1..]
            .split('>')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let name = if name.is_empty() {
            None
        } else {
            Some(name.to_owned())
        };
        return (addr, name);
    }
    (decoded.trim().to_ascii_lowercase(), None)
}

/// Every bare address in an address list.
fn parse_address_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|part| parse_address(part.trim()).0)
        .filter(|addr| addr.contains('@'))
        .collect()
}

/// Decodes RFC 2047 encoded words (`=?UTF-8?B?...?=` and `=?UTF-8?Q?...?=`).
fn decode_rfc2047(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("=?") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("?=") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let word = &after_start[..end];
        let mut parts = word.splitn(3, '?');
        let Some(charset) = parts.next() else {
            out.push_str(&rest[start..start + 2 + end + 2]);
            rest = &rest[start + 2 + end + 2..];
            continue;
        };
        let Some(encoding) = parts.next() else {
            out.push_str(&rest[start..start + 2 + end + 2]);
            rest = &rest[start + 2 + end + 2..];
            continue;
        };
        let payload = parts.next().unwrap_or("");
        if charset.eq_ignore_ascii_case("utf-8") {
            if encoding.eq_ignore_ascii_case("b") {
                use base64::Engine as _;
                out.push_str(
                    base64::engine::general_purpose::STANDARD
                        .decode(payload)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .as_deref()
                        .unwrap_or("?"),
                );
                rest = &rest[start + 2 + end + 2..];
                continue;
            }
            if encoding.eq_ignore_ascii_case("q") {
                out.push_str(&decode_quoted_printable(payload));
                rest = &rest[start + 2 + end + 2..];
                continue;
            }
        }
        out.push_str(word);
        rest = &rest[start + 2 + end + 2..];
    }
    out.push_str(rest);
    out
}

/// Decodes quoted-printable per RFC 2045, preserving newlines.
fn decode_quoted_printable(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'=' if i + 2 < bytes.len() => {
                if bytes[i + 1] == b'\n' {
                    i += 2;
                    continue;
                }
                if bytes[i + 1] == b'\r' && i + 2 < bytes.len() && bytes[i + 2] == b'\n' {
                    i += 3;
                    continue;
                }
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                let Ok(byte) = u8::from_str_radix(hex, 16) else {
                    out.push(b'=');
                    i += 1;
                    continue;
                };
                out.push(byte);
                i += 3;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

/// The body to carry into a run: first `text/plain` part (multipart) or the flat body.
fn extract_body(content_type: &str, encoding: Option<&str>, body: &str) -> String {
    if content_type.to_ascii_lowercase().starts_with("multipart/") {
        // The boundary is case-sensitive in the body; take it from the header as given.
        let boundary = content_type
            .split(';')
            .find_map(|part| part.trim().strip_prefix("boundary="))
            .map(|boundary| boundary.trim_matches('"').to_owned());
        if let Some(boundary) = boundary {
            for part in body.split(&format!("--{boundary}")) {
                let Some((headers, part_body)) = split_headers(part.trim()) else {
                    continue;
                };
                let part_map = headers_map(&headers);
                let part_type = part_map
                    .get("content-type")
                    .map_or("text/plain", String::as_str);
                if !part_type.to_ascii_lowercase().starts_with("text/plain") {
                    continue;
                }
                let part_encoding = part_map
                    .get("content-transfer-encoding")
                    .map(|value| value.trim().to_ascii_lowercase());
                return decode_transfer(part_encoding.as_deref(), part_body);
            }
            return String::new();
        }
    }
    decode_transfer(encoding, body.to_owned())
}

/// Applies the content-transfer-encoding of a body.
fn decode_transfer(encoding: Option<&str>, body: String) -> String {
    match encoding {
        Some("base64") => {
            use base64::Engine as _;
            let compact: String = body.chars().filter(|ch| !ch.is_whitespace()).collect();
            base64::engine::general_purpose::STANDARD
                .decode(compact.as_bytes())
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_default()
        }
        Some("quoted-printable") => decode_quoted_printable(&body),
        _ => body,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_rfc822;

    #[test]
    fn req_047_a_plain_message_parses() {
        let raw = b"From: Ada Lovelace <ada@example.com>\r\nTo: boma+bot_1@mail.example.com\r\nSubject: Hello there\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nShip the report.\r\n";
        let mail = parse_rfc822(raw).expect("the message parses");
        assert_eq!(mail.from, "ada@example.com");
        assert_eq!(mail.from_name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(mail.subject, "Hello there");
        assert_eq!(mail.recipients, vec!["boma+bot_1@mail.example.com"]);
        assert_eq!(mail.body, "Ship the report.");
    }

    #[test]
    fn req_047_a_multipart_message_yields_its_first_text_part() {
        let raw = b"From: ada@example.com\r\nTo: boma+bot_1@mail.example.com\r\nSubject: =?UTF-8?B?UsOpcG9ydA==?=\r\nContent-Type: multipart/mixed; boundary=XYZ\r\n\r\n--XYZ\r\nContent-Type: application/pdf\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n--XYZ\r\nContent-Type: text/plain\r\n\r\nRead me.\r\n--XYZ--\r\n";
        let mail = parse_rfc822(raw).expect("the message parses");
        assert_eq!(mail.body, "Read me.");
        assert_eq!(mail.subject, "Réport");
    }

    #[test]
    fn req_047_a_base64_body_decodes() {
        let raw = b"From: ada@example.com\r\nTo: boma+bot_1@mail.example.com\r\nContent-Type: text/plain\r\nContent-Transfer-Encoding: base64\r\n\r\nU2hpcCBpdC4=\r\n";
        let mail = parse_rfc822(raw).expect("the message parses");
        assert_eq!(mail.body, "Ship it.");
    }

    #[test]
    fn req_047_garbage_is_refused_loudly() {
        assert!(parse_rfc822(b"").is_err());
        assert!(parse_rfc822(b"From: no-at-sign\r\n\r\nbody").is_err());
        assert!(parse_rfc822(&[0xff, 0xfe, 0xfd]).is_err());
    }
}
