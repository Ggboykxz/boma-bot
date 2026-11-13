//! The encryption half of the secrets vault (REQ-046).
//!
//! Envelope encryption (spec DEC-10, option (a) of the recommended self-hosted profile):
//! every space owns a random 256-bit data-encryption key (DEK) wrapped — AES-256-GCM — under
//! the deployment's master key (`BOMA_MASTER_KEY`, exactly 64 hex characters). Each secret is
//! sealed under its space's DEK. Rotating the master key re-wraps the DEKs only, never the
//! secrets; the stores implement the whole flow and this module implements the primitives.
//!
//! When the key is absent — tests and local development — a per-process key is generated and a
//! warning is logged, so nobody ships a deployment built on an ephemeral key without noticing.
//! Nothing here writes plaintext anywhere else: stores persist ciphertext and nonces only.

use std::collections::BTreeMap;

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use secrecy::{ExposeSecret, SecretString};

use boma_app::ports::SealedSecret;

/// The vault the stores build on.
#[derive(Clone)]
pub struct SecretBox {
    master: [u8; 32],
    stable: bool,
}

impl std::fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The key is deliberately absent: the Debug output is safe to log.
        f.debug_struct("SecretBox")
            .field("stable", &self.stable)
            .finish_non_exhaustive()
    }
}

impl SecretBox {
    /// Builds the vault from `BOMA_MASTER_KEY` when set and valid.
    ///
    /// An empty or whitespace-only value is treated as absent: the variable left empty in an
    /// `.env` or a Compose file means "no key configured", not a malformed key.
    ///
    /// # Errors
    ///
    /// Returns [`SecretBoxError::InvalidKey`] when the variable is set to something that does
    /// not hold exactly 64 hex characters (32 bytes).
    pub fn from_env(env: &BTreeMap<String, String>) -> Result<Self, SecretBoxError> {
        match env
            .get("BOMA_MASTER_KEY")
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            None => {
                tracing::warn!(
                    "BOMA_MASTER_KEY is not set: sealing secrets with an ephemeral per-process key"
                );
                Ok(Self::ephemeral())
            }
            Some(value) => {
                let master = hex_decode::<32>(value).ok_or(SecretBoxError::InvalidKey)?;
                Ok(Self {
                    master,
                    stable: true,
                })
            }
        }
    }

    /// A vault nobody provided a key for. Only in tests and local development.
    #[must_use]
    pub fn ephemeral() -> Self {
        Self {
            master: fresh_data_key(),
            stable: false,
        }
    }

    /// Whether the key came from the environment, not from this process.
    #[must_use]
    pub const fn stable_key(&self) -> bool {
        self.stable
    }

    /// Seals `value` under a data-encryption key, with a fresh nonce.
    fn seal_under_with_nonce(
        space_key: &[u8; 32],
        value: &[u8],
    ) -> Result<SealedSecret, boma_app::AppError> {
        let mut nonce_bytes = [0u8; 12];
        getrandom::fill(&mut nonce_bytes).map_err(|_| boma_app::AppError::Unavailable)?;
        let aead = data_key_cipher(space_key)?;
        let ciphertext = aead
            .encrypt(&Nonce::from(nonce_bytes), value)
            .map_err(|_| boma_app::AppError::Unavailable)?;
        Ok(SealedSecret {
            ciphertext,
            nonce: nonce_bytes.to_vec(),
        })
    }

    /// Opens a value sealed under a data-encryption key.
    fn open_under(
        space_key: &[u8; 32],
        sealed: &SealedSecret,
    ) -> Result<Vec<u8>, boma_app::AppError> {
        let nonce =
            <&aes_gcm::Nonce<aes_gcm::aead::consts::U12>>::try_from(sealed.nonce.as_slice())
                .map_err(|_| boma_app::AppError::Unavailable)?;
        let aead = data_key_cipher(space_key)?;
        aead.decrypt(nonce, sealed.ciphertext.as_ref())
            .map_err(|_| boma_app::AppError::Unavailable)
    }

    /// Seals `value` under a data-encryption key; the caller (a store) owns the DEK.
    pub(crate) fn seal_with_data_key(
        space_key: &[u8; 32],
        value: &SecretString,
    ) -> Result<SealedSecret, boma_app::AppError> {
        Self::seal_under_with_nonce(space_key, value.expose_secret().as_bytes())
    }

    /// Opens a sealed value under the given DEK.
    pub(crate) fn open_with_data_key(
        space_key: &[u8; 32],
        sealed: &SealedSecret,
    ) -> Result<SecretString, boma_app::AppError> {
        let plaintext = Self::open_under(space_key, sealed)?;
        String::from_utf8(plaintext)
            .map(SecretString::from)
            .map_err(|_| boma_app::AppError::Unavailable)
    }

    /// Wraps a data-encryption key under the master key.
    pub(crate) fn wrap_data_key(
        &self,
        space_key: &[u8; 32],
    ) -> Result<SealedSecret, boma_app::AppError> {
        Self::seal_under_with_nonce(&self.master, space_key)
    }

    /// Unwraps a data-encryption key with the master key.
    pub(crate) fn unwrap_data_key(
        &self,
        wrapped: &SealedSecret,
    ) -> Result<[u8; 32], boma_app::AppError> {
        let plaintext = Self::open_under(&self.master, wrapped)?;
        <[u8; 32]>::try_from(plaintext.as_slice()).map_err(|_| boma_app::AppError::Unavailable)
    }

    /// The data-encryption key a new space gets.
    #[must_use]
    pub(crate) fn fresh_data_key() -> [u8; 32] {
        fresh_data_key()
    }
}

