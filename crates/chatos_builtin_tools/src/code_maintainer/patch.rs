use super::utils::ensure_path_inside_root;
use std::fs;
use std::path::Path;

mod hunks;
mod parser;
mod replacement;

#[cfg(test)]
mod tests;

use hunks::{apply_hunks, join_lines, split_lines};
use parser::{parse_patch, parse_replace_style_patch};
use replacement::replace_text_once;

#[derive(Debug, Default, serde::Serialize)]
pub struct ApplyPatchResult {
    pub updated: Vec<String>,
    pub added: Vec<String>,
    pub deleted: Vec<String>,
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

pub fn apply_patch(
    root: &Path,
    patch: &str,
    allow_writes: bool,
) -> Result<ApplyPatchResult, String> {
    if !allow_writes {
        return Err("Writes are disabled.".to_string());
    }
    let ops = match parse_patch(patch) {
        Ok(ops) => ops,
        Err(primary_err) => parse_replace_style_patch(patch).map_err(|fallback_err| {
            format!("{primary_err}; fallback parse failed: {fallback_err}")
        })?,
    };
    let mut result = ApplyPatchResult::default();

    for op in ops {
        match op {
            PatchOp::Add { path, lines } => {
                let target = ensure_path_inside_root(root, Path::new(&path))?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                }
                let content = lines.join("\n");
                fs::write(&target, content).map_err(|err| err.to_string())?;
                result.added.push(path);
            }
            PatchOp::Delete { path } => {
                let target = ensure_path_inside_root(root, Path::new(&path))?;
                if target.is_dir() {
                    fs::remove_dir_all(&target).map_err(|err| err.to_string())?;
                } else if target.exists() {
                    fs::remove_file(&target).map_err(|err| err.to_string())?;
                }
                result.deleted.push(path);
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
                let original = fs::read_to_string(&target).map_err(|err| err.to_string())?;
                let output = replace_text_once(&original, &old_text, &new_text)?;
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
                    fs::read_to_string(&target).map_err(|err| err.to_string())?
                } else {
                    String::new()
                };
                let (orig_lines, eol, ends_with_eol) = split_lines(&original);
                let next_lines = apply_hunks(&orig_lines, &hunks)?;
                let output = join_lines(&next_lines, &eol, ends_with_eol);
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
}
