// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) fn copy_workspace_snapshot(source: &str, destination: &str) -> Result<(), String> {
    let source = fs::canonicalize(source)
        .map_err(|err| format!("read source workspace {source} failed: {err}"))?;
    let destination = PathBuf::from(destination);
    fs::create_dir_all(&destination).map_err(|err| {
        format!(
            "create workspace snapshot destination {} failed: {err}",
            destination.display()
        )
    })?;
    clear_directory(destination.as_path(), false)?;
    copy_directory_contents(source.as_path(), destination.as_path(), source.as_path())
}

pub(super) fn replace_git_worktree_with_workspace(
    source: &str,
    worktree: &Path,
) -> Result<(), String> {
    let source = fs::canonicalize(source)
        .map_err(|err| format!("read source workspace {source} failed: {err}"))?;
    fs::create_dir_all(worktree)
        .map_err(|err| format!("create git worktree {} failed: {err}", worktree.display()))?;
    clear_directory(worktree, true)?;
    copy_directory_contents(source.as_path(), worktree, source.as_path())
}

fn clear_directory(path: &Path, preserve_git: bool) -> Result<(), String> {
    for entry in fs::read_dir(path)
        .map_err(|err| format!("read directory {} failed: {err}", path.display()))?
    {
        let entry = entry.map_err(|err| format!("read directory entry failed: {err}"))?;
        if preserve_git && entry.file_name() == ".git" {
            continue;
        }
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path)
            .map_err(|err| format!("read metadata {} failed: {err}", entry_path.display()))?;
        if metadata.is_dir() {
            fs::remove_dir_all(&entry_path).map_err(|err| {
                format!("remove directory {} failed: {err}", entry_path.display())
            })?;
        } else {
            fs::remove_file(&entry_path)
                .map_err(|err| format!("remove file {} failed: {err}", entry_path.display()))?;
        }
    }
    Ok(())
}

fn copy_directory_contents(source: &Path, destination: &Path, root: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source)
        .map_err(|err| format!("read directory {} failed: {err}", source.display()))?
    {
        let entry = entry.map_err(|err| format!("read directory entry failed: {err}"))?;
        let source_path = entry.path();
        if should_skip_workspace_entry(root, source_path.as_path()) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|err| format!("read file type {} failed: {err}", source_path.display()))?;
        let dest_path = destination.join(entry.file_name());
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            fs::create_dir_all(&dest_path)
                .map_err(|err| format!("create directory {} failed: {err}", dest_path.display()))?;
            copy_directory_contents(source_path.as_path(), dest_path.as_path(), root)?;
        } else if file_type.is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    format!("create directory {} failed: {err}", parent.display())
                })?;
            }
            fs::copy(&source_path, &dest_path).map_err(|err| {
                format!(
                    "copy file {} to {} failed: {err}",
                    source_path.display(),
                    dest_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn should_skip_workspace_entry(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    relative.components().any(|component| {
        matches!(
            component,
            Component::Normal(name)
                if name == ".git"
                    || name == ".chatos"
                    || name == ".task-runner"
                    || name == ".task_runner"
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use uuid::Uuid;

    use super::{copy_workspace_snapshot, replace_git_worktree_with_workspace};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "chatos-workspace-snapshot-test-{label}-{}",
                Uuid::new_v4()
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            self.0.as_path()
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn snapshot_skips_internal_directories() {
        let root = TestDirectory::new("skip");
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir_all(source.join("src")).expect("create source directory");
        fs::write(source.join("src/main.rs"), "fn main() {}\n").expect("write source file");
        for skipped in [".git", ".chatos", ".task-runner", ".task_runner"] {
            fs::create_dir_all(source.join(skipped)).expect("create skipped directory");
            fs::write(source.join(skipped).join("internal.txt"), "ignored")
                .expect("write skipped file");
        }

        copy_workspace_snapshot(
            source.to_string_lossy().as_ref(),
            destination.to_string_lossy().as_ref(),
        )
        .expect("copy workspace snapshot");

        assert!(destination.join("src/main.rs").is_file());
        assert!(!destination.join(".git").exists());
        assert!(!destination.join(".chatos").exists());
        assert!(!destination.join(".task-runner").exists());
        assert!(!destination.join(".task_runner").exists());
    }

    #[test]
    fn replacing_worktree_preserves_git_metadata_and_removes_stale_files() {
        let root = TestDirectory::new("replace");
        let source = root.path().join("source");
        let worktree = root.path().join("worktree");
        fs::create_dir_all(&source).expect("create source directory");
        fs::create_dir_all(worktree.join(".git")).expect("create git metadata directory");
        fs::write(source.join("fresh.txt"), "fresh").expect("write source file");
        fs::write(worktree.join(".git/config"), "metadata").expect("write git metadata");
        fs::write(worktree.join("stale.txt"), "stale").expect("write stale file");

        replace_git_worktree_with_workspace(source.to_string_lossy().as_ref(), &worktree)
            .expect("replace git worktree");

        assert!(worktree.join("fresh.txt").is_file());
        assert!(!worktree.join("stale.txt").exists());
        assert_eq!(
            fs::read_to_string(worktree.join(".git/config")).expect("read git metadata"),
            "metadata"
        );
    }
}
