use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::project_run_environment::ProjectRunToolchainOption;

use super::super::environment_support::{
    infer_version_suffix, normalize_path, normalize_string, option_id, option_version,
};

pub(super) type ToolchainOptions = BTreeMap<String, Vec<ProjectRunToolchainOption>>;
pub(super) type ToolchainSeen = HashSet<String>;

pub(super) fn push_option(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    kind: &str,
    path: PathBuf,
    source: &str,
    preferred_id: Option<&str>,
) {
    push_option_with_label(
        out,
        seen,
        kind,
        normalize_path(path.as_path()),
        source,
        preferred_id,
        None,
        false,
    );
}

pub(super) fn push_option_with_label(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    kind: &str,
    path: String,
    source: &str,
    preferred_id: Option<&str>,
    label: Option<String>,
    allow_missing: bool,
) {
    let normalized_path = normalize_string(path.as_str());
    if normalized_path.is_empty() {
        return;
    }
    let path_obj = Path::new(normalized_path.as_str());
    if !allow_missing && !path_obj.is_file() && !path_obj.is_dir() {
        return;
    }
    let unique_key = format!("{kind}:{normalized_path}");
    if !seen.insert(unique_key) {
        return;
    }
    let resolved_label = label.unwrap_or_else(|| infer_version_suffix(path_obj));
    let id = option_id(kind, normalized_path.as_str());
    out.entry(kind.to_string())
        .or_default()
        .push(ProjectRunToolchainOption {
            id: id.clone(),
            kind: kind.to_string(),
            label: resolved_label,
            version: option_version(normalized_path.as_str()),
            path: normalized_path,
            source: source.to_string(),
            is_default: preferred_id.is_some_and(|value| value == id),
        });
}

pub(super) fn push_if_exists(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    kind: &str,
    candidate: PathBuf,
    source: &str,
    preferred_id: Option<&str>,
) {
    if candidate.is_file() || candidate.is_dir() {
        push_option(out, seen, kind, candidate, source, preferred_id);
    }
}

pub(super) fn push_relative_option(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    kind: &str,
    project_root: &Path,
    relative_path: &str,
    source: &str,
    label: &str,
) {
    let candidate = project_root.join(relative_path);
    if candidate.is_file() || candidate.is_dir() {
        push_option_with_label(
            out,
            seen,
            kind,
            normalize_path(candidate.as_path()),
            source,
            None,
            Some(label.to_string()),
            false,
        );
    }
}

pub(super) fn discover_direct_file_option(
    out: &mut ToolchainOptions,
    seen: &mut ToolchainSeen,
    kind: &str,
    candidate: &Path,
    source: &str,
    label: &str,
) {
    if candidate.is_file() {
        push_option_with_label(
            out,
            seen,
            kind,
            normalize_path(candidate),
            source,
            None,
            Some(label.to_string()),
            false,
        );
    }
}

pub(super) fn list_child_dirs(root: &Path) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

pub(super) fn java_home_candidate(path: &Path) -> Option<PathBuf> {
    if path
        .join("Contents")
        .join("Home")
        .join("bin")
        .join("java")
        .is_file()
    {
        return Some(path.join("Contents").join("Home"));
    }
    if path.join("bin").join("java").is_file() {
        return Some(path.to_path_buf());
    }
    None
}
