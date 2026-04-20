use crate::services::memory_server_client;

fn is_default_title(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    matches!(
        t.as_str(),
        "new chat" | "untitled" | "new session" | "new conversation" | "新对话" | "新会话"
    ) || t.is_empty()
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
    if first_line.is_empty() {
        return "New Conversation".to_string();
    }

    // String slicing uses byte offsets; char_indices yields safe UTF-8 boundaries.
    match first_line.char_indices().nth(max_len) {
        Some((cutoff, _)) => format!("{}...", &first_line[..cutoff]),
        None => first_line,
    }
}

pub async fn maybe_rename_session_title(
    session_id: &str,
    user_content: &str,
    max_len: usize,
) -> bool {
    if session_id.is_empty() {
        return false;
    }

    if let Ok(Some(session)) = memory_server_client::get_session_by_id(session_id).await {
        if !is_default_title(&session.title) {
            return false;
        }
        let new_title = derive_title_from_content(user_content, max_len);
        if new_title != session.title {
            let _ =
                memory_server_client::update_session(session_id, Some(new_title), None, None).await;
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::derive_title_from_content;

    #[test]
    fn keeps_short_title_unchanged() {
        assert_eq!(derive_title_from_content("hello world", 30), "hello world");
    }

    #[test]
    fn truncates_multibyte_title_on_char_boundary() {
        let content = String::from_utf8(vec![
            0x61, 0x62, 0x63, 0xE4, 0xBD, 0xA0, 0xE5, 0xA5, 0xBD, 0xE4, 0xB8, 0x96, 0xE7, 0x95,
            0x8C, 0x64, 0x65, 0x66,
        ])
        .expect("valid utf8");

        let title = derive_title_from_content(&content, 5);
        let expected =
            String::from_utf8(vec![0x61, 0x62, 0x63, 0xE4, 0xBD, 0xA0, 0xE5, 0xA5, 0xBD])
                .expect("valid utf8");

        assert_eq!(title, format!("{}...", expected));
    }
}
