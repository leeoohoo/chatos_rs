use std::path::{Path, PathBuf};

pub(super) fn canonicalize_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    std::fs::canonicalize(path).map(normalize_canonical_path)
}

pub(super) fn path_is_within_root(candidate: &Path, root: &Path) -> bool {
    let candidate_norm = normalize_path_for_compare(candidate);
    let root_norm = normalize_path_for_compare(root);

    if candidate_norm == root_norm {
        return true;
    }

    let prefix = format!("{root_norm}/");
    candidate_norm.starts_with(&prefix)
}

pub(super) fn normalize_path_for_compare(path: &Path) -> String {
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

pub(super) fn shell_quote_path_for_shell(path: &Path) -> String {
    let raw = path.to_string_lossy().to_string();
    if cfg!(windows) {
        return format!("\"{}\"", raw.replace('"', "\"\""));
    }
    format!("'{}'", raw.replace('"', "\\\"").replace('\'', "'\"'\"'"))
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
