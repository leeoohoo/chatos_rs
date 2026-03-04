use std::path::PathBuf;

fn sanitize_segment(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }

    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }

    let compact = out.trim_matches('_').to_string();
    if compact.is_empty() {
        "unknown".to_string()
    } else {
        compact
    }
}

pub fn resolve_data_dir(user_id: &str, _project_id: Option<&str>) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let user_seg = sanitize_segment(user_id);
    let project_seg = "__global__".to_string();

    home.join(".chatos")
        .join("notepad")
        .join(user_seg)
        .join(project_seg)
}
