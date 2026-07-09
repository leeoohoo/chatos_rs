// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::approval::{
    approve_pending_approval, deny_pending_approval, list_in_progress_approvals,
    list_pending_approvals,
};
use crate::LocalRuntime;

use super::super::types::{LocalApiError, ResolveApprovalRequest, UpdateApprovalSettingsRequest};

pub(crate) async fn local_approval_settings(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    let state = runtime.state.read().await;
    Ok(Json(approval_settings_payload(&state.approval)))
}

pub(crate) async fn local_update_approval_settings(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<UpdateApprovalSettingsRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let mut state = runtime.state.write().await;
    if let Some(default_mode) = req.default_mode {
        state.approval.default_mode = default_mode;
    }
    if let Some(projects) = req.projects {
        state.approval.projects = projects;
    }
    if let Some(mut ai) = req.ai {
        if ai.api_key.is_none() {
            ai.api_key = state.approval.ai.api_key.clone();
        }
        state.approval.ai = ai;
    }
    if let Some(memory) = req.memory {
        state.approval.memory = memory;
    }
    state.save(runtime.state_path.as_path())?;
    Ok(Json(approval_settings_payload(&state.approval)))
}

pub(crate) async fn local_pending_approvals() -> Result<Json<Value>, LocalApiError> {
    Ok(Json(json!({
        "items": list_pending_approvals().await,
        "reviewing": list_in_progress_approvals().await,
    })))
}

pub(crate) async fn local_approve_pending_approval(
    Path(id): Path<String>,
    Json(req): Json<ResolveApprovalRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let ok = approve_pending_approval(id.as_str(), req.remember_allow.unwrap_or(false)).await;
    if !ok {
        return Err(LocalApiError::bad_request("pending approval not found"));
    }
    Ok(Json(json!({ "ok": true })))
}

fn approval_settings_payload(state: &crate::approval::ApprovalState) -> Value {
    let mut value = json!(state);
    if let Some(ai) = value.get_mut("ai").and_then(Value::as_object_mut) {
        let has_api_key = state
            .ai
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        ai.remove("api_key");
        ai.insert("has_api_key".to_string(), Value::Bool(has_api_key));
    }
    value
}

pub(crate) async fn local_deny_pending_approval(
    Path(id): Path<String>,
    Json(req): Json<ResolveApprovalRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let ok = deny_pending_approval(id.as_str(), req.reason).await;
    if !ok {
        return Err(LocalApiError::bad_request("pending approval not found"));
    }
    Ok(Json(json!({ "ok": true })))
}
