// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceQuota {
    roots: Vec<PathBuf>,
    limit_bytes: Option<u64>,
}

impl WorkspaceQuota {
    pub(crate) fn new(root: PathBuf, limit_bytes: Option<u64>) -> Self {
        Self {
            roots: vec![root],
            limit_bytes: limit_bytes.filter(|value| *value > 0),
        }
    }

    pub(crate) fn with_extra_roots(mut self, roots: impl IntoIterator<Item = PathBuf>) -> Self {
        self.roots.extend(roots);
        self
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.limit_bytes.is_some()
    }

    pub(crate) fn check_sync(&self) -> Result<u64, String> {
        let mut used_bytes = 0u64;
        for root in &self.roots {
            used_bytes = used_bytes.saturating_add(directory_size_bytes(root.as_path())?);
        }
        if let Some(limit_bytes) = self.limit_bytes {
            if used_bytes > limit_bytes {
                return Err(format!(
                    "workspace disk limit exceeded: {used_bytes} bytes used > {limit_bytes} bytes allowed"
                ));
            }
        }
        Ok(used_bytes)
    }

    pub(crate) async fn check(&self) -> Result<u64, String> {
        let quota = self.clone();
        tokio::task::spawn_blocking(move || quota.check_sync())
            .await
            .map_err(|err| format!("workspace disk usage check failed: {err}"))?
    }
}

fn directory_size_bytes(root: &Path) -> Result<u64, String> {
    let mut total = 0u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = match std::fs::symlink_metadata(path.as_path()) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("inspect {} failed: {err}", path.display())),
        };
        if metadata.file_type().is_symlink() || metadata.is_file() {
            total = total.saturating_add(metadata.len());
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let entries = std::fs::read_dir(path.as_path())
            .map_err(|err| format!("read {} failed: {err}", path.display()))?;
        for entry in entries {
            let entry =
                entry.map_err(|err| format!("read {} entry failed: {err}", path.display()))?;
            pending.push(entry.path());
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_counts_nested_workspace_files() {
        let root = std::env::temp_dir().join(format!(
            "chatos-sandbox-quota-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("nested")).expect("create root");
        std::fs::write(root.join("one.bin"), vec![1u8; 8]).expect("write one");
        std::fs::write(root.join("nested/two.bin"), vec![2u8; 16]).expect("write two");

        let quota = WorkspaceQuota::new(root.clone(), Some(23));
        assert!(quota.check_sync().is_err());
        let quota = WorkspaceQuota::new(root.clone(), Some(24));
        assert_eq!(quota.check_sync().expect("within quota"), 24);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn quota_includes_native_sandbox_state_roots() {
        let root = std::env::temp_dir().join(format!(
            "chatos-sandbox-quota-extra-test-{}",
            uuid::Uuid::new_v4()
        ));
        let workspace = root.join("workspace");
        let state = root.join("state");
        std::fs::create_dir_all(&workspace).expect("workspace");
        std::fs::create_dir_all(&state).expect("state");
        std::fs::write(workspace.join("one.bin"), vec![1u8; 8]).expect("workspace file");
        std::fs::write(state.join("two.bin"), vec![2u8; 16]).expect("state file");

        let quota = WorkspaceQuota::new(workspace, Some(23)).with_extra_roots([state]);
        assert!(quota.check_sync().is_err());

        let _ = std::fs::remove_dir_all(root);
    }
}
