// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::workspace::paths::{canonicalize_existing_dir, normalize_relative_workspace_path};
use crate::WorkspaceState;

pub(crate) fn create_workspace_directory(
    workspace: &WorkspaceState,
    requested_path: &str,
) -> Result<String> {
    let normalized = normalize_relative_workspace_path(requested_path)?;
    if normalized == "." {
        anyhow::bail!("directory path must not be the workspace root");
    }
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let mut current = root;
    for component in Path::new(normalized.as_str()).components() {
        let std::path::Component::Normal(segment) = component else {
            anyhow::bail!("directory path contains an unsupported component");
        };
        current.push(segment);
        match fs::symlink_metadata(current.as_path()) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!("directory path crosses a symbolic link");
            }
            Ok(metadata) if !metadata.is_dir() => {
                anyhow::bail!("directory path contains a non-directory entry");
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(current.as_path())?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::create_workspace_directory;
    use crate::WorkspaceState;

    fn workspace(root: PathBuf) -> WorkspaceState {
        WorkspaceState {
            id: "workspace-1".to_string(),
            absolute_root: root,
            alias: "work".to_string(),
            fingerprint: "fingerprint".to_string(),
            project_config_trust: None,
        }
    }

    #[test]
    fn creates_directories_relative_to_the_authorized_workspace() {
        let root = std::env::temp_dir().join(format!(
            "chatos-local-workspace-directories-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("apps/backend")).expect("create test directories");
        let workspace = workspace(root.canonicalize().expect("canonical workspace"));

        let created =
            create_workspace_directory(&workspace, "apps/frontend/src").expect("create directory");
        assert_eq!(created, "apps/frontend/src");
        assert!(root.join("apps/frontend/src").is_dir());
        assert!(create_workspace_directory(&workspace, "../outside").is_err());

        let _ = std::fs::remove_dir_all(root);
    }
}
