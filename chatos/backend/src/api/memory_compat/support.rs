// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use super::contracts::{CompatCreateMessageRequest, CompatSyncMessageRequest};
use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::modules::conversation_runtime::memory_compat as compat_runtime;

pub(super) fn compat_message_input_from_create(
    req: CompatCreateMessageRequest,
) -> compat_runtime::CompatMessageInput {
    compat_message_input(
        req.role,
        req.content,
        req.message_mode,
        req.message_source,
        req.tool_calls,
        req.tool_call_id,
        req.reasoning,
        req.metadata,
    )
}

pub(super) fn compat_message_input_from_sync(
    req: CompatSyncMessageRequest,
) -> compat_runtime::CompatMessageInput {
    compat_message_input(
        req.role,
        req.content,
        req.message_mode,
        req.message_source,
        req.tool_calls,
        req.tool_call_id,
        req.reasoning,
        req.metadata,
    )
}

fn compat_message_input(
    role: String,
    content: String,
    message_mode: Option<String>,
    message_source: Option<String>,
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
) -> compat_runtime::CompatMessageInput {
    compat_runtime::CompatMessageInput {
        role,
        content,
        message_mode,
        message_source,
        tool_calls,
        tool_call_id,
        reasoning,
        metadata,
    }
}

pub(super) fn compat_scoped_result<T>(
    result: Result<T, compat_runtime::CompatScopedOperationError>,
    context: &str,
) -> Result<T, (StatusCode, Json<Value>)> {
    match result {
        Ok(value) => Ok(value),
        Err(compat_runtime::CompatScopedOperationError::SessionAccess(err)) => {
            Err(compat_session_access_error(err))
        }
        Err(compat_runtime::CompatScopedOperationError::Internal(err)) => {
            Err(compat_internal_error(context, err))
        }
    }
}

pub(super) fn compat_message_result<T>(
    result: Result<T, compat_runtime::CompatMessageOperationError>,
    context: &str,
) -> Result<T, (StatusCode, Json<Value>)> {
    match result {
        Ok(value) => Ok(value),
        Err(compat_runtime::CompatMessageOperationError::NotFound) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "message not found"})),
        )),
        Err(compat_runtime::CompatMessageOperationError::SessionAccess(err)) => {
            Err(compat_session_access_error(err))
        }
        Err(compat_runtime::CompatMessageOperationError::Internal(err)) => {
            Err(compat_internal_error(context, err))
        }
    }
}

pub(super) fn resolve_scope_user_id(
    requested_user_id: Option<String>,
    auth: &AuthUser,
) -> Result<String, (StatusCode, Json<Value>)> {
    resolve_user_id(requested_user_id, auth)
}

pub(super) fn compat_session_access_error(
    err: crate::core::session_access::SessionAccessError,
) -> (StatusCode, Json<Value>) {
    crate::core::session_access::map_session_access_error_compat(err)
}

pub(super) fn compat_internal_error(context: &str, detail: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": context,
            "detail": detail,
        })),
    )
}
