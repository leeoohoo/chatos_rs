// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::models::session::Session;
use crate::modules::conversation_runtime::session_scope::resolve_session_project_scope;

pub const CHATOS_CLIENT_SURFACE_HEADER: &str = "x-chatos-client-surface";
pub const LOCAL_CONNECTOR_DESKTOP_SURFACE: &str = "local-connector-desktop";

/// Validates the opaque client-owned project scope already persisted on the
/// conversation. ChatOS never resolves project identity or ownership itself.
pub fn ensure_session_project_scope(
    session: &Session,
    requested_project_id: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    let Some(requested_project_id) =
        crate::modules::conversation_runtime::session_scope::normalize_project_scope(
            requested_project_id,
        )
    else {
        return Ok(());
    };
    let session_project_id =
        resolve_session_project_scope(session.project_id.as_deref(), session.metadata.as_ref());
    if session_project_id.as_deref() != Some(requested_project_id.as_str()) {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "code": "project_context_mismatch",
                "error": "请求项目与对话绑定的客户端项目不一致",
            })),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;

    use super::ensure_session_project_scope;
    use crate::models::session::Session;

    #[test]
    fn client_owned_project_scope_needs_no_server_record() {
        let session = Session::new(
            "Conversation".to_string(),
            None,
            None,
            Some("user-1".to_string()),
            Some("client-project".to_string()),
        );

        assert!(ensure_session_project_scope(&session, None).is_ok());
        assert!(ensure_session_project_scope(&session, Some("client-project")).is_ok());
    }

    #[test]
    fn explicit_request_cannot_escape_the_owned_conversation_scope() {
        let session = Session::new(
            "Conversation".to_string(),
            None,
            Some(json!({ "chat_runtime": { "project_id": "client-project" } })),
            Some("user-1".to_string()),
            None,
        );

        let (status, body) =
            ensure_session_project_scope(&session, Some("another-project")).unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body.0["code"], "project_context_mismatch");
    }
}
