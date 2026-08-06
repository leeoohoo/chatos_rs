// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::utils::ensure_path_inside_root;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

mod hunks;
mod parser;
mod replacement;

#[cfg(test)]
mod tests;

use hunks::{apply_hunks, join_lines, split_lines};
use parser::{parse_patch, parse_replace_style_patch};
use replacement::replace_text_once;

const DEFAULT_PATCH_TARGET_LIMIT_BYTES: i64 = 4 * 1024 * 1024;

#[derive(Debug, Default, serde::Serialize)]
pub struct ApplyPatchResult {
    pub updated: Vec<String>,
    pub added: Vec<String>,
    pub deleted: Vec<String>,
}

impl ApplyPatchResult {
    pub fn changed(&self) -> bool {
        self.changed_path_count() > 0
    }

    pub fn changed_path_count(&self) -> usize {
        self.updated.len() + self.added.len() + self.deleted.len()
    }
}

enum PatchOp {
    Update {
        path: String,
        move_to: Option<String>,
        hunks: Vec<String>,
    },
    Add {
        path: String,
        lines: Vec<String>,
    },
    Delete {
        path: String,
    },
    Replace {
        path: String,
        old_text: String,
        new_text: String,
    },
}

struct PatchPathSnapshot {
    path: PathBuf,
    content: Option<Vec<u8>>,
}

struct PatchTransaction {
    snapshots: Vec<PatchPathSnapshot>,
    missing_parent_dirs: Vec<PathBuf>,
}

impl PatchTransaction {
    fn capture(root: &Path, ops: &[PatchOp], max_target_bytes: u64) -> Result<Self, String> {
        let resolved_root = root
            .canonicalize()
            .map_err(|err| format!("Resolve workspace root failed: {err}"))?;
        let mut paths = BTreeSet::new();
        for op in ops {
            for path in patch_op_paths(op) {
                paths.insert(ensure_path_inside_root(root, Path::new(path))?);
            }
        }

        let mut snapshots = Vec::with_capacity(paths.len());
        let mut missing_parent_dirs = BTreeSet::new();
        for path in paths {
            let content = if path.exists() {
                let metadata = fs::metadata(&path).map_err(|err| err.to_string())?;
                if metadata.is_dir() {
                    return Err(format!(
                        "Patch target must be a file, not a directory: {}",
                        path.display()
                    ));
                }
                ensure_patch_target_within_limit(&path, metadata.len(), max_target_bytes)?;
                Some(fs::read(&path).map_err(|err| err.to_string())?)
            } else {
                collect_missing_parent_dirs(
                    &resolved_root,
                    path.parent(),
                    &mut missing_parent_dirs,
                );
                None
            };
            snapshots.push(PatchPathSnapshot { path, content });
        }

        let mut missing_parent_dirs = missing_parent_dirs.into_iter().collect::<Vec<_>>();
        missing_parent_dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        Ok(Self {
            snapshots,
            missing_parent_dirs,
        })
    }

