// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

use super::parser::split_local_shell_words;

pub(crate) fn path_is_inside_root(candidate: &Path, root: &Path) -> bool {
    let candidate = normalize_path_for_guard(candidate);
    let root = normalize_path_for_guard(root);
    candidate == root || candidate.starts_with(format!("{root}/").as_str())
}

pub(crate) fn normalize_path_for_guard(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    if cfg!(windows) {
        normalized = normalized.to_ascii_lowercase();
    }
    normalized
}

pub(super) fn validate_local_terminal_path_arguments(
    line: &str,
    root_cwd: &Path,
    current_cwd: &Path,
) -> Option<String> {
    let words = split_local_shell_words(line.trim())?;
    for word in words {
        let word = word.trim();
        if word.is_empty() || word.starts_with('-') || word.contains("://") {
            continue;
        }
        if word.starts_with('~') {
            return Some(
                "Blocked: paths outside the terminal workspace are not allowed.".to_string(),
            );
        }
        let candidate = Path::new(word);
        if candidate.is_absolute() {
            let Ok(canonical) = fs::canonicalize(candidate) else {
                return Some("Blocked: cannot verify absolute path target.".to_string());
            };
            if !path_is_inside_root(canonical.as_path(), root_cwd) {
                return Some(
                    "Blocked: paths outside the terminal workspace are not allowed.".to_string(),
                );
            }
        } else if candidate.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            let resolved = current_cwd.join(candidate);
            let Ok(canonical) = fs::canonicalize(resolved.as_path()) else {
                return Some("Blocked: cannot verify parent-directory path target.".to_string());
            };
            if !path_is_inside_root(canonical.as_path(), root_cwd) {
                return Some(
                    "Blocked: paths outside the terminal workspace are not allowed.".to_string(),
                );
            }
        }
    }
    None
}
