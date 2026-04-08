use std::env;
use std::path::{Path, PathBuf};

pub const DEFAULT_WORKSPACE_DIR: &str = "~/.agent_workspace";

pub fn default_workspace_dir() -> String {
    if let Some(legacy) = resolve_legacy_workspace_dir() {
        return legacy;
    }
    if cfg!(windows) {
        if let Some(home) = home_dir() {
            let mut out = home;
            if !out.ends_with('\\') && !out.ends_with('/') {
                out.push('\\');
            }
            out.push_str(".agent_workspace");
            return out;
        }
        return ".agent_workspace".to_string();
    }
    DEFAULT_WORKSPACE_DIR.to_string()
}

pub fn normalize_workspace_dir(raw: Option<&str>) -> String {
    let value = raw.unwrap_or("").trim();
    if value.is_empty() {
        default_workspace_dir()
    } else {
        value.to_string()
    }
}

pub fn sanitize_workspace_dir(raw: Option<String>) -> Option<String> {
    raw.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn resolve_workspace_dir(raw: Option<&str>) -> String {
    let normalized = normalize_workspace_dir(raw);
    let expanded = expand_tilde(&normalized);
    expand_env_vars(&expanded)
}

fn expand_tilde(path: &str) -> String {
    if path == "~" || path.starts_with("~/") || path.starts_with("~\\") {
        if let Some(home) = home_dir() {
            let suffix = &path[1..];
            if suffix.is_empty() {
                return home;
            }
            return format!("{}{}", home, suffix);
        }
    }
    path.to_string()
}

fn expand_env_vars(path: &str) -> String {
    let mut out = String::new();
    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let mut name = String::new();
            let mut closed = false;
            while let Some(&c) = chars.peek() {
                chars.next();
                if c == '%' {
                    closed = true;
                    break;
                }
                name.push(c);
            }
            if closed && !name.is_empty() {
                if let Ok(val) = env::var(&name) {
                    if !val.is_empty() {
                        out.push_str(&val);
                        continue;
                    }
                }
            }
            out.push('%');
            out.push_str(&name);
            if closed {
                out.push('%');
            }
            continue;
        }
        if ch == '$' {
            if let Some(&'{') = chars.peek() {
                chars.next();
                let mut name = String::new();
                let mut closed = false;
                while let Some(c) = chars.next() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    name.push(c);
                }
                if closed && !name.is_empty() {
                    if let Ok(val) = env::var(&name) {
                        if !val.is_empty() {
                            out.push_str(&val);
                            continue;
                        }
                    }
                }
                out.push_str("${");
                out.push_str(&name);
                if closed {
                    out.push('}');
                }
                continue;
            }
            let mut name = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    name.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            if !name.is_empty() {
                if let Ok(val) = env::var(&name) {
                    if !val.is_empty() {
                        out.push_str(&val);
                        continue;
                    }
                }
                out.push('$');
                out.push_str(&name);
                continue;
            }
        }
        out.push(ch);
    }
    out
}

fn home_dir() -> Option<String> {
    if let Ok(value) = env::var("HOME") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    if let Ok(value) = env::var("USERPROFILE") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    let drive = env::var("HOMEDRIVE").ok();
    let path = env::var("HOMEPATH").ok();
    if let (Some(d), Some(p)) = (drive, path) {
        let d = d.trim().to_string();
        let p = p.trim().to_string();
        if !d.is_empty() || !p.is_empty() {
            return Some(format!("{}{}", d, p));
        }
    }
    None
}

fn resolve_legacy_workspace_dir() -> Option<String> {
    let home = home_dir()?;
    let current = if cfg!(windows) {
        format_path_under_home(&home, ".agent_workspace", true)
    } else {
        format!("{}/.agent_workspace", home)
    };
    if Path::new(&current).exists() {
        return None;
    }

    let home_path = PathBuf::from(&home);
    let Ok(entries) = std::fs::read_dir(home_path) else {
        return None;
    };
    for entry in entries.flatten() {
        let candidate = entry.path();
        let Some(name) = candidate.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !candidate.is_dir() || !name.starts_with('.') || !name.ends_with("_workspace") {
            continue;
        }
        return Some(candidate.to_string_lossy().to_string());
    }
    None
}

fn format_path_under_home(home: &str, segment: &str, windows: bool) -> String {
    let mut out = home.to_string();
    if windows {
        if !out.ends_with('\\') && !out.ends_with('/') {
            out.push('\\');
        }
        out.push_str(segment);
        return out;
    }
    format!("{}/{}", out.trim_end_matches('/'), segment)
}
