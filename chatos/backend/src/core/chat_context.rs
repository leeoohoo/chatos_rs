// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::services::chatos_sessions;

pub async fn resolve_effective_user_id(
    explicit_user_id: Option<String>,
    session_id: &str,
) -> Option<String> {
    if explicit_user_id.is_some() || session_id.is_empty() {
        return explicit_user_id;
    }

    match chatos_sessions::get_session_by_id(session_id).await {
        Ok(Some(session)) => session.user_id,
        _ => None,
    }
}
