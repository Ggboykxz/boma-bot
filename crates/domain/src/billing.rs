//! Billing (REQ-051): the Stripe-shaped webhook signature, nothing else in this crate.
//!
//! The webhook is a POST the deployment receives after a browser-side checkout. The provider
//! proves itself with `Stripe-Signature: t=<seconds>,v1=<hmac-sha256(secret, t + "." + body)>`.
//! The rebuild verifies it with the same rules Stripe documents: the timestamp must be inside a
//! five-minute window and the digest must match in constant time. Everything after verification
//! — reading the event, seeding credits — is the application's business, not the domain's.

use crate::DomainError;

/// The maximum drift between the event timestamp and this deployment's clock, in seconds.
pub const WEBHOOK_MAX_SKEW_SECONDS: i64 = 300;

/// The window in which a checkout session completed stays open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeSignature<'a> {
    /// The signed timestamp, seconds since the epoch.
    pub timestamp: &'a str,
    /// The `v1` digest, lowercase hex.
    pub digest: &'a str,
}

/// Parses a `Stripe-Signature` header.
///
/// # Errors
///
/// Returns [`DomainError::InvalidName`] when the header carries no `t` or no `v1`.
pub fn parse_stripe_signature(header: &str) -> Result<StripeSignature<'_>, DomainError> {
    let mut timestamp = None;
    let mut digest = None;
    for part in header.split(',') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("t=") {
            timestamp = Some(value);
        } else if let Some(value) = part.strip_prefix("v1=") {
            digest = Some(value);
        }
    }
    match (timestamp, digest) {
        (Some(timestamp), Some(digest)) if !timestamp.is_empty() && !digest.is_empty() => {
            Ok(StripeSignature { timestamp, digest })
        }
        _ => Err(DomainError::InvalidName(
            "the Stripe-Signature header carries no t or v1".to_owned(),
        )),
    }
}

/// Whether the webhook signature is one Stripe produced, in constant time.
#[must_use]
pub fn verify_stripe_signature(
    secret: &[u8],
    timestamp: &str,
    body: &[u8],
    header: &str,
    now: i64,
) -> bool {
    use hmac::{Hmac, KeyInit, Mac};

    let Ok(parsed) = parse_stripe_signature(header) else {
        return false;
    };
    if parsed.timestamp != timestamp {
        return false;
    }
    let Ok(sent) = timestamp.parse::<i64>() else {
        return false;
    };
    if (now - sent).abs() > WEBHOOK_MAX_SKEW_SECONDS {
        return false;
    }
    if parsed.digest.is_empty() || !parsed.digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return false;
    }
    let Ok(mut mac) = Hmac::<sha2::Sha256>::new_from_slice(secret) else {
        return false; // unreachable: HMAC-SHA256 accepts any key length
    };
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    let mut expected = String::with_capacity(64);
    for byte in mac.finalize().into_bytes() {
        use std::fmt::Write as _;
        let _ = write!(expected, "{byte:02x}");
    }
    let mut diff = parsed.digest.len() ^ expected.len();
    for (left, right) in parsed.digest.as_bytes().iter().zip(expected.as_bytes()) {
        diff |= usize::from(left ^ right);
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::{parse_stripe_signature, verify_stripe_signature};

    fn signed(secret: &[u8], timestamp: &str, body: &[u8]) -> String {
        use hmac::{Hmac, KeyInit, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret).expect("any length");
        mac.update(timestamp.as_bytes());
        mac.update(b".");
        mac.update(body);
        format!(
            "t={timestamp},v1={}",
            mac.finalize()
                .into_bytes()
                .iter()
                .fold(String::new(), |mut acc, byte| {
                    use std::fmt::Write as _;
                    let _ = write!(acc, "{byte:02x}");
                    acc
                })
        )
    }

    #[test]
    fn req_051_a_valid_signature_verifies_inside_the_window() {
        let body = br#"{"type":"checkout.session.completed","data":{"object":{"id":"cs_1"}}}"#;
        let header = signed(b"whsec_secret", "1799999700", body);
        assert_eq!(
            parse_stripe_signature(&header)
                .expect("a parseable header")
                .digest
                .len(),
            64
        );
        assert!(verify_stripe_signature(
            b"whsec_secret",
            "1799999700",
            body,
            &header,
            1_800_000_000
        ));
        // Ten minutes later: the replay is refused.
        assert!(!verify_stripe_signature(
            b"whsec_secret",
            "1799999700",
            body,
            &header,
            1_800_000_600
        ));
    }

    #[test]
    fn req_051_a_wrong_secret_or_a_tampered_body_is_refused() {
        let body = br#"{"type":"checkout.session.completed"}"#;
        let header = signed(b"whsec_secret", "1799999700", body);
        assert!(!verify_stripe_signature(
            b"other",
            "1799999700",
            body,
            &header,
            1_800_000_000
        ));
        assert!(!verify_stripe_signature(
            b"whsec_secret",
            "1799999700",
            b"tampered",
            &header,
            1_800_000_000
        ));
        assert!(!verify_stripe_signature(
            b"whsec_secret",
            "1799999700",
            body,
            "garbage",
            1_800_000_000
        ));
        assert!(!verify_stripe_signature(
            b"whsec_secret",
            "abc",
            body,
            &header,
            1_800_000_000
        ));
    }

    #[test]
    fn req_051_the_header_tolerates_extra_entries_and_whitespace() {
        let base = parse_stripe_signature("t=1,v1=aa,v0=bb").expect("a parseable header");
        assert_eq!(base.timestamp, "1");
        assert_eq!(base.digest, "aa");
        let padded = parse_stripe_signature(" t=2 , v1= cc ").expect("whitespace is tolerated");
        assert_eq!(padded.timestamp, "2");
    }
}
