// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use chatos_sandbox_image_mcp::SandboxImageBackend;
use serde_json::{json, Value};

use crate::config::{api_url, normalize_optional};
use crate::registration::ensure_success;
use crate::sandbox::docker::{docker_status, ensure_docker_running};
use crate::sandbox::images::{local_sandbox_image_catalog, start_local_sandbox_image_job};
use crate::LocalRuntime;

use super::super::types::{InitializeImageRequest, LocalApiError, ToggleSandboxRequest};
use super::status::status_payload;

pub(crate) async fn local_docker_status() -> Json<Value> {
    Json(docker_status().await)
}

pub(crate) async fn local_toggle_sandbox(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<ToggleSandboxRequest>,
) -> Result<Json<Value>, LocalApiError> {
    if req.enabled {
        ensure_docker_running()
            .await
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    }
    {
        let mut state = runtime.state.write().await;
        state.sandbox.enabled = req.enabled;
        state.save(runtime.state_path.as_path())?;
    }
    upsert_sandbox_pairings(&runtime, req.enabled).await?;
    runtime.start_connector_if_configured().await?;
    Ok(Json(status_payload(&runtime).await))
}

pub(crate) async fn local_sandbox_images(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    Ok(Json(local_sandbox_image_catalog(&runtime).await))
}

pub(crate) async fn local_sandbox_image_jobs(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    let jobs = runtime.sandbox_runtime.jobs.read().await.clone();
    Ok(Json(json!(jobs)))
}

pub(crate) async fn local_sandbox_leases(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    let leases = runtime
        .sandbox_runtime
        .leases
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    Ok(Json(json!(leases)))
}

pub(crate) async fn local_initialize_sandbox_image(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<InitializeImageRequest>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    ensure_docker_running()
        .await
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let job = start_local_sandbox_image_job(
        &runtime,
        req.features,
        normalize_optional(req.custom_build_script.as_deref()),
        None,
        None,
    )
    .await
    .map_err(LocalApiError::bad_request)?;
    Ok(Json(json!(job)))
}

pub(crate) async fn local_sandbox_image_mcp(
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    let backend = LocalSandboxImageMcpBackend { runtime };
    Json(chatos_sandbox_image_mcp::handle_jsonrpc(&backend, payload).await)
}

struct LocalSandboxImageMcpBackend {
    runtime: LocalRuntime,
}

#[async_trait::async_trait]
impl SandboxImageBackend for LocalSandboxImageMcpBackend {
    async fn image_catalog(&self) -> Result<Value, String> {
        ensure_local_sandbox_enabled(&self.runtime)
            .await
            .map_err(|err| err.message().to_string())?;
        Ok(local_sandbox_image_catalog(&self.runtime).await)
    }

    async fn image_jobs(&self) -> Result<Value, String> {
        ensure_local_sandbox_enabled(&self.runtime)
            .await
            .map_err(|err| err.message().to_string())?;
        let jobs = self.runtime.sandbox_runtime.jobs.read().await.clone();
        Ok(json!(jobs))
    }

    async fn initialize_image(
        &self,
        features: Vec<String>,
        custom_build_script: Option<String>,
    ) -> Result<Value, String> {
        ensure_local_sandbox_enabled(&self.runtime)
            .await
            .map_err(|err| err.message().to_string())?;
        ensure_docker_running()
            .await
            .map_err(|err| err.to_string())?;
        let job =
            start_local_sandbox_image_job(&self.runtime, features, custom_build_script, None, None)
                .await
                .map_err(|err| err.to_string())?;
        Ok(json!(job))
    }
}

async fn ensure_local_sandbox_enabled(runtime: &LocalRuntime) -> Result<(), LocalApiError> {
    let state = runtime.state.read().await;
    if state.sandbox.enabled {
        Ok(())
    } else {
        Err(LocalApiError::bad_request("local sandbox is disabled"))
    }
}

async fn upsert_sandbox_pairings(
    runtime: &LocalRuntime,
    enabled: bool,
) -> Result<(), LocalApiError> {
    let (cloud_base_url, access_token, device_id, workspaces) = {
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
            state.workspaces.clone(),
        )
    };
    for workspace in workspaces {
        let response = runtime
            .http_client
            .post(
                api_url(
                    cloud_base_url.as_str(),
                    "/api/local-connectors/sandbox-pairings",
                )
                .as_str(),
            )
            .bearer_auth(access_token.as_str())
            .json(&json!({
                "device_id": device_id.as_str(),
                "workspace_id": workspace.id,
                "enabled": enabled,
                "sandbox_mode": "docker",
            }))
            .send()
            .await
            .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
        ensure_success(response.status(), "upsert sandbox pairing")
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    }
    Ok(())
}
