// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde::Serialize;

use crate::workspace::paths::{
    canonicalize_existing_dir, normalize_relative_workspace_path, relative_to_workspace,
    resolve_workspace_dir, resolve_workspace_path,
};
use crate::WorkspaceState;

const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SEARCH_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SEARCH_VISITS: usize = 20_000;
const SEARCH_DEADLINE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceDirectoryEntry {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
    pub(crate) modified_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceDirectoryListing {
    pub(crate) path: String,
    pub(crate) parent: Option<String>,
    pub(crate) entries: Vec<WorkspaceDirectoryEntry>,
}

pub(crate) fn list_workspace_directory(
    workspace: &WorkspaceState,
    requested_path: &str,
    include_files: bool,
) -> Result<WorkspaceDirectoryListing> {
    let directory = resolve_workspace_dir(workspace, requested_path)?;
    let path = relative_to_workspace(workspace, directory.as_path());
    let parent = directory
        .parent()
        .filter(|parent| parent.starts_with(workspace.absolute_root.as_path()))
        .map(|parent| relative_to_workspace(workspace, parent))
        .filter(|parent| parent != &path);
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory.as_path())? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if !include_files && !file_type.is_dir() {
            continue;
        }
        let metadata = entry.metadata()?;
        entries.push(WorkspaceDirectoryEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: relative_to_workspace(workspace, entry.path().as_path()),
            is_dir: file_type.is_dir(),
            size: metadata.len(),
            modified_at: modified_at_ms(&metadata),
        });
    }
    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(WorkspaceDirectoryListing {
        path,
        parent,
        entries,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceFileRead {
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) modified_at: Option<u64>,
    pub(crate) is_binary: bool,
    pub(crate) content: String,
}

pub(crate) fn read_workspace_file(
    workspace: &WorkspaceState,
    requested_path: &str,
) -> Result<WorkspaceFileRead> {
    let path = resolve_workspace_path(workspace, requested_path)?;
    let metadata = fs::metadata(path.as_path())?;
    if !metadata.is_file() {
        return Err(anyhow!("workspace path is not a file"));
    }
    if metadata.len() > MAX_PREVIEW_BYTES {
        return Err(anyhow!(
            "file is too large to preview: {} bytes (limit {})",
            metadata.len(),
            MAX_PREVIEW_BYTES
        ));
    }
    let bytes = fs::read(path.as_path())?;
    let is_binary = is_binary_buffer(bytes.as_slice());
    let content = if is_binary {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    } else {
        String::from_utf8_lossy(bytes.as_slice()).to_string()
    };
    Ok(WorkspaceFileRead {
        path: relative_to_workspace(workspace, path.as_path()),
        size: metadata.len(),
        modified_at: modified_at_ms(&metadata),
        is_binary,
        content,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceEntrySearch {
    pub(crate) matches: Vec<WorkspaceDirectoryEntry>,
    pub(crate) visited_dirs: usize,
    pub(crate) truncated: bool,
}

pub(crate) fn search_workspace_entries(
    workspace: &WorkspaceState,
    requested_path: &str,
    query: &str,
    limit: usize,
) -> Result<WorkspaceEntrySearch> {
    let root = resolve_workspace_dir(workspace, requested_path)?;
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Err(anyhow!("search query must not be empty"));
    }
    let limit = limit.clamp(1, 500);
    let started = Instant::now();
    let mut stack = vec![root];
    let mut matches = Vec::new();
    let mut visited_dirs = 0usize;
    let mut truncated = false;
    while let Some(directory) = stack.pop() {
        if started.elapsed() >= SEARCH_DEADLINE || visited_dirs >= MAX_SEARCH_VISITS {
            truncated = true;
            break;
        }
        visited_dirs += 1;
        let entries = match fs::read_dir(directory.as_path()) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if started.elapsed() >= SEARCH_DEADLINE || matches.len() >= limit {
                truncated = true;
                break;
            }
            let file_type = match entry.file_type() {
                Ok(file_type) if !file_type.is_symlink() => file_type,
                _ => continue,
            };
            let path = entry.path();
            let relative = relative_to_workspace(workspace, path.as_path());
            let name = entry.file_name().to_string_lossy().to_string();
            if file_type.is_dir() {
                stack.push(path.clone());
            }
            if !name.to_lowercase().contains(query.as_str())
                && !relative.to_lowercase().contains(query.as_str())
            {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            matches.push(WorkspaceDirectoryEntry {
                name,
                path: relative,
                is_dir: file_type.is_dir(),
                size: metadata.len(),
                modified_at: modified_at_ms(&metadata),
            });
        }
        if truncated {
            break;
        }
    }
    matches.sort_by(|left, right| left.path.to_lowercase().cmp(&right.path.to_lowercase()));
    Ok(WorkspaceEntrySearch {
        matches,
        visited_dirs,
        truncated,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceContentMatch {
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceContentSearch {
    pub(crate) matches: Vec<WorkspaceContentMatch>,
    pub(crate) scanned_files: usize,
    pub(crate) truncated: bool,
}

pub(crate) fn search_workspace_content(
    workspace: &WorkspaceState,
    requested_path: &str,
    query: &str,
    limit: usize,
) -> Result<WorkspaceContentSearch> {
    let root = resolve_workspace_path(workspace, requested_path)?;
    let query = query.trim();
    if query.is_empty() {
        return Err(anyhow!("search query must not be empty"));
    }
    let query_lower = query.to_lowercase();
    let limit = limit.clamp(1, 500);
    let started = Instant::now();
    let mut stack = vec![root];
    let mut matches = Vec::new();
    let mut scanned_files = 0usize;
    let mut visited = 0usize;
    let mut truncated = false;
    while let Some(path) = stack.pop() {
        if started.elapsed() >= SEARCH_DEADLINE || visited >= MAX_SEARCH_VISITS {
            truncated = true;
            break;
        }
        visited += 1;
        let metadata = match fs::symlink_metadata(path.as_path()) {
            Ok(metadata) if !metadata.file_type().is_symlink() => metadata,
            _ => continue,
        };
        if metadata.is_dir() {
            let entries = match fs::read_dir(path.as_path()) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            stack.extend(entries.flatten().map(|entry| entry.path()));
            continue;
        }
        if !metadata.is_file() || metadata.len() > MAX_SEARCH_FILE_BYTES {
            continue;
        }
        let bytes = match fs::read(path.as_path()) {
            Ok(bytes) if !is_binary_buffer(bytes.as_slice()) => bytes,
            _ => continue,
        };
        scanned_files += 1;
        let content = String::from_utf8_lossy(bytes.as_slice());
        for (line_index, line) in content.lines().enumerate() {
            let line_lower = line.to_lowercase();
            if let Some(offset) = line_lower.find(query_lower.as_str()) {
                matches.push(WorkspaceContentMatch {
                    path: relative_to_workspace(workspace, path.as_path()),
                    line: line_index + 1,
                    column: line_lower[..offset].chars().count() + 1,
                    text: truncate_chars(line, 2_000),
                });
                if matches.len() >= limit {
                    truncated = true;
                    break;
                }
            }
        }
        if truncated {
            break;
        }
    }
    Ok(WorkspaceContentSearch {
        matches,
        scanned_files,
        truncated,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceWriteResult {
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) modified_at: Option<u64>,
    pub(crate) created: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceDeleteResult {
    pub(crate) path: String,
    pub(crate) is_dir: bool,
    pub(crate) recursive: bool,
    pub(crate) deleted: bool,
}

pub(crate) fn delete_workspace_entry(
    workspace: &WorkspaceState,
    requested_path: &str,
    recursive: bool,
) -> Result<WorkspaceDeleteResult> {
    let (path, normalized) = resolve_workspace_entry_path_no_follow(workspace, requested_path)?;
    let metadata = fs::symlink_metadata(path.as_path())?;
    let is_dir = metadata.is_dir() && !metadata.file_type().is_symlink();
    if is_dir {
        if recursive {
            fs::remove_dir_all(path.as_path())?;
        } else {
            fs::remove_dir(path.as_path())?;
        }
    } else {
        fs::remove_file(path.as_path())?;
    }
    Ok(WorkspaceDeleteResult {
        path: normalized,
        is_dir,
        recursive,
        deleted: true,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WorkspaceMoveResult {
    pub(crate) from_path: String,
    pub(crate) to_path: String,
    pub(crate) name: String,
    pub(crate) is_dir: bool,
    pub(crate) replaced: bool,
    pub(crate) moved: bool,
}

pub(crate) fn move_workspace_entry(
    workspace: &WorkspaceState,
    source_path: &str,
    target_path: &str,
    replace_existing: bool,
) -> Result<WorkspaceMoveResult> {
    let (source, source_relative) = resolve_workspace_entry_path_no_follow(workspace, source_path)?;
    let (target, target_relative) = resolve_workspace_write_path(workspace, target_path)?;
    let source_metadata = fs::symlink_metadata(source.as_path())?;
    let source_is_dir = source_metadata.is_dir() && !source_metadata.file_type().is_symlink();
    if source == target {
        return Ok(WorkspaceMoveResult {
            from_path: source_relative,
            to_path: target_relative.clone(),
            name: target
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            is_dir: source_is_dir,
            replaced: false,
            moved: false,
        });
    }
    if source_is_dir {
        let source_canonical = source.canonicalize()?;
        let target_parent = target
            .parent()
            .ok_or_else(|| anyhow!("move target has no parent"))?
            .canonicalize()?;
        if target_parent.starts_with(source_canonical.as_path()) {
            return Err(anyhow!("cannot move a directory into its descendant"));
        }
    }
    let mut replaced = false;
    if target.exists() {
        if !replace_existing {
            return Err(anyhow!("move target already exists"));
        }
        let metadata = fs::symlink_metadata(target.as_path())?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(target.as_path())?;
        } else {
            fs::remove_file(target.as_path())?;
        }
        replaced = true;
    }
    fs::rename(source.as_path(), target.as_path())?;
    Ok(WorkspaceMoveResult {
        from_path: source_relative,
        to_path: target_relative,
        name: target
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
        is_dir: source_is_dir,
        replaced,
        moved: true,
    })
}

pub(crate) fn write_workspace_file(
    workspace: &WorkspaceState,
    requested_path: &str,
    content: &str,
    create_only: bool,
) -> Result<WorkspaceWriteResult> {
    let (path, normalized) = resolve_workspace_write_path(workspace, requested_path)?;
    let existed = path.exists();
    if existed {
        let metadata = fs::symlink_metadata(path.as_path())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(anyhow!("workspace write target is not a regular file"));
        }
        if create_only {
            return Err(anyhow!("file already exists"));
        }
    }
    let mut options = fs::OpenOptions::new();
    options.write(true);
    if create_only {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    use std::io::Write;
    let mut file = options.open(path.as_path())?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    let metadata = file.metadata()?;
    Ok(WorkspaceWriteResult {
        path: normalized,
        size: metadata.len(),
        modified_at: modified_at_ms(&metadata),
        created: !existed,
    })
}

fn resolve_workspace_write_path(
    workspace: &WorkspaceState,
    requested_path: &str,
) -> Result<(PathBuf, String)> {
    let normalized = normalize_relative_workspace_path(requested_path)?;
    if normalized == "." {
        return Err(anyhow!("file path must not be the workspace root"));
    }
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let candidate = root.join(normalized.as_str());
    let parent = candidate
        .parent()
        .ok_or_else(|| anyhow!("file path has no parent directory"))?
        .canonicalize()
        .with_context(|| format!("resolve workspace file parent {}", candidate.display()))?;
    if !parent.starts_with(root.as_path()) || !parent.is_dir() {
        return Err(anyhow!("file path escapes authorized workspace"));
    }
    Ok((candidate, normalized))
}

fn resolve_workspace_entry_path_no_follow(
    workspace: &WorkspaceState,
    requested_path: &str,
) -> Result<(PathBuf, String)> {
    let (candidate, normalized) = resolve_workspace_write_path(workspace, requested_path)?;
    fs::symlink_metadata(candidate.as_path())?;
    Ok((candidate, normalized))
}

fn is_binary_buffer(bytes: &[u8]) -> bool {
    bytes.iter().take(8_000).any(|byte| *byte == 0)
}

fn modified_at_ms(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push_str("...");
    }
    output
}

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

    use super::{
        create_workspace_directory, delete_workspace_entry, list_workspace_directory,
        move_workspace_entry, read_workspace_file, search_workspace_content,
        search_workspace_entries, write_workspace_file,
    };
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
    fn lists_and_creates_directories_relative_to_the_authorized_workspace() {
        let root = std::env::temp_dir().join(format!(
            "chatos-local-workspace-directories-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("apps/backend")).expect("create test directories");
        let workspace = workspace(root.canonicalize().expect("canonical workspace"));

        let listing = list_workspace_directory(&workspace, "apps", false).expect("list workspace");
        assert_eq!(listing.path, "apps");
        assert_eq!(listing.parent.as_deref(), Some("."));
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].path, "apps/backend");

        let created =
            create_workspace_directory(&workspace, "apps/frontend/src").expect("create directory");
        assert_eq!(created, "apps/frontend/src");
        assert!(root.join("apps/frontend/src").is_dir());
        assert!(create_workspace_directory(&workspace, "../outside").is_err());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn lists_files_and_supports_safe_read_write_and_search() {
        let root = std::env::temp_dir().join(format!(
            "chatos-local-workspace-files-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("src")).expect("create test directory");
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"hello\"); }\n",
        )
        .expect("write test file");
        let workspace = workspace(root.canonicalize().expect("canonical workspace"));

        let listing = list_workspace_directory(&workspace, "src", true).expect("list files");
        assert_eq!(listing.entries.len(), 1);
        assert!(!listing.entries[0].is_dir);
        let read = read_workspace_file(&workspace, "src/main.rs").expect("read file");
        assert!(!read.is_binary);
        assert!(read.content.contains("hello"));
        let names = search_workspace_entries(&workspace, ".", "main", 10).expect("search entries");
        assert_eq!(names.matches.len(), 1);
        let content =
            search_workspace_content(&workspace, ".", "println", 10).expect("search content");
        assert_eq!(content.matches.len(), 1);
        let write = write_workspace_file(&workspace, "src/main.rs", "fn main() {}\n", false)
            .expect("write file");
        assert!(!write.created);
        let created =
            write_workspace_file(&workspace, "src/new.txt", "new\n", true).expect("create file");
        assert!(created.created);
        let moved =
            move_workspace_entry(&workspace, "src/new.txt", "moved.txt", false).expect("move file");
        assert!(moved.moved);
        let deleted = delete_workspace_entry(&workspace, "moved.txt", false).expect("delete file");
        assert!(deleted.deleted);
        assert!(write_workspace_file(&workspace, "../outside.txt", "no", false).is_err());

        let _ = std::fs::remove_dir_all(root);
    }
}
