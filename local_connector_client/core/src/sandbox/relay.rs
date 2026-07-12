// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use chatos_sandbox_image_mcp::{
    SandboxImageBackend, SANDBOX_IMAGE_PROJECT_ID_HEADER, SANDBOX_IMAGE_RUN_ID_HEADER,
};
use reqwest::Method;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::history::CommandHistoryRecorder;
use crate::relay::{relay_error_response, RelayRequest, RelayResponse};
use crate::sandbox::docker::ensure_docker_running;
use crate::sandbox::images::{local_sandbox_image_catalog, start_local_sandbox_image_job};
use crate::sandbox::lease::{
    create_local_sandbox_lease, get_local_sandbox, health_local_sandbox, release_local_sandbox,
};
use crate::sandbox::proxy::proxy_local_sandbox_mcp;
use crate::sandbox::types::LocalSandboxRuntime;
use crate::{LocalRuntime, LocalState};

pub(crate) async fn handle_sandbox_request(
    value: Value,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    history_recorder: &CommandHistoryRecorder,
) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return relay_error_response("sandbox_response", "", 400, err.to_string());
        }
    };
    match handle_local_sandbox_request(
        &request,
        state,
        http_client,
        sandbox_runtime,
        history_recorder,
    )
    .await
    {
        Ok((status, headers, body)) => RelayResponse {
            message_type: "sandbox_response".to_string(),
            request_id: request.request_id,
            status,
            headers,
            body,
        }
        .into_value(),
        Err(err) => RelayResponse {
            message_type: "sandbox_response".to_string(),
            request_id: request.request_id,
            status: 502,
            headers: BTreeMap::new(),
            body: json!({ "error": err.to_string() }),
        }
        .into_value(),
    }
}

async fn handle_local_sandbox_request(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    history_recorder: &CommandHistoryRecorder,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    let method = request
        .method
        .as_deref()
        .unwrap_or("POST")
        .parse::<Method>()
        .context("parse sandbox request method")?;
    let path = normalize_sandbox_http_path(request.path.as_deref().unwrap_or("/"));
    if method == Method::POST && path == "/api/local/sandbox/images/mcp" {
        let runtime = relay_local_runtime(http_client, sandbox_runtime, history_recorder);
        let backend = LocalSandboxImageRelayBackend {
            runtime,
            project_id: relay_header(request, SANDBOX_IMAGE_PROJECT_ID_HEADER),
            run_id: relay_header(request, SANDBOX_IMAGE_RUN_ID_HEADER),
        };
        let body = chatos_sandbox_image_mcp::handle_jsonrpc(&backend, request.body.clone()).await;
        return Ok((200, BTreeMap::new(), body));
    }
    if method == Method::GET && path == "/api/local/sandbox/images/jobs" {
        if !state.sandbox.enabled {
            return Ok((
                400,
                BTreeMap::new(),
                json!({ "error": "local sandbox is disabled" }),
            ));
        }
        let jobs = sandbox_runtime.jobs.read().await.clone();
        return Ok((200, BTreeMap::new(), json!(jobs)));
    }
    if method == Method::POST && path == "/api/sandboxes/leases" {
        return create_local_sandbox_lease(request, state, http_client, sandbox_runtime).await;
    }
    if method == Method::GET && path == "/api/sandboxes" {
        let leases = sandbox_runtime
            .leases
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        return Ok((200, BTreeMap::new(), json!(leases)));
    }
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() >= 3 && parts[0] == "api" && parts[1] == "sandboxes" {
        let sandbox_id = parts[2];
        if method == Method::GET && parts.len() == 3 {
            return get_local_sandbox(sandbox_runtime, sandbox_id).await;
        }
        if method == Method::GET && parts.len() == 4 && parts[3] == "health" {
            return health_local_sandbox(http_client, sandbox_runtime, sandbox_id).await;
        }
        if method == Method::POST && parts.len() == 4 && parts[3] == "release" {
            return release_local_sandbox(request, sandbox_runtime, sandbox_id).await;
        }
        if method == Method::POST && parts.len() == 4 && parts[3] == "mcp" {
            return proxy_local_sandbox_mcp(
                request,
                state,
                http_client,
                sandbox_runtime,
                sandbox_id,
                history_recorder,
            )
            .await;
        }
    }
    Ok((
        404,
        BTreeMap::new(),
        json!({ "error": format!("unsupported local sandbox path: {path}") }),
    ))
}

fn relay_local_runtime(
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
    history_recorder: &CommandHistoryRecorder,
) -> LocalRuntime {
    LocalRuntime {
        state_path: history_recorder.state_path.clone(),
        state: history_recorder.state.clone(),
        http_client: http_client.clone(),
        connector_task: Arc::new(Mutex::new(None)),
        sandbox_runtime: sandbox_runtime.clone(),
    }
}

struct LocalSandboxImageRelayBackend {
    runtime: LocalRuntime,
    project_id: Option<String>,
    run_id: Option<String>,
}

#[async_trait::async_trait]
impl SandboxImageBackend for LocalSandboxImageRelayBackend {
    async fn image_catalog(&self) -> Result<Value, String> {
        ensure_relay_local_sandbox_enabled(&self.runtime).await?;
        Ok(local_sandbox_image_catalog(&self.runtime).await)
    }

    async fn image_jobs(&self) -> Result<Value, String> {
        ensure_relay_local_sandbox_enabled(&self.runtime).await?;
        let jobs = self.runtime.sandbox_runtime.jobs.read().await.clone();
        Ok(json!(jobs))
    }

    async fn initialize_image(
        &self,
        features: Vec<String>,
        custom_build_script: Option<String>,
    ) -> Result<Value, String> {
        ensure_relay_local_sandbox_enabled(&self.runtime).await?;
        ensure_docker_running()
            .await
            .map_err(|err| err.to_string())?;
        let job = start_local_sandbox_image_job(
            &self.runtime,
            features,
            custom_build_script,
            self.project_id.clone(),
            self.run_id.clone(),
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(json!(job))
    }
}

fn relay_header(request: &RelayRequest, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn ensure_relay_local_sandbox_enabled(runtime: &LocalRuntime) -> Result<(), String> {
    let state = runtime.state.read().await;
    if state.sandbox.enabled {
        Ok(())
    } else {
        Err("local sandbox is disabled".to_string())
    }
}

fn normalize_sandbox_http_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}