    fn rollback(&self) -> Result<(), String> {
        let mut errors = Vec::new();
        for snapshot in &self.snapshots {
            if snapshot.path.exists() {
                if snapshot.path.is_dir() {
                    errors.push(format!(
                        "rollback target became a directory: {}",
                        snapshot.path.display()
                    ));
                    continue;
                }
                if let Err(err) = fs::remove_file(&snapshot.path) {
                    errors.push(format!("remove {}: {err}", snapshot.path.display()));
                }
            }
        }
        for snapshot in &self.snapshots {
            let Some(content) = &snapshot.content else {
                continue;
            };
            if let Some(parent) = snapshot.path.parent() {
                if let Err(err) = fs::create_dir_all(parent) {
                    errors.push(format!("create {}: {err}", parent.display()));
                    continue;
                }
            }
            if let Err(err) = fs::write(&snapshot.path, content) {
                errors.push(format!("restore {}: {err}", snapshot.path.display()));
            }
        }
        for path in &self.missing_parent_dirs {
            if path.exists()
                && path
                    .read_dir()
                    .map(|mut entries| entries.next().is_none())
                    .unwrap_or(false)
            {
                if let Err(err) = fs::remove_dir(path) {
                    errors.push(format!("remove directory {}: {err}", path.display()));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

#[allow(dead_code)]
pub fn apply_patch(
    root: &Path,
    patch: &str,
    allow_writes: bool,
) -> Result<ApplyPatchResult, String> {
    apply_patch_limited(root, patch, allow_writes, DEFAULT_PATCH_TARGET_LIMIT_BYTES)
}

pub fn apply_patch_limited(
    root: &Path,
    patch: &str,
    allow_writes: bool,
    max_target_bytes: i64,
) -> Result<ApplyPatchResult, String> {
    if !allow_writes {
        return Err("Writes are disabled.".to_string());
    }
    let max_target_bytes = normalized_patch_target_limit(max_target_bytes);
    let ops = match parse_patch(patch) {
        Ok(ops) => ops,
        Err(primary_err) => parse_replace_style_patch(patch).map_err(|fallback_err| {
            format!("{primary_err}; fallback parse failed: {fallback_err}")
        })?,
    };
    let transaction = PatchTransaction::capture(root, &ops, max_target_bytes)?;
    let operation_result = (|| {
        let mut result = ApplyPatchResult::default();

        for op in ops {
            match op {
                PatchOp::Add { path, lines } => {
                    let target = ensure_path_inside_root(root, Path::new(&path))?;
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                    }
                    let content = lines.join("\n");
                    ensure_patch_target_within_limit(
                        &target,
                        content.len() as u64,
                        max_target_bytes,
                    )?;
                    if target.exists()
                        && read_patch_target_to_string(&target, max_target_bytes)? == content
                    {
                        continue;
                    }
                    fs::write(&target, content).map_err(|err| err.to_string())?;
                    result.added.push(path);
                }
                PatchOp::Delete { path } => {
                    let target = ensure_path_inside_root(root, Path::new(&path))?;
                    let existed = target.exists();
                    if target.is_dir() {
                        fs::remove_dir_all(&target).map_err(|err| err.to_string())?;
                    } else if target.exists() {
                        fs::remove_file(&target).map_err(|err| err.to_string())?;
                    }
                    if existed {
                        result.deleted.push(path);
                    }
                }
                PatchOp::Replace {
                    path,
                    old_text,
                    new_text,
                } => {
                    let target = ensure_path_inside_root(root, Path::new(&path))?;
                    if !target.exists() {
                        return Err(format!("Target not found for replace: {path}"));
                    }
                    let original = read_patch_target_to_string(&target, max_target_bytes)?;
                    let output = replace_text_once(&original, &old_text, &new_text)?;
                    ensure_patch_target_within_limit(
                        &target,
                        output.len() as u64,
                        max_target_bytes,
                    )?;
                    if output == original {
                        continue;
                    }
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                    }
                    fs::write(&target, output).map_err(|err| err.to_string())?;
                    result.updated.push(path);
                }
                PatchOp::Update {
                    path,
                    move_to,
                    hunks,
                } => {
                    let target = ensure_path_inside_root(root, Path::new(&path))?;
                    let original = if target.exists() {
                        read_patch_target_to_string(&target, max_target_bytes)?
                    } else {
                        String::new()
                    };
                    let (orig_lines, eol, ends_with_eol) = split_lines(&original);
                    let next_lines = apply_hunks(&orig_lines, &hunks)?;
                    let output = join_lines(&next_lines, &eol, ends_with_eol);
                    ensure_patch_target_within_limit(
                        &target,
                        output.len() as u64,
                        max_target_bytes,
                    )?;
                    if output == original && move_to.is_none() {
                        continue;
                    }
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                    }
                    fs::write(&target, output).map_err(|err| err.to_string())?;
                    if let Some(move_to) = move_to {
                        let moved = ensure_path_inside_root(root, Path::new(&move_to))?;
                        if let Some(parent) = moved.parent() {
                            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                        }
                        fs::rename(&target, &moved).map_err(|err| err.to_string())?;
                        result.updated.push(move_to);
                    } else {
                        result.updated.push(path);
                    }
                }
            }
        }

        Ok(result)
    })();

    match operation_result {
        Ok(result) => Ok(result),
        Err(err) => match transaction.rollback() {
            Ok(()) => Err(err),
            Err(rollback_err) => Err(format!("{err}; patch rollback failed: {rollback_err}")),
        },
    }
}

fn patch_op_paths(op: &PatchOp) -> Vec<&str> {
    match op {
        PatchOp::Update { path, move_to, .. } => {
            let mut paths = vec![path.as_str()];
            if let Some(move_to) = move_to {
                paths.push(move_to.as_str());
            }
            paths
        }
        PatchOp::Add { path, .. } | PatchOp::Delete { path } | PatchOp::Replace { path, .. } => {
            vec![path.as_str()]
        }
    }
}

fn collect_missing_parent_dirs(
    root: &Path,
    parent: Option<&Path>,
    missing: &mut BTreeSet<PathBuf>,
) {
    let mut current = parent;
    while let Some(path) = current {
        if path == root || !path.starts_with(root) {
            break;
        }
        if path.exists() {
            break;
        }
        missing.insert(path.to_path_buf());
        current = path.parent();
    }
}

fn read_patch_target_to_string(path: &Path, max_target_bytes: u64) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|err| err.to_string())?;
    ensure_patch_target_within_limit(path, metadata.len(), max_target_bytes)?;
    fs::read_to_string(path).map_err(|err| err.to_string())
}

fn ensure_patch_target_within_limit(
    path: &Path,
    actual_bytes: u64,
    max_target_bytes: u64,
) -> Result<(), String> {
    if actual_bytes > max_target_bytes {
        return Err(format!(
            "Patch target exceeds write limit: {} bytes > {} bytes ({})",
            actual_bytes,
            max_target_bytes,
            path.display()
        ));
    }
    Ok(())
}

fn normalized_patch_target_limit(max_target_bytes: i64) -> u64 {
    if max_target_bytes <= 0 {
        DEFAULT_PATCH_TARGET_LIMIT_BYTES as u64
    } else {
        max_target_bytes as u64
    }
}
