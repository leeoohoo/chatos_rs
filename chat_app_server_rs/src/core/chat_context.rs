use crate::models::session::SessionService;
use crate::repositories::system_contexts;
use crate::services::session_title::maybe_rename_session_title;

pub fn maybe_spawn_session_title_rename(
    enabled: bool,
    session_id: &str,
    content: &str,
    max_len: usize,
) {
    if !enabled || session_id.is_empty() || content.is_empty() {
        return;
    }

    let sid = session_id.to_string();
    let text = content.to_string();
    tokio::spawn(async move {
        let _ = maybe_rename_session_title(&sid, &text, max_len).await;
    });
}

pub async fn resolve_effective_user_id(
    explicit_user_id: Option<String>,
    session_id: &str,
) -> Option<String> {
    if explicit_user_id.is_some() || session_id.is_empty() {
        return explicit_user_id;
    }

    match SessionService::get_by_id(session_id).await {
        Ok(Some(session)) => session.user_id,
        _ => None,
    }
}

pub async fn resolve_system_prompt(
    explicit_prompt: Option<String>,
    use_active_system_context: bool,
    user_id: Option<String>,
) -> Option<String> {
    if explicit_prompt.is_some() {
        return explicit_prompt;
    }

    if !use_active_system_context {
        return None;
    }

    let Some(uid) = user_id else {
        return None;
    };

    match system_contexts::get_active_system_context(&uid).await {
        Ok(Some(ctx)) => ctx.content,
        _ => None,
    }
}
