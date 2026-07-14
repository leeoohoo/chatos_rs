// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::{EffectiveSandboxPolicy, PermissionProfileId, SandboxBackendKind};
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
    CreateLocalSandboxLeaseRequest, LocalSandboxLease, LocalSandboxNetworkPolicy,
    LocalSandboxRuntime,
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
    let body = local_sandbox_request_body(request, state, &Method::POST, "/api/sandboxes/leases")?;
    let input = serde_json::from_value::<CreateLocalSandboxLeaseRequest>(body)
        .context("parse local sandbox lease request")?;
    let effective_policy =
        local_docker_effective_policy(&input.policy, &state.sandbox.effective_policy_defaults());
    if let Some(response) = unsupported_backend_response(&effective_policy) {
        return Ok(response);
    }
    ensure_docker_running().await?;
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let lease_id = format!("lease-{}", Uuid::new_v4());
    let sandbox_id = format!("sandbox-{}", Uuid::new_v4());
    let agent_token = format!("sat-{lease_id}");
    let run_workspace = local_sandbox_run_workspace(workspace, input.run_id.as_str())?;
    let response_seed = json!({ "run_workspace": run_workspace.to_string_lossy() });
    prepare_local_sandbox_workspace(request, state, &response_seed)?;
    let resource_limits = input.resource_limits.unwrap_or_default();
    let network = input.network.unwrap_or_default();
    validate_local_sandbox_network_policy(&network)?;
    let image_ref =
        select_local_sandbox_image_ref(state, sandbox_runtime, input.image_id.as_deref()).await;
    let backend_id = start_local_sandbox_container(
        sandbox_id.as_str(),
        run_workspace.as_path(),
        image_ref.as_str(),
        agent_token.as_str(),
        &resource_limits,
        &network,
        effective_policy.permission_profile_id,
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
        effective_policy,
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
        "effective_policy": lease.effective_policy,
    })
}

fn unsupported_backend_response(
    effective_policy: &EffectiveSandboxPolicy,
) -> Option<(u16, BTreeMap<String, String>, Value)> {
    (effective_policy.sandbox_mode != SandboxBackendKind::Docker).then(|| {
        (
            409,
            BTreeMap::new(),
            json!({
                "error": "local process sandbox is not ready on this device",
                "code": "sandbox_backend_not_ready",
                "requested_backend": effective_policy.sandbox_mode,
            }),
        )
    })
}

fn local_docker_effective_policy(
    request: &chatos_sandbox_contract::SandboxLeasePolicyRequest,
    local_maximum: &EffectiveSandboxPolicy,
) -> EffectiveSandboxPolicy {
    let mut docker_maximum = local_maximum.clone();
    if !docker_maximum
        .permission_profile_id
        .is_no_broader_than(PermissionProfileId::WorkspaceWrite)
    {
        docker_maximum.permission_profile_id = PermissionProfileId::WorkspaceWrite;
    }
    EffectiveSandboxPolicy::resolve_no_broader_than(request, &docker_maximum)
}

