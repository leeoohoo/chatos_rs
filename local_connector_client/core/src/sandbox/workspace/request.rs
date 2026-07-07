// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};
use reqwest::Method;
use serde_json::Value;

use super::paths::local_sandbox_workspace_root;
use crate::relay::RelayRequest;
use crate::workspace::paths::workspace_for_request;
use crate::LocalState;

pub(crate) fn local_sandbox_request_body(
    request: &RelayRequest,
    state: &LocalState,
    method: &Method,
    path: &str,
) -> Result<Value> {
    if !is_sandbox_create_lease_request(method, path) {
        return Ok(request.body.clone());
    }
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let workspace_root = local_sandbox_workspace_root(workspace)?;
    let mut body = request.body.clone();
    let object = body
        .as_object_mut()
        .ok_or_else(|| anyhow!("sandbox create lease body must be a JSON object"))?;
    object.insert(
        "workspace_root".to_string(),
        Value::String(workspace_root.to_string_lossy().to_string()),
    );
    Ok(body)
}

fn is_sandbox_create_lease_request(method: &Method, path: &str) -> bool {
    *method == Method::POST && normalize_sandbox_http_path(path) == "/api/sandboxes/leases"
}

fn normalize_sandbox_http_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}
