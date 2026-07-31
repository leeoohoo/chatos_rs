// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

const WORKSPACE_MAX_ENTRIES: usize = 65_536;
const WORKSPACE_MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const WORKSPACE_MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const WORKSPACE_MAX_CHANGED_ENTRIES: usize = 4_096;
const WORKSPACE_MAX_CHANGED_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ContentSnapshot {
    Directory,
    File { size: u64, sha256: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceSnapshot {
    content: ContentSnapshot,
    identity: FileIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    first: u64,
    second: u64,
    third: u64,
}

#[derive(Debug, Clone)]
pub(super) struct WorkspaceAclPath {
    pub(super) path: PathBuf,
    pub(super) directory: bool,
    pub(super) protected_git: bool,
}

#[derive(Debug)]
pub(super) struct WindowsWorkspaceMirror {
    source_root: PathBuf,
    staged_root: PathBuf,
    source_root_identity: FileIdentity,
    source_baseline: BTreeMap<String, SourceSnapshot>,
    mirror_baseline: BTreeMap<String, ContentSnapshot>,
}

impl WindowsWorkspaceMirror {
    pub(super) fn stage(source_root: &Path, staged_root: &Path) -> Result<Self, String> {
        let root_metadata = safe_metadata(source_root, "approved Plugin Hook workspace root")?;
        if !root_metadata.is_dir() {
            return Err("approved Plugin Hook workspace root is not a directory".to_string());
        }
        let source_root_identity = file_identity(source_root, &root_metadata)?;
        if staged_root.exists() {
            return Err("Windows Plugin Hook workspace mirror already exists".to_string());
        }
        fs::create_dir(staged_root).map_err(|error| {
            format!("create Windows Plugin Hook workspace mirror failed: {error}")
        })?;

        let mut state = CollectionState::default();
        let mut source_baseline = BTreeMap::new();
        let mut mirror_baseline = BTreeMap::new();
        let mut casefolded_paths = BTreeSet::new();
        copy_tree(
            source_root,
            staged_root,
            Path::new(""),
            &mut state,
            &mut source_baseline,
            &mut mirror_baseline,
            &mut casefolded_paths,
        )?;
        let root_after = safe_metadata(
            source_root,
            "approved Plugin Hook workspace root after staging",
        )?;
        if !root_after.is_dir() || file_identity(source_root, &root_after)? != source_root_identity
        {
            return Err("approved Plugin Hook workspace root changed during staging".to_string());
        }
        if !mirror_baseline.keys().any(|path| is_git_path(path)) {
            let git_root = staged_root.join(".git");
            fs::create_dir(git_root.as_path()).map_err(|error| {
                format!("create Windows Plugin Hook protected .git mirror failed: {error}")
            })?;
            mirror_baseline.insert(".git".to_string(), ContentSnapshot::Directory);
        }

        Ok(Self {
            source_root: source_root.to_path_buf(),
            staged_root: staged_root.to_path_buf(),
            source_root_identity,
            source_baseline,
            mirror_baseline,
        })
    }

    pub(super) fn staged_root(&self) -> &Path {
        self.staged_root.as_path()
    }

    pub(super) fn acl_paths(&self) -> Vec<WorkspaceAclPath> {
        let mut paths = Vec::with_capacity(self.mirror_baseline.len() + 1);
        paths.push(WorkspaceAclPath {
            path: self.staged_root.clone(),
            directory: true,
            protected_git: false,
        });
        paths.extend(
            self.mirror_baseline
                .iter()
                .map(|(relative, snapshot)| WorkspaceAclPath {
                    path: self.staged_root.join(relative_path(relative)),
                    directory: matches!(snapshot, ContentSnapshot::Directory),
                    protected_git: is_git_path(relative),
                }),
        );
        paths
    }

    pub(super) fn commit(&self) -> Result<(), String> {
        let output = collect_content_tree(self.staged_root.as_path())?;
        verify_git_unchanged(&self.mirror_baseline, &output)?;
        let changed = changed_paths(&self.mirror_baseline, &output)?;
        if changed.is_empty() {
            return Ok(());
        }

        let root_metadata = safe_metadata(
            self.source_root.as_path(),
            "approved Plugin Hook workspace root before commit",
        )?;
        if !root_metadata.is_dir()
            || file_identity(self.source_root.as_path(), &root_metadata)?
                != self.source_root_identity
        {
            return Err("approved Plugin Hook workspace root changed during execution".to_string());
        }
        let current = collect_source_tree(self.source_root.as_path())?;
        preflight_changes(&self.source_baseline, &current, changed.as_slice(), &output)?;
        apply_changes(
            self.source_root.as_path(),
            self.staged_root.as_path(),
            self.source_root_identity,
            &self.source_baseline,
            &output,
            changed.as_slice(),
        )
    }
}

#[derive(Default)]
struct CollectionState {
    entries: usize,
    total_bytes: u64,
}

fn copy_tree(
    source_root: &Path,
    staged_root: &Path,
    relative_root: &Path,
    state: &mut CollectionState,
    source_baseline: &mut BTreeMap<String, SourceSnapshot>,
    mirror_baseline: &mut BTreeMap<String, ContentSnapshot>,
    casefolded_paths: &mut BTreeSet<String>,
) -> Result<(), String> {
    let source_directory = source_root.join(relative_root);
    let directory_before = safe_metadata(
        source_directory.as_path(),
        "approved Plugin Hook workspace directory",
    )?;
    if !directory_before.is_dir() {
        return Err("approved Plugin Hook workspace directory changed type".to_string());
    }
    let directory_identity = file_identity(source_directory.as_path(), &directory_before)?;
    let mut entries = fs::read_dir(source_directory.as_path())
        .map_err(|error| format!("read approved Plugin Hook workspace failed: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read approved Plugin Hook workspace entry failed: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    let expected_names = entries
        .iter()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();

    for entry in entries {
        let name = entry.file_name();
        validate_windows_component(name.as_os_str())?;
        let relative = relative_root.join(name.as_os_str());
        let key = relative_key(relative.as_path())?;
        insert_casefolded_path(casefolded_paths, key.as_str())?;
        state.entries = state
            .entries
            .checked_add(1)
            .filter(|count| *count <= WORKSPACE_MAX_ENTRIES)
            .ok_or_else(|| "approved Plugin Hook workspace exceeds the entry limit".to_string())?;

        let source_path = source_root.join(relative.as_path());
        let staged_path = staged_root.join(relative.as_path());
        let before = safe_metadata(
            source_path.as_path(),
            "approved Plugin Hook workspace entry",
        )?;
        if before.is_dir() {
            fs::create_dir(staged_path.as_path()).map_err(|error| {
                format!("create Windows Plugin Hook workspace directory failed: {error}")
            })?;
            let content = ContentSnapshot::Directory;
            source_baseline.insert(
                key.clone(),
                SourceSnapshot {
                    content: content.clone(),
                    identity: file_identity(source_path.as_path(), &before)?,
                },
            );
            mirror_baseline.insert(key, content);
            copy_tree(
                source_root,
                staged_root,
                relative.as_path(),
                state,
                source_baseline,
                mirror_baseline,
                casefolded_paths,
            )?;
        } else if before.is_file() {
            reserve_file_bytes(state, before.len())?;
            let sha256 =
                copy_file_and_hash(source_path.as_path(), staged_path.as_path(), before.len())?;
            let after = safe_metadata(
                source_path.as_path(),
                "approved Plugin Hook workspace file after staging",
            )?;
            if !after.is_file()
                || after.len() != before.len()
                || file_identity(source_path.as_path(), &after)?
                    != file_identity(source_path.as_path(), &before)?
            {
                return Err(
                    "approved Plugin Hook workspace file changed during staging".to_string()
                );
            }
            let content = ContentSnapshot::File {
                size: before.len(),
                sha256,
            };
            source_baseline.insert(
                key.clone(),
                SourceSnapshot {
                    content: content.clone(),
                    identity: file_identity(source_path.as_path(), &before)?,
                },
            );
            mirror_baseline.insert(key, content);
        } else {
            return Err("approved Plugin Hook workspace contains an unsupported entry".to_string());
        }
    }
    let after_names = read_sorted_names(source_directory.as_path())?;
    let directory_after = safe_metadata(
        source_directory.as_path(),
        "approved Plugin Hook workspace directory after staging",
    )?;
    if after_names != expected_names
        || !directory_after.is_dir()
        || file_identity(source_directory.as_path(), &directory_after)? != directory_identity
    {
        return Err("approved Plugin Hook workspace directory changed during staging".to_string());
    }
    Ok(())
}

fn read_sorted_names(path: &Path) -> Result<Vec<OsString>, String> {
    let mut names = fs::read_dir(path)
        .map_err(|error| format!("re-read approved Plugin Hook workspace failed: {error}"))?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|error| format!("re-read Plugin Hook workspace entry failed: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    Ok(names)
}

fn collect_content_tree(root: &Path) -> Result<BTreeMap<String, ContentSnapshot>, String> {
    let mut output = BTreeMap::new();
    let mut casefolded_paths = BTreeSet::new();
    let mut state = CollectionState::default();
    collect_content_tree_inner(
        root,
        Path::new(""),
        &mut state,
        &mut output,
        &mut casefolded_paths,
    )?;
    Ok(output)
}

fn collect_content_tree_inner(
    root: &Path,
    relative_root: &Path,
    state: &mut CollectionState,
    output: &mut BTreeMap<String, ContentSnapshot>,
    casefolded_paths: &mut BTreeSet<String>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(root.join(relative_root))
        .map_err(|error| format!("read Windows Plugin Hook workspace mirror failed: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            format!("read Windows Plugin Hook workspace mirror entry failed: {error}")
        })?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        validate_windows_component(name.as_os_str())?;
        let relative = relative_root.join(name.as_os_str());
        let key = relative_key(relative.as_path())?;
        insert_casefolded_path(casefolded_paths, key.as_str())?;
        state.entries = state
            .entries
            .checked_add(1)
            .filter(|count| *count <= WORKSPACE_MAX_ENTRIES)
            .ok_or_else(|| {
                "Windows Plugin Hook workspace mirror exceeds the entry limit".to_string()
            })?;
        let path = root.join(relative.as_path());
        let metadata = safe_metadata(path.as_path(), "Windows Plugin Hook workspace mirror entry")?;
        if metadata.is_dir() {
            output.insert(key, ContentSnapshot::Directory);
            collect_content_tree_inner(root, relative.as_path(), state, output, casefolded_paths)?;
        } else if metadata.is_file() {
            reserve_file_bytes(state, metadata.len())?;
            let sha256 = hash_file_stable(path.as_path(), &metadata)?;
            output.insert(
                key,
                ContentSnapshot::File {
                    size: metadata.len(),
                    sha256,
                },
            );
        } else {
            return Err(
                "Windows Plugin Hook workspace mirror contains an unsupported entry".to_string(),
            );
        }
    }
    Ok(())
}

fn collect_source_tree(root: &Path) -> Result<BTreeMap<String, SourceSnapshot>, String> {
    let mut output = BTreeMap::new();
    let mut casefolded_paths = BTreeSet::new();
    let mut state = CollectionState::default();
    collect_source_tree_inner(
        root,
        Path::new(""),
        &mut state,
        &mut output,
        &mut casefolded_paths,
    )?;
    Ok(output)
}

fn collect_source_tree_inner(
    root: &Path,
    relative_root: &Path,
    state: &mut CollectionState,
    output: &mut BTreeMap<String, SourceSnapshot>,
    casefolded_paths: &mut BTreeSet<String>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(root.join(relative_root))
        .map_err(|error| format!("re-read approved Plugin Hook workspace failed: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("re-read approved Plugin Hook workspace entry failed: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        validate_windows_component(name.as_os_str())?;
        let relative = relative_root.join(name.as_os_str());
        let key = relative_key(relative.as_path())?;
        insert_casefolded_path(casefolded_paths, key.as_str())?;
        if is_git_path(key.as_str()) {
            continue;
        }
        state.entries = state
            .entries
            .checked_add(1)
            .filter(|count| *count <= WORKSPACE_MAX_ENTRIES)
            .ok_or_else(|| "approved Plugin Hook workspace exceeds the entry limit".to_string())?;
        let path = root.join(relative.as_path());
        let metadata = safe_metadata(path.as_path(), "approved Plugin Hook workspace entry")?;
        let identity = file_identity(path.as_path(), &metadata)?;
        if metadata.is_dir() {
            output.insert(
                key,
                SourceSnapshot {
                    content: ContentSnapshot::Directory,
                    identity,
                },
            );
            collect_source_tree_inner(root, relative.as_path(), state, output, casefolded_paths)?;
        } else if metadata.is_file() {
            reserve_file_bytes(state, metadata.len())?;
            let sha256 = hash_file_stable(path.as_path(), &metadata)?;
            output.insert(
                key,
                SourceSnapshot {
                    content: ContentSnapshot::File {
                        size: metadata.len(),
                        sha256,
                    },
                    identity,
                },
            );
        } else {
            return Err("approved Plugin Hook workspace contains an unsupported entry".to_string());
        }
    }
    Ok(())
}

fn verify_git_unchanged(
    baseline: &BTreeMap<String, ContentSnapshot>,
    output: &BTreeMap<String, ContentSnapshot>,
) -> Result<(), String> {
    let baseline_git = baseline
        .iter()
        .filter(|(path, _)| is_git_path(path))
        .collect::<BTreeMap<_, _>>();
    let output_git = output
        .iter()
        .filter(|(path, _)| is_git_path(path))
        .collect::<BTreeMap<_, _>>();
    if baseline_git != output_git {
        return Err(
            "Windows Plugin Hook attempted to modify the protected .git mirror".to_string(),
        );
    }
    Ok(())
}

fn changed_paths(
    baseline: &BTreeMap<String, ContentSnapshot>,
    output: &BTreeMap<String, ContentSnapshot>,
) -> Result<Vec<String>, String> {
    let paths = baseline
        .keys()
        .chain(output.keys())
        .filter(|path| !is_git_path(path))
        .cloned()
        .collect::<BTreeSet<_>>();
    let changed = paths
        .into_iter()
        .filter(|path| baseline.get(path) != output.get(path))
        .collect::<Vec<_>>();
    if changed.len() > WORKSPACE_MAX_CHANGED_ENTRIES {
        return Err("Windows Plugin Hook workspace change count exceeds its limit".to_string());
    }
    let changed_bytes = changed.iter().try_fold(0_u64, |total, path| {
        let bytes = match output.get(path) {
            Some(ContentSnapshot::File { size, .. }) => *size,
            _ => 0,
        };
        total
            .checked_add(bytes)
            .filter(|value| *value <= WORKSPACE_MAX_CHANGED_BYTES)
            .ok_or_else(|| {
                "Windows Plugin Hook workspace changes exceed the byte limit".to_string()
            })
    })?;
    let _ = changed_bytes;
    Ok(changed)
}

fn preflight_changes(
    baseline: &BTreeMap<String, SourceSnapshot>,
    current: &BTreeMap<String, SourceSnapshot>,
    changed: &[String],
    output: &BTreeMap<String, ContentSnapshot>,
) -> Result<(), String> {
    for path in changed {
        if baseline.get(path) != current.get(path) {
            return Err(format!(
                "approved Plugin Hook workspace changed concurrently at {path}"
            ));
        }
        for ancestor in ancestor_keys(path) {
            if baseline.get(ancestor.as_str()) != current.get(ancestor.as_str()) {
                return Err(format!(
                    "approved Plugin Hook workspace ancestor changed concurrently at {ancestor}"
                ));
            }
        }
    }

    let removed_directories = changed
        .iter()
        .filter(|path| {
            baseline
                .get(path.as_str())
                .is_some_and(|entry| matches!(&entry.content, ContentSnapshot::Directory))
                && !matches!(output.get(path.as_str()), Some(ContentSnapshot::Directory))
        })
        .collect::<Vec<_>>();
    for current_path in current.keys() {
        if !baseline.contains_key(current_path)
            && removed_directories
                .iter()
                .any(|directory| is_descendant(current_path, directory))
        {
            return Err(format!(
                "approved Plugin Hook workspace gained a concurrent entry at {current_path}"
            ));
        }
    }
    Ok(())
}

fn apply_changes(
    source_root: &Path,
    staged_root: &Path,
    source_root_identity: FileIdentity,
    baseline: &BTreeMap<String, SourceSnapshot>,
    output: &BTreeMap<String, ContentSnapshot>,
    changed: &[String],
) -> Result<(), String> {
    let mut deletions = changed
        .iter()
        .filter(|path| {
            baseline.get(path.as_str()).is_some()
                && match (baseline.get(path.as_str()), output.get(path.as_str())) {
                    (Some(old), Some(new)) => old.content.kind() != new.kind(),
                    (Some(_), None) => true,
                    _ => false,
                }
        })
        .cloned()
        .collect::<Vec<_>>();
    deletions.sort_by(|left, right| {
        path_depth(right)
            .cmp(&path_depth(left))
            .then_with(|| right.cmp(left))
    });
    for path in deletions {
        verify_root_identity(source_root, source_root_identity)?;
        verify_unchanged_ancestors(source_root, path.as_str(), baseline, output)?;
        verify_current_entry(source_root, path.as_str(), baseline.get(path.as_str()))?;
        let destination = source_root.join(relative_path(path.as_str()));
        match &baseline[&path].content {
            ContentSnapshot::File { .. } => {
                fs::remove_file(destination.as_path()).map_err(|error| {
                    format!("delete approved Plugin Hook workspace file failed: {error}")
                })?
            }
            ContentSnapshot::Directory => {
                fs::remove_dir(destination.as_path()).map_err(|error| {
                    format!("delete approved Plugin Hook workspace directory failed: {error}")
                })?
            }
        }
    }

    let mut directories = changed
        .iter()
        .filter(|path| matches!(output.get(path.as_str()), Some(ContentSnapshot::Directory)))
        .filter(|path| {
            !baseline
                .get(path.as_str())
                .is_some_and(|entry| matches!(&entry.content, ContentSnapshot::Directory))
        })
        .cloned()
        .collect::<Vec<_>>();
    directories.sort_by(|left, right| {
        path_depth(left)
            .cmp(&path_depth(right))
            .then_with(|| left.cmp(right))
    });
    for path in directories {
        verify_root_identity(source_root, source_root_identity)?;
        verify_unchanged_ancestors(source_root, path.as_str(), baseline, output)?;
        let destination = source_root.join(relative_path(path.as_str()));
        ensure_safe_parent(source_root, destination.as_path())?;
        fs::create_dir(destination.as_path()).map_err(|error| {
            format!("create approved Plugin Hook workspace directory failed: {error}")
        })?;
    }

    let files = changed
        .iter()
        .filter_map(|path| match output.get(path.as_str()) {
            Some(ContentSnapshot::File { size, sha256 }) => {
                Some((path.as_str(), *size, sha256.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    for (path, size, sha256) in files {
        verify_root_identity(source_root, source_root_identity)?;
        verify_unchanged_ancestors(source_root, path, baseline, output)?;
        let expected = baseline.get(path);
        let expected_file =
            expected.filter(|entry| matches!(&entry.content, ContentSnapshot::File { .. }));
        if expected_file.is_some() {
            verify_current_entry(source_root, path, expected_file)?;
        } else if source_root.join(relative_path(path)).exists() {
            return Err(format!(
                "approved Plugin Hook workspace path changed before commit at {path}"
            ));
        }
        let source = staged_root.join(relative_path(path));
        let destination = source_root.join(relative_path(path));
        ensure_safe_parent(source_root, destination.as_path())?;
        write_file_atomically(
            source.as_path(),
            destination.as_path(),
            size,
            sha256,
            expected_file,
        )?;
    }
    Ok(())
}

impl ContentSnapshot {
    fn kind(&self) -> u8 {
        match self {
            Self::Directory => 0,
            Self::File { .. } => 1,
        }
    }
}

fn verify_root_identity(root: &Path, expected: FileIdentity) -> Result<(), String> {
    let metadata = safe_metadata(root, "approved Plugin Hook workspace root during commit")?;
    if !metadata.is_dir() || file_identity(root, &metadata)? != expected {
        return Err("approved Plugin Hook workspace root changed during commit".to_string());
    }
    Ok(())
}

fn verify_current_entry(
    root: &Path,
    relative: &str,
    expected: Option<&SourceSnapshot>,
) -> Result<(), String> {
    let path = root.join(relative_path(relative));
    let actual = match fs::symlink_metadata(path.as_path()) {
        Ok(metadata) => Some(snapshot_source_file(path.as_path(), &metadata)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "re-read approved Plugin Hook workspace path failed: {error}"
            ))
        }
    };
    if actual.as_ref() != expected {
        return Err(format!(
            "approved Plugin Hook workspace changed before commit at {relative}"
        ));
    }
    Ok(())
}

fn verify_unchanged_ancestors(
    root: &Path,
    relative: &str,
    baseline: &BTreeMap<String, SourceSnapshot>,
    output: &BTreeMap<String, ContentSnapshot>,
) -> Result<(), String> {
    for ancestor in ancestor_keys(relative) {
        let Some(expected) = baseline.get(ancestor.as_str()) else {
            continue;
        };
        if !matches!(&expected.content, ContentSnapshot::Directory)
            || !matches!(
                output.get(ancestor.as_str()),
                Some(ContentSnapshot::Directory)
            )
        {
            continue;
        }
        verify_current_entry(root, ancestor.as_str(), Some(expected))?;
    }
    Ok(())
}

fn snapshot_source_file(path: &Path, metadata: &fs::Metadata) -> Result<SourceSnapshot, String> {
    ensure_safe_file_type(metadata, "approved Plugin Hook workspace path")?;
    let content = if metadata.is_dir() {
        ContentSnapshot::Directory
    } else if metadata.is_file() {
        if metadata.len() > WORKSPACE_MAX_FILE_BYTES {
            return Err("approved Plugin Hook workspace file exceeds its size limit".to_string());
        }
        ContentSnapshot::File {
            size: metadata.len(),
            sha256: hash_file_stable(path, metadata)?,
        }
    } else {
        return Err("approved Plugin Hook workspace path has an unsupported type".to_string());
    };
    Ok(SourceSnapshot {
        content,
        identity: file_identity(path, metadata)?,
    })
}

fn ensure_safe_parent(root: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "approved Plugin Hook workspace destination has no parent".to_string())?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| "approved Plugin Hook workspace destination escaped its root".to_string())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err("approved Plugin Hook workspace parent is not normalized".to_string());
        };
        current.push(component);
        let metadata = safe_metadata(current.as_path(), "approved Plugin Hook workspace parent")?;
        if !metadata.is_dir() {
            return Err("approved Plugin Hook workspace parent is not a directory".to_string());
        }
    }
    Ok(())
}

fn write_file_atomically(
    source: &Path,
    destination: &Path,
    expected_size: u64,
    expected_sha256: &str,
    expected_destination: Option<&SourceSnapshot>,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "approved Plugin Hook workspace file has no parent".to_string())?;
    let temp = parent.join(format!(".chatos-plugin-hook-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let metadata = safe_metadata(source, "Windows Plugin Hook changed file")?;
        if !metadata.is_file() || metadata.len() != expected_size {
            return Err("Windows Plugin Hook changed file drifted before commit".to_string());
        }
        let mut input = File::open(source)
            .map_err(|error| format!("open Windows Plugin Hook changed file failed: {error}"))?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(temp.as_path())
            .map_err(|error| {
                format!("create Plugin Hook workspace temporary file failed: {error}")
            })?;
        let mut hasher = Sha256::new();
        let mut copied = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = input.read(&mut buffer).map_err(|error| {
                format!("read Windows Plugin Hook changed file failed: {error}")
            })?;
            if read == 0 {
                break;
            }
            copied = copied
                .checked_add(read as u64)
                .filter(|size| *size <= WORKSPACE_MAX_FILE_BYTES)
                .ok_or_else(|| {
                    "Windows Plugin Hook changed file exceeds its size limit".to_string()
                })?;
            hasher.update(&buffer[..read]);
            output.write_all(&buffer[..read]).map_err(|error| {
                format!("write Plugin Hook workspace temporary file failed: {error}")
            })?;
        }
        output.sync_all().map_err(|error| {
            format!("sync Plugin Hook workspace temporary file failed: {error}")
        })?;
        let actual_sha256 = hex::encode(hasher.finalize());
        if copied != expected_size || actual_sha256 != expected_sha256 {
            return Err("Windows Plugin Hook changed file drifted during commit".to_string());
        }
        let current_destination = match fs::symlink_metadata(destination) {
            Ok(metadata) => Some(snapshot_source_file(destination, &metadata)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(format!(
                    "re-read Plugin Hook workspace destination failed: {error}"
                ))
            }
        };
        if current_destination.as_ref() != expected_destination {
            return Err("Plugin Hook workspace destination changed during commit".to_string());
        }
        atomic_replace(temp.as_path(), destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp.as_path());
    }
    result
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let wide = |path: &Path| {
        let mut value = path.as_os_str().encode_wide().collect::<Vec<_>>();
        value.push(0);
        value
    };
    let source = wide(source);
    let destination = wide(destination);
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(format!(
            "commit Plugin Hook workspace file failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination)
        .map_err(|error| format!("commit Plugin Hook workspace file failed: {error}"))
}

fn copy_file_and_hash(
    source: &Path,
    destination: &Path,
    expected_size: u64,
) -> Result<String, String> {
    let mut input = File::open(source)
        .map_err(|error| format!("open approved Plugin Hook workspace file failed: {error}"))?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|error| format!("create Windows Plugin Hook workspace file failed: {error}"))?;
    let mut hasher = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("read approved Plugin Hook workspace file failed: {error}"))?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(read as u64)
            .filter(|size| *size <= WORKSPACE_MAX_FILE_BYTES)
            .ok_or_else(|| {
                "approved Plugin Hook workspace file grew beyond its limit".to_string()
            })?;
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|error| format!("write Windows Plugin Hook workspace file failed: {error}"))?;
    }
    output
        .sync_all()
        .map_err(|error| format!("sync Windows Plugin Hook workspace file failed: {error}"))?;
    if copied != expected_size {
        return Err("approved Plugin Hook workspace file changed during staging".to_string());
    }
    Ok(hex::encode(hasher.finalize()))
}

fn hash_file_stable(path: &Path, before: &fs::Metadata) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("open Plugin Hook workspace file failed: {error}"))?;
    let mut hasher = Sha256::new();
    let mut read_total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("read Plugin Hook workspace file failed: {error}"))?;
        if read == 0 {
            break;
        }
        read_total = read_total
            .checked_add(read as u64)
            .filter(|size| *size <= WORKSPACE_MAX_FILE_BYTES)
            .ok_or_else(|| "Plugin Hook workspace file exceeds its size limit".to_string())?;
        hasher.update(&buffer[..read]);
    }
    let after = safe_metadata(path, "Plugin Hook workspace file after hashing")?;
    if !after.is_file()
        || after.len() != before.len()
        || read_total != before.len()
        || file_identity(path, &after)? != file_identity(path, before)?
    {
        return Err("Plugin Hook workspace file changed while hashing".to_string());
    }
    Ok(hex::encode(hasher.finalize()))
}

