use std::path::{Path, PathBuf};

pub(super) fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|err| err.to_string())
}

pub(super) fn resolve_target_path(root: &Path, path_input: &str) -> Result<PathBuf, String> {
    let trimmed = path_input.trim();
    let joined = if trimmed.is_empty() || trimmed == "." {
        root.to_path_buf()
    } else {
        let path = PathBuf::from(trimmed);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    };
    let canonical = canonicalize_existing(joined.as_path())?;
    if !canonical.starts_with(root) {
        return Err("target path escapes workspace root".to_string());
    }
    Ok(canonical)
}

pub(super) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
