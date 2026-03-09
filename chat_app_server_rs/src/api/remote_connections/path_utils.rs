pub(super) fn shell_quote(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            escaped.push_str("'\\''");
        } else {
            escaped.push(ch);
        }
    }
    escaped.push('\'');
    escaped
}

pub(super) fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return ".".to_string();
    }

    let mut compact = trimmed.replace("\\", "/");
    while compact.contains("//") {
        compact = compact.replace("//", "/");
    }

    if compact != "/" {
        compact = compact.trim_end_matches('/').to_string();
    }

    if compact.is_empty() {
        ".".to_string()
    } else {
        compact
    }
}

pub(super) fn join_remote_path(parent: &str, name: &str) -> String {
    let parent = normalize_remote_path(parent);
    if parent == "." {
        return name.to_string();
    }
    if parent == "/" {
        return format!("/{name}");
    }
    format!("{parent}/{name}")
}

pub(super) fn remote_parent_path(path: &str) -> Option<String> {
    let path = normalize_remote_path(path);
    if path == "." || path == "/" {
        return None;
    }

    let mut parts = path
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    parts.pop();
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

pub(super) fn input_triggers_busy(data: &str) -> bool {
    if data.is_empty() {
        return false;
    }
    if data.contains('\r') || data.contains('\n') {
        return true;
    }
    data.as_bytes()
        .iter()
        .any(|b| matches!(*b, 0x03 | 0x04 | 0x1A))
}