fn reserve_file_bytes(state: &mut CollectionState, size: u64) -> Result<(), String> {
    if size > WORKSPACE_MAX_FILE_BYTES {
        return Err("Plugin Hook workspace file exceeds its size limit".to_string());
    }
    state.total_bytes = state
        .total_bytes
        .checked_add(size)
        .filter(|total| *total <= WORKSPACE_MAX_TOTAL_BYTES)
        .ok_or_else(|| "Plugin Hook workspace exceeds its total byte limit".to_string())?;
    Ok(())
}

fn safe_metadata(path: &Path, label: &str) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("read {label} metadata failed: {error}"))?;
    ensure_safe_file_type(&metadata, label)?;
    Ok(metadata)
}

fn ensure_safe_file_type(metadata: &fs::Metadata, label: &str) -> Result<(), String> {
    if metadata.file_type().is_symlink() {
        return Err(format!("{label} must not be a symlink"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!("{label} must not be a reparse point"));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn file_identity(path: &Path, _metadata: &fs::Metadata) -> Result<FileIdentity, String> {
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FileIdInfo, GetFileInformationByHandleEx, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_ID_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if wide.contains(&0) {
        return Err("Windows Plugin Hook workspace path contains NUL".to_string());
    }
    wide.push(0);
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!(
            "open Windows Plugin Hook workspace identity handle failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut info = FILE_ID_INFO::default();
    let result = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    };
    unsafe {
        CloseHandle(handle);
    }
    if result == 0 {
        return Err(format!(
            "read Windows Plugin Hook workspace identity failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let first_file_id = u64::from_le_bytes(
        info.FileId.Identifier[..8]
            .try_into()
            .map_err(|_| "Windows Plugin Hook workspace file identity is invalid".to_string())?,
    );
    let second_file_id = u64::from_le_bytes(
        info.FileId.Identifier[8..]
            .try_into()
            .map_err(|_| "Windows Plugin Hook workspace file identity is invalid".to_string())?,
    );
    Ok(FileIdentity {
        first: info.VolumeSerialNumber,
        second: first_file_id,
        third: second_file_id,
    })
}

#[cfg(unix)]
fn file_identity(_path: &Path, metadata: &fs::Metadata) -> Result<FileIdentity, String> {
    use std::os::unix::fs::MetadataExt;

    Ok(FileIdentity {
        first: metadata.dev(),
        second: metadata.ino(),
        third: 0,
    })
}

fn validate_windows_component(value: &OsStr) -> Result<(), String> {
    let value = value
        .to_str()
        .ok_or_else(|| "Plugin Hook workspace path is not Unicode".to_string())?;
    if value.is_empty()
        || value.ends_with([' ', '.'])
        || value.contains(':')
        || value.chars().any(|character| {
            character <= '\u{1f}' || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
        })
    {
        return Err("Plugin Hook workspace path is not Windows-safe".to_string());
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            });
    if reserved {
        return Err("Plugin Hook workspace path uses a Windows device name".to_string());
    }
    Ok(())
}

fn relative_key(path: &Path) -> Result<String, String> {
    let mut components = Vec::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err("Plugin Hook workspace path is not normalized".to_string());
        };
        let component = component
            .to_str()
            .ok_or_else(|| "Plugin Hook workspace path is not Unicode".to_string())?;
        components.push(component);
    }
    let key = components.join("/");
    if key.is_empty() || key.len() > 2_048 {
        return Err("Plugin Hook workspace path has invalid bounds".to_string());
    }
    Ok(key)
}

fn relative_path(value: &str) -> PathBuf {
    value.split('/').collect()
}

fn insert_casefolded_path(paths: &mut BTreeSet<String>, path: &str) -> Result<(), String> {
    if !paths.insert(path.to_ascii_lowercase()) {
        return Err("Plugin Hook workspace contains a case-insensitive path collision".to_string());
    }
    Ok(())
}

fn is_git_path(path: &str) -> bool {
    path.split('/')
        .next()
        .is_some_and(|component| component.eq_ignore_ascii_case(".git"))
}

fn ancestor_keys(path: &str) -> Vec<String> {
    let parts = path.split('/').collect::<Vec<_>>();
    (1..parts.len()).map(|end| parts[..end].join("/")).collect()
}

fn is_descendant(path: &str, directory: &str) -> bool {
    path.len() > directory.len()
        && path.starts_with(directory)
        && path.as_bytes().get(directory.len()) == Some(&b'/')
}

fn path_depth(path: &str) -> usize {
    path.bytes().filter(|byte| *byte == b'/').count() + 1
}

#[cfg(test)]
mod tests {
    use super::WindowsWorkspaceMirror;
    use std::fs;

    #[test]
    fn commits_bounded_workspace_changes_without_copying_git_back() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let source = temp.path().join("source");
        let staged = temp.path().join("staged");
        fs::create_dir_all(source.join(".git")).expect("create git directory");
        fs::create_dir_all(source.join("old-dir")).expect("create old directory");
        fs::write(source.join(".git/config"), "protected").expect("write git config");
        fs::write(source.join("keep.txt"), "before").expect("write source file");
        fs::write(source.join("remove.txt"), "remove").expect("write removed file");
        fs::write(source.join("old-dir/inside.txt"), "inside").expect("write nested file");

        let mirror = WindowsWorkspaceMirror::stage(source.as_path(), staged.as_path())
            .expect("stage workspace");
        let acl_paths = mirror.acl_paths();
        assert!(acl_paths
            .first()
            .is_some_and(|path| { path.directory && path.path.as_path() == mirror.staged_root() }));
        assert!(acl_paths.iter().any(|path| path.protected_git));
        fs::write(mirror.staged_root().join("keep.txt"), "after").expect("update staged file");
        fs::remove_file(mirror.staged_root().join("remove.txt")).expect("remove staged file");
        fs::remove_file(mirror.staged_root().join("old-dir/inside.txt"))
            .expect("remove nested staged file");
        fs::remove_dir(mirror.staged_root().join("old-dir")).expect("remove staged directory");
        fs::create_dir(mirror.staged_root().join("new-dir")).expect("create staged directory");
        fs::write(mirror.staged_root().join("new-dir/new.txt"), "new")
            .expect("write staged new file");
        mirror.commit().expect("commit workspace mirror");

        assert_eq!(
            fs::read_to_string(source.join("keep.txt")).unwrap(),
            "after"
        );
        assert!(!source.join("remove.txt").exists());
        assert!(!source.join("old-dir").exists());
        assert_eq!(
            fs::read_to_string(source.join("new-dir/new.txt")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(source.join(".git/config")).unwrap(),
            "protected"
        );
    }

    #[test]
    fn rejects_git_changes_and_concurrent_source_changes() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let source = temp.path().join("source");
        fs::create_dir_all(source.join(".git")).expect("create git directory");
        fs::write(source.join(".git/config"), "protected").expect("write git config");
        fs::write(source.join("file.txt"), "before").expect("write source file");

        let git_staged = temp.path().join("git-staged");
        let git_mirror = WindowsWorkspaceMirror::stage(source.as_path(), git_staged.as_path())
            .expect("stage workspace");
        fs::write(git_mirror.staged_root().join(".git/config"), "changed")
            .expect("change staged git");
        assert!(git_mirror.commit().is_err());
        assert_eq!(
            fs::read_to_string(source.join(".git/config")).unwrap(),
            "protected"
        );

        let conflict_staged = temp.path().join("conflict-staged");
        let conflict = WindowsWorkspaceMirror::stage(source.as_path(), conflict_staged.as_path())
            .expect("stage workspace");
        fs::write(conflict.staged_root().join("file.txt"), "hook").expect("write staged conflict");
        fs::write(source.join("file.txt"), "user").expect("write concurrent source change");
        assert!(conflict.commit().is_err());
        assert_eq!(fs::read_to_string(source.join("file.txt")).unwrap(), "user");
    }

    #[test]
    fn absent_git_is_guarded_only_inside_the_mirror() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let source = temp.path().join("source");
        let staged = temp.path().join("staged");
        fs::create_dir(source.as_path()).expect("create source");
        fs::write(source.join("file.txt"), "before").expect("write source file");
        let mirror = WindowsWorkspaceMirror::stage(source.as_path(), staged.as_path())
            .expect("stage workspace");
        assert!(mirror.staged_root().join(".git").is_dir());
        fs::write(mirror.staged_root().join("file.txt"), "after").expect("change staged file");
        mirror.commit().expect("commit workspace mirror");
        assert!(!source.join(".git").exists());
        assert_eq!(
            fs::read_to_string(source.join("file.txt")).unwrap(),
            "after"
        );
    }

    #[test]
    fn commits_file_directory_type_changes_and_rejects_new_children_during_delete() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let source = temp.path().join("source");
        fs::create_dir_all(source.join("dir-to-file")).expect("create source directory");
        fs::write(source.join("file-to-dir"), "file").expect("write source file");
        fs::write(source.join("dir-to-file/child.txt"), "child").expect("write source child");

        let staged = temp.path().join("staged");
        let mirror = WindowsWorkspaceMirror::stage(source.as_path(), staged.as_path())
            .expect("stage workspace");
        fs::remove_file(mirror.staged_root().join("file-to-dir")).expect("remove staged file");
        fs::create_dir(mirror.staged_root().join("file-to-dir")).expect("create staged directory");
        fs::write(
            mirror.staged_root().join("file-to-dir/child.txt"),
            "new child",
        )
        .expect("write new staged child");
        fs::remove_file(mirror.staged_root().join("dir-to-file/child.txt"))
            .expect("remove old staged child");
        fs::remove_dir(mirror.staged_root().join("dir-to-file"))
            .expect("remove old staged directory");
        fs::write(mirror.staged_root().join("dir-to-file"), "replacement")
            .expect("write staged replacement file");
        mirror.commit().expect("commit type changes");
        assert_eq!(
            fs::read_to_string(source.join("file-to-dir/child.txt")).unwrap(),
            "new child"
        );
        assert_eq!(
            fs::read_to_string(source.join("dir-to-file")).unwrap(),
            "replacement"
        );

        let conflict_staged = temp.path().join("conflict-staged");
        let conflict = WindowsWorkspaceMirror::stage(source.as_path(), conflict_staged.as_path())
            .expect("stage conflict workspace");
        fs::remove_file(conflict.staged_root().join("file-to-dir/child.txt"))
            .expect("remove staged child");
        fs::remove_dir(conflict.staged_root().join("file-to-dir"))
            .expect("remove staged directory");
        fs::write(source.join("file-to-dir/user.txt"), "concurrent")
            .expect("write concurrent child");
        assert!(conflict.commit().is_err());
        assert_eq!(
            fs::read_to_string(source.join("file-to-dir/user.txt")).unwrap(),
            "concurrent"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_source_and_output() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temporary directory");
        let source = temp.path().join("source");
        fs::create_dir(source.as_path()).expect("create source");
        fs::write(source.join("target.txt"), "target").expect("write target");
        symlink(source.join("target.txt"), source.join("link.txt")).expect("create source link");
        assert!(WindowsWorkspaceMirror::stage(
            source.as_path(),
            temp.path().join("unsafe-staged").as_path()
        )
        .is_err());

        fs::remove_file(source.join("link.txt")).expect("remove source link");
        let staged = temp.path().join("staged");
        let mirror = WindowsWorkspaceMirror::stage(source.as_path(), staged.as_path())
            .expect("stage workspace");
        symlink(
            mirror.staged_root().join("target.txt"),
            mirror.staged_root().join("output-link.txt"),
        )
        .expect("create output link");
        assert!(mirror.commit().is_err());
    }
}
