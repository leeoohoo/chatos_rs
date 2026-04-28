use std::path::PathBuf;

pub(super) fn home_dir() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("HOME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    if let Ok(value) = std::env::var("USERPROFILE") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    let drive = std::env::var("HOMEDRIVE").ok();
    let path = std::env::var("HOMEPATH").ok();
    if let (Some(d), Some(p)) = (drive, path) {
        let d = d.trim();
        let p = p.trim();
        if !d.is_empty() || !p.is_empty() {
            return Some(PathBuf::from(format!("{}{}", d, p)));
        }
    }
    None
}
