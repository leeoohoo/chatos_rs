use std::path::{Path, PathBuf};

use crate::models::project::ProjectService;
use serde_json::Value;

use super::BoundContext;

pub(super) async fn resolve_project_root(
    ctx: &BoundContext,
) -> Result<(Option<String>, PathBuf), String> {
    if let Some(project_id) = ctx.project_id.as_deref() {
        let project = ProjectService::get_by_id(project_id)
            .await?
            .ok_or_else(|| format!("project not found: {}", project_id))?;
        let root = canonicalize_path(Path::new(project.root_path.as_str()))?;
        return Ok((Some(project.id), root));
    }

    let root = canonicalize_path(ctx.root.as_path())?;
    if let Some(found) = infer_project_id_from_root(root.as_path(), ctx.user_id.as_deref()).await {
        return Ok((Some(found), root));
    }
    Ok((None, root))
}

async fn infer_project_id_from_root(root: &Path, user_id: Option<&str>) -> Option<String> {
    let list = ProjectService::list(user_id.map(|v| v.to_string()))
        .await
        .ok()?;
    for project in list {
        let p = PathBuf::from(project.root_path.as_str());
        if let Ok(project_root) = canonicalize_path(p.as_path()) {
            if same_path(project_root.as_path(), root) {
                return Some(project.id);
            }
        }
    }
    None
}

pub(super) fn resolve_target_path(
    project_root: &Path,
    path_input: &str,
) -> Result<PathBuf, String> {
    let trimmed = path_input.trim();
    let candidate = if trimmed.is_empty() || trimmed == "." {
        project_root.to_path_buf()
    } else if Path::new(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        project_root.join(trimmed)
    };

    if !candidate.exists() {
        return Err(format!("path does not exist: {}", candidate.display()));
    }
    if !candidate.is_dir() {
        return Err(format!("path is not a directory: {}", candidate.display()));
    }

    let canonical = canonicalize_path(candidate.as_path())?;
    if !is_path_within_root(canonical.as_path(), project_root) {
        return Err(format!(
            "path escaped project root: {} not in {}",
            canonical.display(),
            project_root.display()
        ));
    }
    Ok(canonical)
}

pub(super) fn build_input_payload(
    project_root: &Path,
    target_path: &Path,
    command: &str,
) -> String {
    let mut payload = String::new();
    payload.push_str(cd_command_for_path(project_root).as_str());

    if !same_path(target_path, project_root) {
        payload.push_str(cd_command_for_path(target_path).as_str());
    }

    let normalized_command = normalize_shell_input(command);
    payload.push_str(normalized_command.as_str());
    if !normalized_command.ends_with('\n') && !normalized_command.ends_with('\r') {
        payload.push_str(shell_input_newline());
    }

    payload
}

pub(super) fn terminal_cwd_in_root(cwd: &str, root: &Path) -> bool {
    let path = PathBuf::from(cwd);
    let canonical = match canonicalize_path(path.as_path()) {
        Ok(v) => v,
        Err(_) => return false,
    };
    is_path_within_root(canonical.as_path(), root)
}

pub(super) fn canonicalize_path(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path)
        .map(normalize_canonical_path)
        .map_err(|err| format!("canonicalize {} failed: {}", path.display(), err))
}

fn cd_command_for_path(path: &Path) -> String {
    if cfg!(windows) {
        return format!("cd /d {}{}", shell_quote_path(path), shell_input_newline());
    }
    format!("cd {}{}", shell_quote_path(path), shell_input_newline())
}

fn shell_input_newline() -> &'static str {
    if cfg!(windows) {
        "\r"
    } else {
        "\n"
    }
}

pub(super) fn normalize_shell_input(input: &str) -> String {
    if !cfg!(windows) {
        return input.to_string();
    }

    input.replace("\r\n", "\r").replace('\n', "\r")
}

fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    if !cfg!(windows) {
        return path;
    }

    let raw = path.to_string_lossy().to_string();
    if let Some(stripped) = raw.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{}", stripped));
    }
    if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        return PathBuf::from(stripped);
    }
    path
}

fn same_path(left: &Path, right: &Path) -> bool {
    canonicalize_path(left)
        .ok()
        .zip(canonicalize_path(right).ok())
        .map(|(a, b)| a == b)
        .unwrap_or(false)
}

fn is_path_within_root(path: &Path, root: &Path) -> bool {
    let root = match canonicalize_path(root) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let path = match canonicalize_path(path) {
        Ok(v) => v,
        Err(_) => return false,
    };
    path == root || path.starts_with(root)
}

pub(super) fn derive_terminal_name(root: &Path) -> String {
    root.file_name()
        .map(|s| format!("{}-terminal", s.to_string_lossy()))
        .unwrap_or_else(|| "project-terminal".to_string())
}

fn shell_quote_path(path: &Path) -> String {
    let raw = path.to_string_lossy().to_string();
    if cfg!(windows) {
        return format!("\"{}\"", raw.replace('"', "\"\""));
    }
    format!("'{}'", raw.replace('"', "\\\"").replace('\'', "'\"'\"'"))
}

fn required_string<'a>(args: &'a Value, field: &str) -> Result<&'a str, String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{field} is required"))
}

pub(super) fn required_trimmed_string(args: &Value, field: &str) -> Result<String, String> {
    let value = required_string(args, field)?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(trimmed.to_string())
}
