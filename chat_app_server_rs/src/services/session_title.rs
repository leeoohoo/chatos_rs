use crate::models::session::SessionService;

fn is_default_title(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    matches!(t.as_str(), "new chat" | "untitled" | "new session") || t.is_empty()
}

fn derive_title_from_content(text: &str, max_len: usize) -> String {
    let raw = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut first_line = raw.trim().split_whitespace().collect::<Vec<_>>().join(" ");
    if first_line.starts_with('#') {
        first_line = first_line.trim_start_matches('#').trim_start().to_string();
    }
    if first_line.starts_with('>') {
        first_line = first_line.trim_start_matches('>').trim_start().to_string();
    }
    if first_line.len() <= max_len {
        if first_line.is_empty() { "New Chat".to_string() } else { first_line }
    } else {
        format!("{}…", &first_line[..max_len])
    }
}

pub async fn maybe_rename_session_title(session_id: &str, user_content: &str, max_len: usize) -> bool {
    if session_id.is_empty() {
        return false;
    }
    if let Ok(Some(session)) = SessionService::get_by_id(session_id).await {
        if !is_default_title(&session.title) {
            return false;
        }
        let new_title = derive_title_from_content(user_content, max_len);
        if new_title != session.title {
            let _ = SessionService::update(session_id, Some(new_title), None, None).await;
            return true;
        }
    }
    false
}

