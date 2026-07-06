// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

pub fn display_path(path: impl AsRef<str>) -> String {
    display_path_in_scope(path, None)
}

pub fn display_path_in_scope(path: impl AsRef<str>, scope_root: Option<&Path>) -> String {
    let raw = path.as_ref().trim();
    if raw.is_empty() {
        return String::new();
    }

    let normalized = normalize_path_for_compare(raw);
    if let Some(scope_root) = scope_root {
        let scope = normalize_path_for_compare(scope_root.to_string_lossy().as_ref());
        if normalized == scope {
            return "/".to_string();
        }
        let prefix = format!("{scope}/");
        if let Some(relative) = normalized.strip_prefix(prefix.as_str()) {
            return join_display_path("/", relative);
        }
    }

    if let Some(scoped) = user_scoped_root_match(normalized.as_str()) {
        let prefix = if scoped.kind == "public" {
            "/public"
        } else {
            "/"
        };
        return join_display_path(prefix, scoped.relative.as_str());
    }

    raw.to_string()
}

fn normalize_path_for_compare(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized
    }
}

fn join_display_path(prefix: &str, relative: &str) -> String {
    let clean_prefix = if prefix == "/" {
        ""
    } else {
        prefix.trim_end_matches('/')
    };
    let clean_relative = relative.trim_start_matches('/');
    if clean_relative.is_empty() {
        return if clean_prefix.is_empty() {
            "/".to_string()
        } else {
            clean_prefix.to_string()
        };
    }
    format!("{clean_prefix}/{clean_relative}")
}

struct UserScopedMatch {
    kind: String,
    relative: String,
}

fn user_scoped_root_match(path: &str) -> Option<UserScopedMatch> {
    let marker = "/users/";
    let users_index = path.find(marker)?;
    let after_users = &path[users_index + marker.len()..];
    let mut parts = after_users.splitn(3, '/');
    let user_component = parts.next()?;
    if user_component.trim().is_empty() {
        return None;
    }
    let kind = parts.next()?;
    if kind != "workspaces" && kind != "public" {
        return None;
    }
    Some(UserScopedMatch {
        kind: kind.to_string(),
        relative: parts.next().unwrap_or("").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{display_path, display_path_in_scope};
    use std::path::Path;

    #[test]
    fn display_path_hides_user_workspace_prefix() {
        let root = "/opt/chatos/backend/data/workspace/users/user-123/workspaces";
        assert_eq!(display_path(root), "/");
        assert_eq!(display_path(format!("{root}/demo/src")), "/demo/src");
    }

    #[test]
    fn display_path_marks_public_space() {
        let root = "/opt/chatos/backend/data/workspace/users/user-123/public";
        assert_eq!(display_path(root), "/public");
        assert_eq!(
            display_path(format!("{root}/keys/id_rsa")),
            "/public/keys/id_rsa"
        );
    }

    #[test]
    fn display_path_can_be_scoped_to_project_root() {
        let project =
            Path::new("/opt/chatos/backend/data/workspace/users/user-123/workspaces/demo");
        assert_eq!(
            display_path_in_scope(project.to_string_lossy(), Some(project)),
            "/"
        );
        assert_eq!(
            display_path_in_scope(
                "/opt/chatos/backend/data/workspace/users/user-123/workspaces/demo/src/main.rs",
                Some(project),
            ),
            "/src/main.rs"
        );
    }
}
