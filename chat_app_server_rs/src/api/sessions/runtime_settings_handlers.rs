use axum::{extract::Path, http::StatusCode, Json};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::models::session::Session;
use crate::models::session_runtime_settings::SessionRuntimeSettings;
use crate::repositories::session_runtime_settings;

use super::contracts::UpdateSessionRuntimeSettingsRequest;

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_id_list(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|item: &String| item == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn as_object(value: Option<&Value>) -> Option<&serde_json::Map<String, Value>> {
    value.and_then(Value::as_object)
}

fn metadata_source(metadata: Option<&Value>) -> Option<&serde_json::Map<String, Value>> {
    let meta = metadata.and_then(Value::as_object)?;
    if let Some(source) = meta
        .get("source_metadata")
        .and_then(Value::as_object)
        .filter(|source| !source.is_empty())
    {
        return Some(source);
    }
    Some(meta)
}

fn read_text_from_objects(
    objects: &[Option<&serde_json::Map<String, Value>>],
    keys: &[&str],
) -> Option<String> {
    for object in objects {
        let Some(object) = object else {
            continue;
        };
        for key in keys {
            if let Some(value) = object.get(*key).and_then(Value::as_str) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

fn read_bool(
    object: Option<&serde_json::Map<String, Value>>,
    keys: &[&str],
    fallback: bool,
) -> bool {
    let Some(object) = object else {
        return fallback;
    };
    for key in keys {
        if let Some(value) = object.get(*key).and_then(Value::as_bool) {
            return value;
        }
    }
    fallback
}

fn read_id_list(object: Option<&serde_json::Map<String, Value>>, keys: &[&str]) -> Vec<String> {
    let Some(object) = object else {
        return Vec::new();
    };
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(items) = value.as_array() {
            return normalize_id_list(
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect(),
            );
        }
    }
    Vec::new()
}

fn fallback_from_session(session: &Session, user_id: &str) -> SessionRuntimeSettings {
    let mut settings = SessionRuntimeSettings::new(session.id.clone(), user_id.to_string());
    let source = metadata_source(session.metadata.as_ref());
    let runtime = as_object(source.and_then(|item| item.get("chat_runtime")));
    let ui_chat_selection = as_object(source.and_then(|item| item.get("ui_chat_selection")));

    settings.selected_model_id = read_text_from_objects(
        &[runtime, ui_chat_selection],
        &["selected_model_id", "selectedModelId"],
    );
    settings.selected_model_name = read_text_from_objects(
        &[runtime, ui_chat_selection],
        &["selected_model_name", "selectedModelName"],
    );
    settings.selected_thinking_level = read_text_from_objects(
        &[runtime, ui_chat_selection],
        &["selected_thinking_level", "selectedThinkingLevel"],
    );
    settings.remote_connection_id =
        read_text_from_objects(&[runtime], &["remote_connection_id", "remoteConnectionId"]);
    settings.workspace_root =
        read_text_from_objects(&[runtime], &["workspace_root", "workspaceRoot"]);
    settings.mcp_enabled = read_bool(runtime, &["mcp_enabled", "mcpEnabled"], true);
    settings.enabled_mcp_ids = read_id_list(runtime, &["enabled_mcp_ids", "enabledMcpIds"]);
    settings.auto_create_task = read_bool(runtime, &["auto_create_task", "autoCreateTask"], false);
    settings
}

async fn load_or_default(
    session: &Session,
    auth: &AuthUser,
) -> Result<SessionRuntimeSettings, String> {
    let existing = session_runtime_settings::get_session_runtime_settings(
        session.id.as_str(),
        auth.user_id.as_str(),
    )
    .await?;
    Ok(existing.unwrap_or_else(|| fallback_from_session(session, auth.user_id.as_str())))
}

pub(super) async fn get_session_runtime_settings(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_owned_session(&conversation_id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error(err),
    };

    match load_or_default(&session, &auth).await {
        Ok(settings) => (
            StatusCode::OK,
            Json(serde_json::to_value(settings).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn update_session_runtime_settings(
    auth: AuthUser,
    Path(conversation_id): Path<String>,
    Json(req): Json<UpdateSessionRuntimeSettingsRequest>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_owned_session(&conversation_id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error(err),
    };

    let mut next = match load_or_default(&session, &auth).await {
        Ok(settings) => settings,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err})),
            );
        }
    };
    next.session_id = conversation_id;
    next.user_id = auth.user_id.clone();

    if let Some(value) = req.selected_model_id {
        next.selected_model_id = normalize_optional_text(value);
    }
    if let Some(value) = req.selected_model_name {
        next.selected_model_name = normalize_optional_text(value);
    }
    if let Some(value) = req.selected_thinking_level {
        next.selected_thinking_level = normalize_optional_text(value);
    }
    if let Some(value) = req.remote_connection_id {
        next.remote_connection_id = normalize_optional_text(value);
    }
    if let Some(value) = req.workspace_root {
        next.workspace_root = normalize_optional_text(value);
    }
    if let Some(value) = req.mcp_enabled {
        next.mcp_enabled = value;
    }
    if let Some(value) = req.enabled_mcp_ids {
        next.enabled_mcp_ids = normalize_id_list(value);
    }
    if let Some(value) = req.auto_create_task {
        next.auto_create_task = value;
    }

    match session_runtime_settings::upsert_session_runtime_settings(&next).await {
        Ok(saved) => (
            StatusCode::OK,
            Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}
