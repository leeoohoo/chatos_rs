// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub fn normalize_plugin_relative_path(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if trimmed.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    if trimmed.contains('\\') {
        return Err("path must use forward slashes".to_string());
    }
    if trimmed.starts_with('/') || trimmed.starts_with('~') {
        return Err("path must be relative to the plugin root".to_string());
    }
    if trimmed.contains("://") || has_windows_drive_prefix(trimmed) {
        return Err("path must not be a URL or absolute drive path".to_string());
    }

    let without_prefix = trimmed.strip_prefix("./").unwrap_or(trimmed);
    let mut segments = Vec::new();
    for segment in without_prefix.split('/') {
        match segment {
            "" | "." => {}
            ".." => return Err("path traversal is not allowed".to_string()),
            _ => segments.push(segment),
        }
    }
    if segments.is_empty() {
        return Err("path must reference content inside the plugin root".to_string());
    }
    Ok(format!("./{}", segments.join("/")))
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
