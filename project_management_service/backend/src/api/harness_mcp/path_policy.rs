// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(super) fn required_file_path(args: &Value) -> Result<String, String> {
    let value = args.get("path").and_then(Value::as_str);
    optional_repo_path(value, false)
}

pub(super) fn optional_repo_path(value: Option<&str>, allow_root: bool) -> Result<String, String> {
    let raw = value.unwrap_or(".");
    let trimmed = raw.trim();
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return Err("path must be relative to the Harness repo root".to_string());
    }
    if trimmed.contains('\0') {
        return Err("path contains a null byte".to_string());
    }
    let normalized = trimmed.replace('\\', "/");
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        let part = part.trim();
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err("path must not contain ..".to_string());
        }
        parts.push(part.to_string());
    }
    if parts.is_empty() {
        if allow_root {
            Ok(String::new())
        } else {
            Err("path is required".to_string())
        }
    } else {
        Ok(parts.join("/"))
    }
}

pub(super) fn path_name(path: &str) -> String {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub(super) fn path_matches_scope(path: &str, scope: &str) -> bool {
    if scope.is_empty() {
        return true;
    }
    path == scope
        || path
            .strip_prefix(scope)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_paths_reject_parent_traversal() {
        assert!(optional_repo_path(Some("../secret"), false).is_err());
        assert!(optional_repo_path(Some("/secret"), false).is_err());
        assert_eq!(
            optional_repo_path(Some("src/./main.rs"), false).unwrap(),
            "src/main.rs"
        );
    }
}
