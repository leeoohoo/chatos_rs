// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use reqwest::Method;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::optional_env;
use crate::relay::RelayRequest;
use crate::sandbox::docker::{
    destroy_local_sandbox_container, ensure_docker_running, published_local_sandbox_agent_endpoint,
    start_local_sandbox_container, wait_for_local_sandbox_agent,
};
use crate::sandbox::images::local_sandbox_image_ref_for_id;
use crate::sandbox::types::{
    CreateLocalSandboxLeaseRequest, LocalSandboxLease, LocalSandboxRuntime,
};
use crate::sandbox::workspace::{
    local_sandbox_request_body, local_sandbox_run_workspace, prepare_local_sandbox_workspace,
};
use crate::workspace::paths::workspace_for_request;
use crate::{
    local_now_rfc3339, LocalState, DEFAULT_LOCAL_SANDBOX_IMAGE, LOCAL_SANDBOX_BACKEND,
    LOCAL_SANDBOX_STATUS_READY,
};

pub(crate) async fn create_local_sandbox_lease(
    request: &RelayRequest,
    state: &LocalState,
    http_client: &reqwest::Client,
    sandbox_runtime: &LocalSandboxRuntime,
) -> Result<(u16, BTreeMap<String, String>, Value)> {
    if !state.sandbox.enabled {
        return Ok((
            400,
            BTreeMap::new(),
            json!({ "error": "local sandbox is disabled" }),
        ));
    }
    ensure_docker_running().await?;
    let body = local_sandbox_request_body(request, state, &Method::POST, "/api/sandboxes/leases")?;
    let input = serde_json::from_value::<CreateLocalSandboxLeaseRequest>(body)
        .context("parse local sandbox lease request")?;
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let lease_id = format!("lease-{}", Uuid::new_v4());
    let sandbox_id = format!("sandbox-{}", Uuid::new_v4());
    let agent_token = format!("sat-{lease_id}");
    let run_workspace = local_sandbox_run_workspace(workspace, input.run_id.as_str())?;
    let response_seed = json!({ "run_workspace": run_workspace.to_string_lossy() });
    prepare_local_sandbox_workspace(request, state, &response_seed)?;
    let resource_limits = input.resource_limits.unwrap_or_default();
    let network = input.network.unwrap_or_default();
    let image_ref =
        select_local_sandbox_image_ref(state, sandbox_runtime, input.image_id.as_deref()).await;
    let backend_id = start_local_sandbox_container(
        sandbox_id.as_str(),
        run_workspace.as_path(),
        image_ref.as_str(),
        agent_token.as_str(),
        &resource_limits,
        &network,
    )
    .await?;
    let Some(agent_endpoint) = published_local_sandbox_agent_endpoint(sandbox_id.as_str()).await
    else {
        let _ = destroy_local_sandbox_container(sandbox_id.as_str()).await;
        return Err(anyhow!("local sandbox agent port was not published"));
    };
    if let Err(err) = wait_for_local_sandbox_agent(http_client, agent_endpoint.as_str()).await {
        let _ = destroy_local_sandbox_container(sandbox_id.as_str()).await;
        return Err(err);
    }
    let now = local_now_rfc3339();
    let expires_at = (Utc::now()
        + ChronoDuration::seconds(input.ttl_seconds.unwrap_or(7200) as i64))
    .to_rfc3339();
    let lease = LocalSandboxLease {
        id: lease_id.clone(),
        sandbox_id: sandbox_id.clone(),
        tenant_id: input.tenant_id,
        user_id: input.user_id,
        project_id: input.project_id,
        run_id: input.run_id,
        workspace_root: input.workspace_root,
        run_workspace: run_workspace.to_string_lossy().to_string(),
        backend: LOCAL_SANDBOX_BACKEND.to_string(),
        backend_id: Some(backend_id),
        image_id: input.image_id,
        image_ref: Some(image_ref),
        status: LOCAL_SANDBOX_STATUS_READY.to_string(),
        agent_endpoint: Some(agent_endpoint),
        agent_token: agent_token.clone(),
        resource_limits,
        network,
        tools: if input.tools.is_empty() {
            vec!["filesystem".to_string(), "terminal".to_string()]
        } else {
            input.tools
        },
        created_at: now.clone(),
        updated_at: now,
        expires_at,
        destroyed_at: None,
        last_error: None,
    };
    let response = local_sandbox_lease_response(&lease);
    sandbox_runtime
        .leases
        .write()
        .await
        .insert(sandbox_id, lease);
    Ok((201, BTreeMap::new(), response))
}

async fn select_local_sandbox_image_ref(
    state: &LocalState,
    sandbox_runtime: &LocalSandboxRuntime,
    image_id: Option<&str>,
) -> String {
    if let Some(image_id) = image_id.filter(|value| *value != "default") {
        if let Some(job) = sandbox_runtime
            .jobs
            .read()
            .await
            .iter()
            .find(|job| job.image_id == image_id && job.status == "succeeded")
        {
            return job.image_ref.clone();
        }
        if let Some(image_ref) = local_sandbox_image_ref_for_id(state, image_id) {
            return image_ref;
        }
    }
    state
        .sandbox
        .selected_image_ref
        .clone()
        .or_else(|| optional_env("LOCAL_CONNECTOR_SANDBOX_DOCKER_IMAGE"))
        .unwrap_or_else(|| DEFAULT_LOCAL_SANDBOX_IMAGE.to_string())
}

fn local_sandbox_lease_response(lease: &LocalSandboxLease) -> Value {
    json!({
        "lease_id": lease.id,
        "sandbox_id": lease.sandbox_id,
        "backend_id": lease.backend_id,
        "image_id": lease.image_id,
        "image_ref": lease.image_ref,
        "status": lease.status,
        "agent_endpoint": lease.agent_endpoint,
        "agent_token": lease.agent_token,
        "run_workspace": lease.run_workspace,
        "expires_at": lease.expires_at,
    })
}
