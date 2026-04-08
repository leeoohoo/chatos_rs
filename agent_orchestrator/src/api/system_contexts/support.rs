use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::models::system_context::SystemContext;
use crate::services::system_context_ai::SystemContextAiError;

pub(super) fn map_system_context_ai_error(err: SystemContextAiError) -> (StatusCode, Json<Value>) {
    match err {
        SystemContextAiError::BadRequest { message } => {
            (StatusCode::BAD_REQUEST, Json(json!({"error": message})))
        }
        SystemContextAiError::Upstream { message, raw } => {
            let mut body = json!({"error": message});
            if let Some(raw) = raw {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("raw".to_string(), Value::String(raw));
                }
            }
            (StatusCode::BAD_GATEWAY, Json(body))
        }
    }
}

pub(super) fn system_context_value(ctx: &SystemContext, app_ids: Option<Vec<String>>) -> Value {
    let mut obj = json!({
        "id": ctx.id.clone(),
        "name": ctx.name.clone(),
        "content": ctx.content.clone(),
        "user_id": ctx.user_id.clone(),
        "is_active": ctx.is_active,
        "created_at": ctx.created_at.clone(),
        "updated_at": ctx.updated_at.clone(),
    });
    if let Some(ids) = app_ids {
        obj["app_ids"] = json!(ids);
    }
    obj
}
