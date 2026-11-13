//! Shared envelope primitives: per-space data-encryption keys wrapped under the master key.
//!
//! Same scheme the secrets store uses (ADR-020): every space owns a random 256-bit DEK, wrapped
//! (AES-256-GCM) under `BOMA_MASTER_KEY`; a value is sealed under its space's DEK. This module
//! holds only the mechanics; the secrets store has its own inline copy kept stable for the
//! recorded path — both operate on the same `space_keys` table.

use boma_app::{AppError, ports::SealedSecret};
use secrecy::SecretString;
use sqlx::PgConnection;

use crate::{secret_box::SecretBox, tenant};

/// Loads the space's DEK, creating and wrapping it on first use.
///
/// The load-or-create is idempotent: concurrent creators racing the same space settle on the
/// row that survives the `ON CONFLICT DO NOTHING`.
///
/// # Errors
///
/// Returns an error when the row does not unwrap under this deployment's master key, or the
/// database cannot answer.
pub(crate) async fn load_or_create_dek(
    conn: &mut PgConnection,
    secrets: &SecretBox,
    space_id: &str,
) -> Result<[u8; 32], AppError> {
    let existing: Option<(Vec<u8>, Vec<u8>)> =
        sqlx::query_as("SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1")
            .bind(space_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(tenant::unavailable)?;
    if let Some((wrapped_key, nonce)) = existing {
        return secrets.unwrap_data_key(&SealedSecret {
            ciphertext: wrapped_key,
            nonce,
        });
    }
    let dek = SecretBox::fresh_data_key();
    let sealed_dek = secrets.wrap_data_key(&dek)?;
    sqlx::query(
        "INSERT INTO space_keys (space_id, wrapped_key, nonce)
         VALUES ($1, $2, $3) ON CONFLICT (space_id) DO NOTHING",
    )
    .bind(space_id)
    .bind(&sealed_dek.ciphertext)
    .bind(&sealed_dek.nonce)
    .execute(&mut *conn)
    .await
    .map_err(tenant::unavailable)?;
    let survivor: (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1")
            .bind(space_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(tenant::unavailable)?;
    secrets.unwrap_data_key(&SealedSecret {
        ciphertext: survivor.0,
        nonce: survivor.1,
    })
}

/// Seals a value under the space's DEK.
///
/// # Errors
///
/// Returns an error when the DEK cannot be loaded or the seal fails.
pub(crate) async fn seal_for_space(
    conn: &mut PgConnection,
    secrets: &SecretBox,
    space_id: &str,
    value: &SecretString,
) -> Result<SealedSecret, AppError> {
    let dek = load_or_create_dek(conn, secrets, space_id).await?;
    SecretBox::seal_with_data_key(&dek, value)
}

/// Opens a value sealed under the space's DEK.
///
/// # Errors
///
/// Returns an error when the DEK cannot be loaded, the seal does not authenticate, or the
/// plaintext is not UTF-8.
pub(crate) async fn open_for_space(
    conn: &mut PgConnection,
    secrets: &SecretBox,
    space_id: &str,
    sealed: &SealedSecret,
) -> Result<SecretString, AppError> {
    let dek = load_or_create_dek(conn, secrets, space_id).await?;
    SecretBox::open_with_data_key(&dek, sealed)
}
