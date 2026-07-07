// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::config::api_url;
use crate::LocalRuntime;

use super::super::types::{LocalApiError, LocalTerminalExecRequest};

pub(crate) async fn local_terminal_exec(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<LocalTerminalExecRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let (cloud_base_url, access_token, device_id) = {
        let state = runtime.state.read().await;
        let auth = state
            .auth
            .as_ref()
            .ok_or_else(|| LocalApiError::bad_request("please login first"))?;
        let device_id = state
            .device_id
            .clone()
            .ok_or_else(|| LocalApiError::bad_request("device is not registered yet"))?;
        (
            auth.cloud_base_url.clone(),
            auth.access_token.clone(),
            device_id,
        )
    };
    let response = runtime
        .http_client
        .post(
            api_url(
                cloud_base_url.as_str(),
                format!(
                    "/api/local-connectors/relay/{}/terminal/exec",
                    urlencoding::encode(device_id.as_str())
                )
                .as_str(),
            )
            .as_str(),
        )
        .bearer_auth(access_token)
        .json(&json!({
            "workspace_id": req.workspace_id,
            "command": req.command,
            "args": req.args.unwrap_or_default(),
            "cwd": req.cwd,
            "timeout_ms": req.timeout_ms,
            "source": "local_connector_ui",
        }))
        .send()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    if !status.is_success() {
        return Err(LocalApiError::bad_gateway(body.to_string()));
    }
    Ok(Json(body))
}
