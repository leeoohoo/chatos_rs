// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};

use crate::WorkspaceState;

pub(crate) fn workspace_project_config_trust_fingerprint(path: &Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize workspace trust path {}", path.display()))?;
    if !canonical.is_dir() {
        return Err(anyhow!(
            "workspace trust path is not a directory: {}",
            canonical.display()
        ));
    }
    workspace_identity_fingerprint(canonical.as_path())
}

pub(crate) fn workspace_project_config_trust_is_current(workspace: &WorkspaceState) -> bool {
    let Some(trust) = workspace.project_config_trust.as_ref() else {
        return false;
    };
    workspace_project_config_trust_fingerprint(workspace.absolute_root.as_path())
        .is_ok_and(|fingerprint| fingerprint == trust.identity_fingerprint)
}

#[cfg(unix)]
fn workspace_identity_fingerprint(canonical: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;

    let metadata = canonical
        .metadata()
        .with_context(|| format!("inspect workspace identity {}", canonical.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(b"chatos-workspace-trust-v1\0unix\0");
    hasher.update(canonical.to_string_lossy().as_bytes());
    hasher.update([0]);
    hasher.update(metadata.dev().to_le_bytes());
    hasher.update(metadata.ino().to_le_bytes());
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(not(unix))]
fn workspace_identity_fingerprint(_canonical: &Path) -> Result<String> {
    Err(anyhow!(
        "trusted project configuration is unavailable until stable directory identity checks are implemented on this operating system"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn replacing_directory_at_same_path_invalidates_trust() {
        let root = std::env::temp_dir().join(format!(
            "chatos-workspace-trust-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create trusted root");
        let first = workspace_project_config_trust_fingerprint(&root).expect("first identity");
        std::fs::remove_dir(&root).expect("remove trusted root");
        std::fs::create_dir(&root).expect("replace trusted root");
        let second = workspace_project_config_trust_fingerprint(&root).expect("second identity");

        assert_ne!(first, second);
        let _ = std::fs::remove_dir(&root);
    }
}
