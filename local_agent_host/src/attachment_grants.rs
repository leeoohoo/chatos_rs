// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::{LocalAttachmentLocator, LocalAttachmentResolver};

const GRANT_PREFIX: &str = "attachment-grant:";
const MINIMUM_GRANT_ID_BYTES: usize = 16;
const MAXIMUM_GRANT_ID_BYTES: usize = 128;
const MAXIMUM_ATTACHMENT_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LocalAttachmentGrantError {
    #[error("attachment grant root must be an existing private absolute directory")]
    InvalidRoot,
    #[error("attachment payload reference is not a valid opaque grant")]
    InvalidGrant,
    #[error("attachment grant payload is unavailable")]
    Unavailable,
    #[error("attachment grant payload metadata does not match the frozen reference")]
    IntegrityMismatch,
}

/// Resolves copied, client-owned visual payloads from a private grant
/// directory. Neither the model nor IPC caller can supply a filesystem path;
/// each payload is addressed only by an opaque, single-segment grant ID.
pub struct LocalAttachmentGrantResolver {
    root: PathBuf,
}

impl fmt::Debug for LocalAttachmentGrantResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalAttachmentGrantResolver")
            .field("root", &"[PRIVATE DIRECTORY]")
            .finish()
    }
}

impl LocalAttachmentGrantResolver {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, LocalAttachmentGrantError> {
        let root = root.as_ref();
        if !root.is_absolute() {
            return Err(LocalAttachmentGrantError::InvalidRoot);
        }
        let link_metadata =
            std::fs::symlink_metadata(root).map_err(|_| LocalAttachmentGrantError::InvalidRoot)?;
        if !link_metadata.is_dir() || link_metadata.file_type().is_symlink() {
            return Err(LocalAttachmentGrantError::InvalidRoot);
        }
        verify_private_root(&link_metadata)?;
        let canonical =
            std::fs::canonicalize(root).map_err(|_| LocalAttachmentGrantError::InvalidRoot)?;
        Ok(Self { root: canonical })
    }

    pub fn payload_path(
        &self,
        payload_reference: &str,
    ) -> Result<PathBuf, LocalAttachmentGrantError> {
        let grant_id = validate_grant(payload_reference)?;
        Ok(self.root.join(format!("{grant_id}.payload")))
    }

    fn resolve_sync(
        &self,
        attachment: &LocalAttachmentLocator,
    ) -> Result<Vec<u8>, LocalAttachmentGrantError> {
        if attachment.byte_size == 0 || attachment.byte_size > MAXIMUM_ATTACHMENT_BYTES {
            return Err(LocalAttachmentGrantError::IntegrityMismatch);
        }
        let path = self.payload_path(attachment.payload_reference.as_str())?;
        let mut file = open_payload(path.as_path())?;
        let metadata = file
            .metadata()
            .map_err(|_| LocalAttachmentGrantError::Unavailable)?;
        if !metadata.is_file() || metadata.len() != attachment.byte_size {
            return Err(LocalAttachmentGrantError::IntegrityMismatch);
        }
        let capacity = usize::try_from(attachment.byte_size)
            .map_err(|_| LocalAttachmentGrantError::IntegrityMismatch)?;
        let mut bytes = Vec::with_capacity(capacity);
        file.by_ref()
            .take(MAXIMUM_ATTACHMENT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| LocalAttachmentGrantError::Unavailable)?;
        if bytes.len() != capacity
            || format!("sha256:{:x}", Sha256::digest(bytes.as_slice())) != attachment.payload_digest
        {
            return Err(LocalAttachmentGrantError::IntegrityMismatch);
        }
        Ok(bytes)
    }
}

#[async_trait]
impl LocalAttachmentResolver for LocalAttachmentGrantResolver {
    async fn resolve(&self, attachment: &LocalAttachmentLocator) -> Result<Vec<u8>, String> {
        self.resolve_sync(attachment)
            .map_err(|error| error.to_string())
    }
}

fn validate_grant(reference: &str) -> Result<&str, LocalAttachmentGrantError> {
    let id = reference
        .strip_prefix(GRANT_PREFIX)
        .ok_or(LocalAttachmentGrantError::InvalidGrant)?;
    if !(MINIMUM_GRANT_ID_BYTES..=MAXIMUM_GRANT_ID_BYTES).contains(&id.len())
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(LocalAttachmentGrantError::InvalidGrant);
    }
    Ok(id)
}

#[cfg(unix)]
fn verify_private_root(metadata: &std::fs::Metadata) -> Result<(), LocalAttachmentGrantError> {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: geteuid has no preconditions and does not dereference memory.
    let current_uid = unsafe { libc::geteuid() };
    if metadata.uid() != current_uid || metadata.mode() & 0o077 != 0 {
        return Err(LocalAttachmentGrantError::InvalidRoot);
    }
    Ok(())
}