fn validate_local_sandbox_network_policy(network: &LocalSandboxNetworkPolicy) -> Result<()> {
    let mode = network.mode.trim();
    if mode.is_empty() || mode.eq_ignore_ascii_case("bridge") {
        return Ok(());
    }
    Err(anyhow!(
        "local Docker sandbox currently only supports bridge networking for the MCP agent; requested network mode {mode:?} is not allowed"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_sandbox_contract::{
        ApprovalPolicy, ApprovalReviewer, PermissionProfileId, SandboxBackendKind,
    };

    #[test]
    fn unsupported_backend_response_rejects_local_process_before_docker_startup() {
        let policy = EffectiveSandboxPolicy {
            sandbox_mode: SandboxBackendKind::LocalProcess,
            permission_profile_id: PermissionProfileId::WorkspaceWrite,
            approval_policy: ApprovalPolicy::OnRequest,
            approval_reviewer: ApprovalReviewer::User,
            policy_revision: None,
            additional_writable_roots: Vec::new(),
        };

        let (status, _, body) =
            unsupported_backend_response(&policy).expect("local process should be rejected");

        assert_eq!(status, 409);
        assert_eq!(
            body.get("code").and_then(Value::as_str),
            Some("sandbox_backend_not_ready")
        );
        assert_eq!(
            body.get("requested_backend").and_then(Value::as_str),
            Some("local_process")
        );
    }

    #[test]
    fn unsupported_backend_response_allows_docker() {
        assert!(unsupported_backend_response(&EffectiveSandboxPolicy::default()).is_none());
    }

    #[test]
    fn local_effective_policy_caps_cloud_request_to_local_defaults() {
        let mut state = crate::sandbox::types::LocalSandboxState::default();
        state.policy_revision = Some("local-revision".to_string());
        let request = chatos_sandbox_contract::SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::Docker),
            permission_profile_id: Some(PermissionProfileId::FullAccess),
            approval_policy: Some(ApprovalPolicy::Never),
            approval_reviewer: Some(ApprovalReviewer::AutoReview),
            policy_revision: Some("cloud-revision".to_string()),
            additional_writable_roots: vec!["C:/outside".to_string()],
        };

        let effective = local_docker_effective_policy(&request, &state.effective_policy_defaults());

        assert_eq!(effective.sandbox_mode, SandboxBackendKind::Docker);
        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );
        assert_eq!(effective.approval_policy, ApprovalPolicy::OnRequest);
        assert_eq!(effective.approval_reviewer, ApprovalReviewer::User);
        assert_eq!(effective.policy_revision.as_deref(), Some("local-revision"));
        assert!(effective.additional_writable_roots.is_empty());
    }

    #[test]
    fn local_effective_policy_allows_task_to_request_narrower_limits() {
        let request = chatos_sandbox_contract::SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::Docker),
            permission_profile_id: Some(PermissionProfileId::ReadOnly),
            approval_policy: Some(ApprovalPolicy::OnRequest),
            approval_reviewer: Some(ApprovalReviewer::User),
            policy_revision: None,
            additional_writable_roots: Vec::new(),
        };

        let effective = local_docker_effective_policy(
            &request,
            &crate::sandbox::types::LocalSandboxState::default().effective_policy_defaults(),
        );

        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::ReadOnly
        );
        assert_eq!(effective.approval_policy, ApprovalPolicy::OnRequest);
        assert_eq!(effective.approval_reviewer, ApprovalReviewer::User);
    }

    #[test]
    fn local_docker_effective_policy_does_not_claim_full_access() {
        let mut state = crate::sandbox::types::LocalSandboxState {
            default_permission_profile_id: PermissionProfileId::FullAccess,
            ..Default::default()
        };
        state.policy_revision = Some("local-revision".to_string());
        let request = chatos_sandbox_contract::SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::Docker),
            permission_profile_id: Some(PermissionProfileId::FullAccess),
            approval_policy: None,
            approval_reviewer: None,
            policy_revision: None,
            additional_writable_roots: Vec::new(),
        };

        let effective = local_docker_effective_policy(&request, &state.effective_policy_defaults());

        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );
        assert_eq!(effective.policy_revision.as_deref(), Some("local-revision"));
    }

    #[test]
    fn local_sandbox_network_policy_rejects_dangerous_or_unsupported_modes() {
        assert!(
            validate_local_sandbox_network_policy(&LocalSandboxNetworkPolicy {
                mode: String::new(),
            })
            .is_ok()
        );
        assert!(
            validate_local_sandbox_network_policy(&LocalSandboxNetworkPolicy {
                mode: "bridge".to_string(),
            })
            .is_ok()
        );
        assert!(
            validate_local_sandbox_network_policy(&LocalSandboxNetworkPolicy {
                mode: "host".to_string(),
            })
            .is_err()
        );
        assert!(
            validate_local_sandbox_network_policy(&LocalSandboxNetworkPolicy {
                mode: "container:other".to_string(),
            })
            .is_err()
        );
        assert!(
            validate_local_sandbox_network_policy(&LocalSandboxNetworkPolicy {
                mode: "none".to_string(),
            })
            .is_err()
        );
    }
}
