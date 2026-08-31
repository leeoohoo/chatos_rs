// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use super::*;

const PLUGIN_FILE_GRANT_TTL_MILLISECONDS: u64 = 10 * 60 * 1_000;
const PLUGIN_FILE_GRANT_MAX_BYTES: u64 = 128 * 1024 * 1024;
const PLUGIN_FILE_GRANT_MAX_FILES: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct PluginFileGrantSummary {
    pub file_grant_id: String,
    pub display_name: String,
    pub size: u64,
    pub sha256: String,
    pub expires_at_unix_ms: u64,
}

#[derive(Debug, Serialize)]
struct PluginFileGrantDescriptor<'a> {
    path: &'a str,
    expires_at_unix_ms: u64,
    size: u64,
    sha256: &'a str,
}

impl PluginRuntimeHost {
    pub fn create_file_grants(
        &self,
        adapter_session_id: &str,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<PluginFileGrantSummary>> {
        if paths.is_empty() || paths.len() > PLUGIN_FILE_GRANT_MAX_FILES {
            bail!("Plugin File Grant request must contain between 1 and 20 files");
        }
        let session = self
            .sessions()
            .map_err(|(_status, message)| anyhow::anyhow!(message))?
            .get(adapter_session_id)
            .cloned()
            .context("Plugin Runtime Session is unavailable")?;
        if session.expires_at <= Utc::now().timestamp() {
            bail!("Plugin Runtime Session has expired");
        }
        let mcp = session
            .mcp
            .as_ref()
            .context("Plugin Runtime Session is not an MCP session")?;
        mcp.validate_active()?;
        let grant_dir = mcp.file_grant_dir()?.to_path_buf();
        validate_private_session_directory(grant_dir.as_path())?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let expires_at_unix_ms = now_ms.saturating_add(PLUGIN_FILE_GRANT_TTL_MILLISECONDS);
        let mut summaries = Vec::with_capacity(paths.len());
        for requested_path in paths {
            let canonical = requested_path
                .canonicalize()
                .with_context(|| format!("resolve selected file: {}", requested_path.display()))?;
            let metadata = fs::symlink_metadata(canonical.as_path())
                .with_context(|| format!("inspect selected file: {}", canonical.display()))?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > PLUGIN_FILE_GRANT_MAX_BYTES
            {
                bail!("selected Plugin file is not a regular file or exceeds 128 MiB");
            }
            let sha256 = hash_regular_file(canonical.as_path(), metadata.len())?;
            let file_grant_id = format!("fg_{}", Uuid::new_v4().simple());
            let descriptor_path = grant_dir.join(format!("{file_grant_id}.json"));
            let canonical_text = canonical
                .to_str()
                .context("selected Plugin file path is not valid UTF-8")?;
            let descriptor = serde_json::to_vec(&PluginFileGrantDescriptor {
                path: canonical_text,
                expires_at_unix_ms,
                size: metadata.len(),
                sha256: sha256.as_str(),
            })?;
            write_private_descriptor(grant_dir.as_path(), descriptor_path.as_path(), &descriptor)?;
            summaries.push(PluginFileGrantSummary {
                file_grant_id,
                display_name: canonical
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("selected-file")
                    .to_string(),
                size: metadata.len(),
                sha256,
                expires_at_unix_ms,
            });
        }
        Ok(summaries)
    }
}

fn validate_private_session_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect Plugin File Grant directory: {}", path.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("Plugin File Grant directory is invalid");
    }
    Ok(())
}

fn hash_regular_file(path: &Path, expected_size: u64) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("open selected Plugin file: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > PLUGIN_FILE_GRANT_MAX_BYTES {
            bail!("selected Plugin file exceeds 128 MiB while hashing");
        }
        hasher.update(&buffer[..read]);
    }
    if total != expected_size {
        bail!("selected Plugin file changed while creating the grant");
    }
    Ok(hex::encode(hasher.finalize()))
}

fn write_private_descriptor(directory: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
    if destination.parent() != Some(directory) || destination.exists() {
        bail!("Plugin File Grant descriptor path is invalid");
    }
    let mut temporary = NamedTempFile::new_in(directory)?;
    std::io::Write::write_all(&mut temporary, bytes)?;
    temporary.as_file().sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temporary
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn hashes_exact_regular_file_and_rejects_size_drift() {
        let temp = TempDir::new().expect("temp directory");
        let selected = temp.path().join("selected.txt");
        fs::write(selected.as_path(), b"selected-file").expect("write selected file");
        assert_eq!(
            hash_regular_file(selected.as_path(), 13).expect("hash selected file"),
            hex::encode(Sha256::digest(b"selected-file"))
        );
        assert!(hash_regular_file(selected.as_path(), 12).is_err());
    }

    #[test]
    fn descriptor_creation_is_private_and_never_overwrites() {
        let temp = TempDir::new().expect("temp directory");
        let destination = temp.path().join("fg_fixture.json");
        write_private_descriptor(temp.path(), destination.as_path(), br#"{"size":1}"#)
            .expect("write descriptor");
        assert_eq!(
            fs::read(destination.as_path()).expect("read descriptor"),
            br#"{"size":1}"#
        );
        assert!(
            write_private_descriptor(temp.path(), destination.as_path(), br#"{"size":2}"#).is_err()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(destination.as_path())
                    .expect("descriptor metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_session_directory_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("temp directory");
        let directory = temp.path().join("grants");
        fs::create_dir(directory.as_path()).expect("create grants directory");
        let linked = temp.path().join("linked-grants");
        symlink(directory.as_path(), linked.as_path()).expect("create directory symlink");
        validate_private_session_directory(directory.as_path()).expect("real directory");
        assert!(validate_private_session_directory(linked.as_path()).is_err());
    }
}
