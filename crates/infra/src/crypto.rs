//! Passwords, tokens, identifiers and time: the small services the product needs.
//!
//! Everything here is deterministic in its contract and random in its bytes. The randomness comes
//! from the operating system's generator (`getrandom`), never from a seeded generator: a token
//! that can be predicted is a session that can be stolen.

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, try_generate_salt},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{TryRng as _, rngs::SysRng};
use sha2::{Digest as _, Sha256};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

/// The number of random bytes behind a session or invitation token.
///
/// Thirty-two bytes is 256 bits of entropy: guessing one is not an attack anyone can afford.
const TOKEN_BYTES: usize = 32;

/// The number of random bytes behind an identifier.
///
/// Nineteen bytes are exactly twenty-six base64url characters, and still carry 152 bits.
const ID_BYTES: usize = 19;

/// The number of base64url characters an identifier carries.
const ID_LENGTH: usize = 26;

/// The timestamp used when the system clock cannot be rendered.
///
/// It cannot happen: the system clock is always inside the range RFC 3339 accepts. It is here so
/// the function never panics and never returns an empty string.
const FALLBACK_TIMESTAMP: &str = "1970-01-01T00:00:00Z";

/// Hashes and checks passwords with Argon2id.
///
/// Argon2id is the hybrid variant: it resists both GPU cracking and side channels, which is why
/// the default algorithm of the `argon2` crate is used rather than a hand-picked one. The stored
/// value is a PHC string, so the parameters travel with the hash and can be raised later without
/// invalidating the passwords already stored.
#[derive(Debug, Clone, Default)]
pub struct Argon2idHasher {
    argon2: Argon2<'static>,
}

impl Argon2idHasher {
    /// Builds the hasher with the default parameters (Argon2id, version 19, 19 MiB, two passes).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl boma_app::ports::PasswordHasher for Argon2idHasher {
    async fn hash(&self, password: &str) -> Result<String, boma_app::AppError> {
        let salt = try_generate_salt().map_err(|error| {
            tracing::debug!(error = %error, "the system random source failed");
            boma_app::AppError::Unavailable
        })?;
        let hash = self
            .argon2
            .hash_password_with_salt(password.as_bytes(), &salt)
            .map_err(|error| {
                tracing::debug!(error = %error, "the password could not be hashed");
                boma_app::AppError::Unavailable
            })?;
        Ok(hash.to_string())
    }

    async fn verify(&self, password: &str, hash: &str) -> Result<bool, boma_app::AppError> {
        // A hash that cannot be read is a broken row, not a wrong password: the two answers are
        // different, and the caller must be able to tell them apart.
        let parsed = argon2::PasswordHash::new(hash).map_err(|error| {
            tracing::debug!(error = %error, "the stored password hash cannot be read");
            boma_app::AppError::Unavailable
        })?;
        Ok(self
            .argon2
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }
}

/// Mints the opaque tokens the product hands out, and hashes them for storage.
///
/// The token is random; the hash is its SHA-256. A session row therefore holds a hash, and the
/// token itself only ever exists in the cookie the person carries.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenGenerator;

impl TokenGenerator {
    /// Builds the generator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl boma_app::ports::TokenGenerator for TokenGenerator {
    fn generate(&self) -> Result<String, boma_app::AppError> {
        let mut bytes = [0u8; TOKEN_BYTES];
        SysRng.try_fill_bytes(&mut bytes).map_err(|error| {
            tracing::debug!(error = %error, "the system random source failed");
            boma_app::AppError::Unavailable
        })?;
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }

    fn hash(&self, token: &str) -> Result<Vec<u8>, boma_app::AppError> {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        Ok(hasher.finalize().to_vec())
    }
}

/// Mints the opaque identifiers the product hands out.
///
/// An identifier is a prefix and twenty-six base64url characters: `usr_3Qk9...`. The characters
/// carry no meaning and no order, so nothing in the product may sort by them or guess them.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdGenerator;

impl IdGenerator {
    /// Builds the generator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl boma_app::ports::IdGenerator for IdGenerator {
    fn new_id(&self, prefix: &str) -> String {
        let mut bytes = [0u8; ID_BYTES];
        if SysRng.try_fill_bytes(&mut bytes).is_err() {
            // The random source failed. An identifier is not a secret, so a last-resort value
            // that is unique but not random is better than a crash: it is counted, stamped and
            // tagged with the process, so two identifiers never collide.
            tracing::error!("the system random source failed: identifiers fall back to a counter");
            return format!("{prefix}_{}", fallback_id());
        }
        let encoded = URL_SAFE_NO_PAD.encode(bytes);
        debug_assert_eq!(encoded.len(), ID_LENGTH);
        format!("{prefix}_{encoded}")
    }
}

/// A last-resort identifier: unique, not random.
fn fallback_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = OffsetDateTime::now_utc().unix_timestamp_nanos();
    let pid = u64::from(std::process::id());
    format!("{stamp:016x}{pid:08x}{counter:08x}")
}

/// Tells the time, as RFC 3339 in UTC with second precision.
///
/// Second precision is what the product compares: a session expiry is compared as text, and a
/// timestamp with a fraction would make two representations of the same instant differ.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl SystemClock {
    /// Builds the clock.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl boma_app::ports::Clock for SystemClock {
    fn now(&self) -> String {
        render(OffsetDateTime::now_utc())
    }

    fn now_plus_seconds(&self, seconds: i64) -> String {
        render(OffsetDateTime::now_utc() + time::Duration::seconds(seconds))
    }
}

/// Renders an instant as RFC 3339 in UTC, with second precision.
fn render(at: OffsetDateTime) -> String {
    let seconds = at.replace_nanosecond(0).unwrap_or(at);
    seconds
        .to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .unwrap_or_else(|_| FALLBACK_TIMESTAMP.to_owned())
}

/// Compares two secrets in constant time.
///
/// Used where a secret is compared outside the database, so the time taken does not say how much
/// of the secret was guessed right.
#[must_use]
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    left.len() == right.len() && left.ct_eq(right).into()
}
