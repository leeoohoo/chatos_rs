use std::env;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use super::{FsPolicyError, PATH_OUTSIDE_ALLOWED_ROOTS, PATH_TRAVERSAL_BLOCKED};
use crate::core::path_guard::{
    canonicalize_existing_dir as canonicalize_existing_dir_shared,
    normalize_path_for_compare as normalize_path_for_compare_shared,
    path_is_within_root as path_is_within_root_shared,
};

pub(super) fn resolve_input_path(raw: &str) -> Result<PathBuf, FsPolicyError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(FsPolicyError::BadRequest("路径不能为空".to_string()));
    }

    let candidate = PathBuf::from(trimmed);
    if contains_parent_dir(candidate.as_path()) {
        return Err(FsPolicyError::Forbidden(PATH_TRAVERSAL_BLOCKED.to_string()));
    }

    if candidate.is_absolute() {
        return Ok(candidate);
    }

    let current_dir = env::current_dir().map_err(|err| FsPolicyError::Internal(err.to_string()))?;
    Ok(current_dir.join(candidate))
}

fn contains_parent_dir(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

pub(super) fn canonicalize_existing_dir(path: &Path) -> Result<PathBuf, std::io::Error> {
    canonicalize_existing_dir_shared(path)
}

pub(super) fn canonicalize_existing_path(
    path: &Path,
    missing_message: &str,
) -> Result<PathBuf, FsPolicyError> {
    canonicalize_path(path).map_err(|err| match err.kind() {
        ErrorKind::NotFound => FsPolicyError::BadRequest(missing_message.to_string()),
        ErrorKind::PermissionDenied => {
            FsPolicyError::Forbidden(PATH_OUTSIDE_ALLOWED_ROOTS.to_string())
        }
        _ => FsPolicyError::Internal(err.to_string()),
    })
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    std::fs::canonicalize(path).map(normalize_canonical_path)
}

pub(super) fn path_is_within_root(candidate: &Path, root: &Path) -> bool {
    path_is_within_root_shared(candidate, root)
}

pub(in super::super) fn normalize_path_for_compare(path: &Path) -> String {
    normalize_path_for_compare_shared(path)
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