fn fresh_data_key() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    #[allow(clippy::expect_used)]
    getrandom::fill(&mut bytes).expect("the OS provides random bytes");
    bytes
}

fn data_key_cipher(key: &[u8; 32]) -> Result<Aes256Gcm, boma_app::AppError> {
    Aes256Gcm::new_from_slice(key).map_err(|_| boma_app::AppError::Unavailable)
}

/// Why a vault refused the work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretBoxError {
    /// The `BOMA_MASTER_KEY` value is not exactly 64 hex characters.
    InvalidKey,
}

impl std::fmt::Display for SecretBoxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey => write!(f, "BOMA_MASTER_KEY must be exactly 64 hex characters"),
        }
    }
}

impl std::error::Error for SecretBoxError {}

/// Decodes a hex string into exactly `LEN` bytes, refusing anything else.
fn hex_decode<const LEN: usize>(value: &str) -> Option<[u8; LEN]> {
    if value.len() != LEN * 2 {
        return None;
    }
    let mut out = [0u8; LEN];
    for index in 0..LEN {
        out[index] = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(key: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("BOMA_MASTER_KEY".to_owned(), key.to_owned())])
    }

    #[test]
    fn req_046_a_wrapped_data_key_round_trips() {
        let vault = SecretBox::from_env(&env(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        ))
        .expect("the key is valid");
        let dek = SecretBox::fresh_data_key();
        let wrapped = vault.wrap_data_key(&dek).expect("the wrap round-trips");
        let unwrapped = vault
            .unwrap_data_key(&wrapped)
            .expect("the unwrap round-trips");
        assert_eq!(dek, unwrapped);
    }

    #[test]
    fn req_046_a_secret_sealed_under_a_dek_round_trips() {
        let dek = SecretBox::fresh_data_key();
        let value = SecretString::from("s3cr3t-token-value".to_owned());
        let sealed = SecretBox::seal_with_data_key(&dek, &value).expect("the seal round-trips");
        let opened = SecretBox::open_with_data_key(&dek, &sealed).expect("the open round-trips");
        assert_eq!(opened.expose_secret(), "s3cr3t-token-value");
    }

    #[test]
    fn req_046_two_seals_of_the_same_value_differ() {
        let dek = SecretBox::fresh_data_key();
        let value = SecretString::from("same-value".to_owned());
        let first = SecretBox::seal_with_data_key(&dek, &value).expect("the seal round-trips");
        let second = SecretBox::seal_with_data_key(&dek, &value).expect("the seal round-trips");
        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.ciphertext, second.ciphertext);
    }

    #[test]
    fn req_046_a_tampered_ciphertext_is_refused() {
        let vault = SecretBox::ephemeral();
        let dek = SecretBox::fresh_data_key();
        let value = SecretString::from("the value".to_owned());
        let mut sealed = SecretBox::seal_with_data_key(&dek, &value).expect("the seal round-trips");
        sealed.ciphertext[0] ^= 0x01;
        assert!(SecretBox::open_with_data_key(&dek, &sealed).is_err());
        let wrapped = vault.wrap_data_key(&dek).expect("the wrap round-trips");
        let mut tampered = wrapped.clone();
        tampered.nonce[0] ^= 0x01;
        assert!(vault.unwrap_data_key(&tampered).is_err());
    }

    #[test]
    fn req_046_an_env_key_without_hex_is_refused() {
        let outcome = SecretBox::from_env(&env("not-hex-at-all"));
        assert_eq!(outcome.err(), Some(SecretBoxError::InvalidKey));
        let outcome = SecretBox::from_env(&env("0011"));
        assert_eq!(outcome.err(), Some(SecretBoxError::InvalidKey));
    }

    #[test]
    fn req_046_an_empty_env_key_is_treated_as_absent() {
        use std::collections::BTreeMap;

        let vault = SecretBox::from_env(&BTreeMap::from([(
            "BOMA_MASTER_KEY".to_owned(),
            String::new(),
        )]))
        .expect("an empty value is absent, not malformed");
        assert!(!vault.stable_key());
        let vault = SecretBox::from_env(&BTreeMap::from([(
            "BOMA_MASTER_KEY".to_owned(),
            "   ".to_owned(),
        )]))
        .expect("a whitespace value is absent, not malformed");
        assert!(!vault.stable_key());
    }

    #[test]
    fn req_046_a_rotated_master_key_still_reads_old_ciphertext() {
        // The rotation procedure: unwrap every DEK with the old master key, re-wrap with the
        // new one; secrets are never re-encrypted, so their ciphertexts are untouched.
        let old_env = env("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let new_env = env("1f1e1d1c1b1a191817161514131211100f0e0d0c0b0a09080706050403020100");
        let old = SecretBox::from_env(&old_env).expect("the key is valid");
        let dek = SecretBox::fresh_data_key();
        let wrapped_old = old.wrap_data_key(&dek).expect("the wrap round-trips");
        let value = SecretString::from("long-lived-credential".to_owned());
        let sealed = SecretBox::seal_with_data_key(&dek, &value).expect("the seal round-trips");

        let new = SecretBox::from_env(&new_env).expect("the key is valid");
        let rewrapped = new
            .wrap_data_key(
                &old.unwrap_data_key(&wrapped_old)
                    .expect("the unwrap round-trips"),
            )
            .expect("the re-wrap round-trips");
        let dek = new
            .unwrap_data_key(&rewrapped)
            .expect("the unwrap round-trips");
        let opened = SecretBox::open_with_data_key(&dek, &sealed).expect("the secret still opens");
        assert_eq!(opened.expose_secret(), "long-lived-credential");
    }
}
