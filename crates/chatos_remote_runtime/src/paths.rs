// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return ".".to_string();
    }

    let mut normalized = trimmed.replace('\\', "/");
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    if normalized != "/" {
        normalized = normalized.trim_end_matches('/').to_string();
    }
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}

pub fn join_remote_path(parent: &str, name: &str) -> String {
    let parent = normalize_remote_path(parent);
    if parent == "." {
        return name.to_string();
    }
    if parent == "/" {
        return format!("/{name}");
    }
    format!("{parent}/{name}")
}

pub fn remote_parent_path(path: &str) -> Option<String> {
    let path = normalize_remote_path(path);
    if path == "." || path == "/" {
        return None;
    }

    let mut parts = path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    parts.pop()?;
    if parts.is_empty() {
        if path.starts_with('/') {
            Some("/".to_string())
        } else {
            Some(".".to_string())
        }
    } else if path.starts_with('/') {
        Some(format!("/{}", parts.join("/")))
    } else {
        Some(parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths_are_normalized_and_joined_consistently() {
        assert_eq!(normalize_remote_path("  /srv//app/  "), "/srv/app");
        assert_eq!(normalize_remote_path(""), ".");
        assert_eq!(join_remote_path("/", "file.txt"), "/file.txt");
        assert_eq!(
            join_remote_path("/srv/app/", "file.txt"),
            "/srv/app/file.txt"
        );
        assert_eq!(
            remote_parent_path("/srv/app/file.txt"),
            Some("/srv/app".to_string())
        );
        assert_eq!(remote_parent_path("file.txt"), Some(".".to_string()));
    }
}
