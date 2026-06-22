use std::path::{Component, Path, PathBuf};

pub(super) fn is_path_within_scope(path: &str, scope: &str) -> bool {
    if path == scope {
        return true;
    }
    let prefix = if scope.ends_with(std::path::MAIN_SEPARATOR) {
        scope.to_string()
    } else {
        format!("{scope}{}", std::path::MAIN_SEPARATOR)
    };
    path.starts_with(prefix.as_str())
}

pub(super) fn path_matches_root(path: &str, root: &str) -> bool {
    if path == root {
        return true;
    }
    let prefix = if root.ends_with(std::path::MAIN_SEPARATOR) {
        root.to_string()
    } else {
        format!("{root}{}", std::path::MAIN_SEPARATOR)
    };
    path.starts_with(prefix.as_str())
}

pub(super) fn normalize_relative_string(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
            _ => {}
        }
    }
    normalized.to_string_lossy().replace('\\', "/")
}

pub(super) fn normalize_path_string(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut normalized = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Prefix(value) => normalized.push(value.as_os_str()),
            Component::RootDir => {
                let separator = std::path::MAIN_SEPARATOR.to_string();
                normalized.push(separator);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized.to_string_lossy().to_string()
}
