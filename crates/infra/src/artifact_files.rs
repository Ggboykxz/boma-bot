//! The out-of-band artifact bytes (REQ-036): a two-level content-addressed filesystem store.
//!
//! `storage_key` is `sha256:<64 hex>`; the first two hex characters pick the directory, the rest
//! names the file. Writes are idempotent — identical bytes produce identical paths, so a retried
//! upload never doubles the storage. Removal is best-effort with loud errors; the caller removes
//! only blobs no row references anymore.

use std::path::{Path, PathBuf};

use boma_app::{
    AppError,
    ports::{ArtifactContentStore, ContentWrite},
};
use sha2::{Digest, Sha256};
use tokio::fs;

/// The content-addressed filesystem store.
#[derive(Debug, Clone)]
pub struct FileArtifactContentStore {
    root: PathBuf,
}

impl FileArtifactContentStore {
    /// Builds the store on a root directory, creating it when absent.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the root cannot be created.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, AppError> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(|error| {
            tracing::warn!(%error, path = %root.display(), "the artifact directory could not be created");
            AppError::Unavailable
        })?;
        Ok(Self { root })
    }

    /// The path one key maps to, validating the key shape.
    fn path_for(&self, key: &str) -> Result<PathBuf, AppError> {
        let hex = key
            .strip_prefix("sha256:")
            .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| {
                AppError::Rule(boma_domain::DomainError::InvalidName(
                    "the storage key is not `sha256:<64 hex>`".to_owned(),
                ))
            })?;
        let (dir, file) = hex.split_at(2);
        Ok(self.root.join(dir).join(file))
    }
}

#[async_trait::async_trait]
impl ArtifactContentStore for FileArtifactContentStore {
    async fn put(&self, bytes: &[u8]) -> Result<ContentWrite, AppError> {
        let mut digest = String::with_capacity(64);
        for byte in Sha256::digest(bytes) {
            use std::fmt::Write as _;
            let _ = write!(&mut digest, "{byte:02x}");
        }
        let key = format!("sha256:{digest}");
        let path = self.path_for(&key)?;
        if path.exists() {
            // Same bytes, same home: nothing to write.
            return Ok(ContentWrite {
                hash: key,
                size: bytes.len() as u64,
            });
        }
        let Some(dir) = path.parent().map(Path::to_path_buf) else {
            return Err(AppError::Unavailable);
        };
        fs::create_dir_all(&dir)
            .await
            .map_err(|_| AppError::Unavailable)?;
        // Write to a temp sibling and rename, so a crash never leaves half a blob.
        let tmp = dir.join(format!(".boma-tmp-{}", uuid::Uuid::new_v4()));
        fs::write(&tmp, bytes)
            .await
            .map_err(|_| AppError::Unavailable)?;
        fs::rename(&tmp, &path)
            .await
            .map_err(|_| AppError::Unavailable)?;
        Ok(ContentWrite {
            hash: key,
            size: bytes.len() as u64,
        })
    }

    async fn read(&self, key: &str) -> Result<Vec<u8>, AppError> {
        let path = self.path_for(key)?;
        fs::read(&path).await.map_err(|_| {
            AppError::Rule(boma_domain::DomainError::InvalidName(
                "the artifact bytes are missing".to_owned(),
            ))
        })
    }

    async fn remove(&self, key: &str) -> Result<(), AppError> {
        let path = self.path_for(key)?;
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(AppError::Unavailable),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FileArtifactContentStore;
    use boma_app::ports::ArtifactContentStore;

    fn tempdir(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn req_036_the_store_is_content_addressed_and_idempotent() {
        let root = tempdir("boma-artifact-test");
        let store = FileArtifactContentStore::new(&root).expect("the root is created");

        let bytes = b"# hello\n".to_vec();
        let first = tokio::runtime::Runtime::new()
            .expect("a runtime")
            .block_on(store.put(&bytes))
            .expect("the write lands");
        let second = tokio::runtime::Runtime::new()
            .expect("a runtime")
            .block_on(store.put(&bytes))
            .expect("the write lands again");
        assert_eq!(first, second, "identical bytes hash identically");
        assert!(first.hash.starts_with("sha256:"));

        let read = tokio::runtime::Runtime::new()
            .expect("a runtime")
            .block_on(store.read(&first.hash))
            .expect("the blob reads back");
        assert_eq!(read, bytes);

        tokio::runtime::Runtime::new()
            .expect("a runtime")
            .block_on(store.remove(&first.hash))
            .expect("the blob removes");
        assert!(
            tokio::runtime::Runtime::new()
                .expect("a runtime")
                .block_on(store.read(&first.hash))
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn d03_a_key_that_is_not_a_hash_is_refused() {
        let root = tempdir("boma-artifact-test-key");
        let store = FileArtifactContentStore::new(&root).expect("the root is created");
        let path = store.path_for("../../etc/passwd");
        assert!(path.is_err());
        let path = store.path_for("sha256:nothex");
        assert!(path.is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
