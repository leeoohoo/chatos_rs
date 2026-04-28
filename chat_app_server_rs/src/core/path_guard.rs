use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub fn canonicalize_existing_dir(path: &Path) -> Result<PathBuf, std::io::Error> {
    let canonical = canonicalize_path(path)?;
    if !canonical.is_dir() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidInput,
            "not a directory",
        ));
    }
    Ok(canonical)
}

pub fn normalize_path_for_compare(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");

    if let Some(stripped) = normalized.strip_prefix("//?/UNC/") {
        normalized = format!("//{}", stripped);
    } else if let Some(stripped) = normalized.strip_prefix("//?/") {
        normalized = stripped.to_string();
    }

    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }

    if cfg!(windows) {
        normalized = normalized.to_ascii_lowercase();
    }

    normalized
}

pub fn path_is_within_root(candidate: &Path, root: &Path) -> bool {
    let candidate_norm = normalize_path_for_compare(candidate);
    let root_norm = normalize_path_for_compare(root);

    if candidate_norm == root_norm {
        return true;
    }

    let prefix = format!("{root_norm}/");
    candidate_norm.starts_with(&prefix)
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    std::fs::canonicalize(path).map(normalize_canonical_path)
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

#[cfg(test)]
mod tests {
    use super::{normalize_path_for_compare, path_is_within_root};
    use std::path::PathBuf;

    #[test]
    fn path_within_root_accepts_exact_match() {
        let root = PathBuf::from("/tmp/demo");
        assert!(path_is_within_root(root.as_path(), root.as_path()));
    }

    #[test]
    fn path_within_root_accepts_child_path() {
        let root = PathBuf::from("/tmp/demo");
        let child = PathBuf::from("/tmp/demo/src/main.rs");
        assert!(path_is_within_root(child.as_path(), root.as_path()));
    }

    #[test]
    fn path_within_root_rejects_sibling_path() {
        let root = PathBuf::from("/tmp/demo");
        let sibling = PathBuf::from("/tmp/demo-elsewhere/src/main.rs");
        assert!(!path_is_within_root(sibling.as_path(), root.as_path()));
    }

    #[test]
    fn normalize_compare_trims_trailing_separator() {
        assert_eq!(
            normalize_path_for_compare(PathBuf::from("/tmp/demo/").as_path()),
            "/tmp/demo"
        );
    }
}