#[cfg(windows)]
fn verify_private_root(_metadata: &std::fs::Metadata) -> Result<(), LocalAttachmentGrantError> {
    // Windows launchers create this directory beneath the current user's app
    // data root with an owner-only DACL. Reparse points are rejected above and
    // again on the opened payload handle below.
    Ok(())
}

#[cfg(unix)]
fn open_payload(path: &Path) -> Result<File, LocalAttachmentGrantError> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| LocalAttachmentGrantError::Unavailable)
}

#[cfg(windows)]
fn open_payload(path: &Path) -> Result<File, LocalAttachmentGrantError> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| LocalAttachmentGrantError::Unavailable)?;
    if file
        .metadata()
        .map_err(|_| LocalAttachmentGrantError::Unavailable)?
        .file_type()
        .is_symlink()
    {
        return Err(LocalAttachmentGrantError::Unavailable);
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRANT_ID: &str = "grant_0123456789abcdef";

    fn private_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        root
    }

    fn locator(bytes: &[u8]) -> LocalAttachmentLocator {
        LocalAttachmentLocator {
            attachment_id: "attachment-1".to_string(),
            media_type: "image/png".to_string(),
            payload_reference: format!("{GRANT_PREFIX}{GRANT_ID}"),
            payload_digest: format!("sha256:{:x}", Sha256::digest(bytes)),
            byte_size: bytes.len() as u64,
        }
    }

    #[tokio::test]
    async fn reads_only_an_integrity_checked_opaque_grant() {
        let root = private_root();
        let bytes = b"frozen visual payload";
        std::fs::write(root.path().join(format!("{GRANT_ID}.payload")), bytes).unwrap();
        let resolver = LocalAttachmentGrantResolver::open(root.path()).unwrap();

        assert_eq!(resolver.resolve(&locator(bytes)).await.unwrap(), bytes);
    }

    #[test]
    fn rejects_paths_and_malformed_grant_identifiers() {
        let root = private_root();
        let resolver = LocalAttachmentGrantResolver::open(root.path()).unwrap();
        for reference in [
            "/tmp/payload",
            "attachment-grant:../payload",
            "attachment-grant:segment/payload",
            "attachment-grant:short",
            "attachment-grant:grant id with spaces",
            &format!(
                "attachment-grant:{}",
                "a".repeat(MAXIMUM_GRANT_ID_BYTES + 1)
            ),
        ] {
            assert_eq!(
                resolver.payload_path(reference).unwrap_err(),
                LocalAttachmentGrantError::InvalidGrant
            );
        }
        assert!(resolver
            .payload_path(&format!(
                "attachment-grant:{}",
                "a".repeat(MINIMUM_GRANT_ID_BYTES)
            ))
            .is_ok());
        assert!(resolver
            .payload_path(&format!(
                "attachment-grant:{}",
                "z".repeat(MAXIMUM_GRANT_ID_BYTES)
            ))
            .is_ok());
    }

    #[tokio::test]
    async fn rejects_size_digest_and_limit_mismatches() {
        let root = private_root();
        let bytes = b"payload";
        std::fs::write(root.path().join(format!("{GRANT_ID}.payload")), bytes).unwrap();
        let resolver = LocalAttachmentGrantResolver::open(root.path()).unwrap();

        let mut wrong_size = locator(bytes);
        wrong_size.byte_size += 1;
        assert!(resolver.resolve(&wrong_size).await.is_err());

        let mut wrong_digest = locator(bytes);
        wrong_digest.payload_digest = format!("sha256:{}", "0".repeat(64));
        assert!(resolver.resolve(&wrong_digest).await.is_err());

        let mut oversized = locator(bytes);
        oversized.byte_size = MAXIMUM_ATTACHMENT_BYTES + 1;
        assert!(resolver.resolve(&oversized).await.is_err());
    }

    #[test]
    fn debug_output_never_discloses_the_private_root() {
        let root = private_root();
        let resolver = LocalAttachmentGrantResolver::open(root.path()).unwrap();
        let rendered = format!("{resolver:?}");
        assert!(!rendered.contains(root.path().to_str().unwrap()));
        assert!(rendered.contains("[PRIVATE DIRECTORY]"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_private_roots_and_symlink_payloads() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = private_root();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            LocalAttachmentGrantResolver::open(root.path()).unwrap_err(),
            LocalAttachmentGrantError::InvalidRoot
        );

        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let target = root.path().join("target");
        std::fs::write(&target, b"payload").unwrap();
        symlink(&target, root.path().join(format!("{GRANT_ID}.payload"))).unwrap();
        let resolver = LocalAttachmentGrantResolver::open(root.path()).unwrap();
        assert!(resolver.resolve_sync(&locator(b"payload")).is_err());
    }
}
