use std::env;

pub const DEFAULT_WORKSPACE_DIR: &str = "~/.chatos_workspace";

pub fn default_workspace_dir() -> String {
    if cfg!(windows) {
        if let Some(home) = home_dir() {
            let mut out = home;
            if !out.ends_with('\\') && !out.ends_with('/') {
                out.push('\\');
            }
            out.push_str(".chatos_workspace");
            return out;
        }
        return ".chatos_workspace".to_string();
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
