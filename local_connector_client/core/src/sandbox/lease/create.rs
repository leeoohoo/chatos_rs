// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::{
    EffectiveSandboxPolicy, NetworkPermissionPolicy, PermissionProfileId, SandboxBackendKind,
    SandboxBackendReadinessStatus,
};
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
use crate::sandbox::process::{native_process_sandbox_capability, start_native_sandbox_process};
use crate::sandbox::project_permissions::load_trusted_project_permission_document;
use crate::sandbox::types::{
    CreateLocalSandboxLeaseRequest, LocalSandboxLease, LocalSandboxNetworkPolicy,
    LocalSandboxRuntime,
};
use crate::sandbox::workspace::{
    local_sandbox_request_body, local_sandbox_run_workspace, prepare_local_sandbox_workspace,
};
use crate::workspace::paths::{resolve_request_workspace_dir, workspace_for_request};
use crate::{
    local_now_rfc3339, LocalState, DEFAULT_LOCAL_SANDBOX_IMAGE, LOCAL_SANDBOX_STATUS_READY,
};

const DEFAULT_LOCAL_SANDBOX_LEASE_TTL_SECONDS: u64 = 7_200;
const MIN_LOCAL_SANDBOX_LEASE_TTL_SECONDS: u64 = 60;
const MAX_LOCAL_SANDBOX_LEASE_TTL_SECONDS: u64 = 24 * 60 * 60;

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
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let project_cwd = resolve_request_workspace_dir(workspace, request, ".")?;
    let project_permissions =
        load_trusted_project_permission_document(workspace, project_cwd.as_path())?;
    let selection =
        local_effective_policy(&input.policy, &state.sandbox, project_permissions.as_ref())?;
    let effective_policy = selection.policy;
    let lease_id = format!("lease-{}", Uuid::new_v4());
    let sandbox_id = format!("sandbox-{}", Uuid::new_v4());
    let agent_token = format!("sat-{lease_id}");
    let run_workspace = local_sandbox_run_workspace(workspace, input.run_id.as_str())?;
    let response_seed = json!({ "run_workspace": run_workspace.to_string_lossy() });
    prepare_local_sandbox_workspace(request, state, &response_seed)?;
    let effective_permissions = state.sandbox.effective_permissions_with_project(
        Some(selection.profile_name.as_str()),
        &effective_policy,
        vec![run_workspace.to_string_lossy().to_string()],
        project_permissions.as_ref(),
    );
    let mut resource_limits = input.resource_limits.unwrap_or_default();
    resource_limits.cpu = resource_limits.cpu.max(0.1);
    resource_limits.memory_mb = resource_limits.memory_mb.max(128);
    resource_limits.disk_mb = resource_limits.disk_mb.max(128);
    resource_limits.max_processes = resource_limits.max_processes.max(16);
    let requested_network = input.network.unwrap_or_default();
    let (backend_id, agent_endpoint, image_id, image_ref, network) = match effective_policy
        .sandbox_mode
    {
        SandboxBackendKind::Docker => {
            ensure_docker_running().await?;
            let network = normalized_local_docker_network(
                &requested_network,
                &effective_permissions.network,
            )?;
            let image_ref =
                select_local_sandbox_image_ref(state, sandbox_runtime, input.image_id.as_deref())
                    .await;
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
            let Some(agent_endpoint) =
                published_local_sandbox_agent_endpoint(sandbox_id.as_str()).await
            else {
                let _ = destroy_local_sandbox_container(sandbox_id.as_str()).await;
                return Err(anyhow!("local sandbox agent port was not published"));
            };
            if let Err(err) =
                wait_for_local_sandbox_agent(http_client, agent_endpoint.as_str()).await
            {
                let _ = destroy_local_sandbox_container(sandbox_id.as_str()).await;
                return Err(err);
            }
            (
                backend_id,
                Some(agent_endpoint),
                input.image_id.clone(),
                Some(image_ref),
                network,
            )
        }
        SandboxBackendKind::LocalProcess => {
            let capability = native_process_sandbox_capability().await;
            if capability.status != SandboxBackendReadinessStatus::Ready {
                return Ok((
                    409,
                    BTreeMap::new(),
                    json!({
                        "error": capability.message,
                        "code": "sandbox_backend_not_ready",
                        "requested_backend": effective_policy.sandbox_mode,
                    }),
                ));
            }
            let network = normalized_native_process_network(
                &requested_network,
                effective_policy.permission_profile_id,
            )?;
            let backend_id = start_native_sandbox_process(
                sandbox_runtime,
                sandbox_id.as_str(),
                run_workspace.as_path(),
                &effective_policy,
                &effective_permissions,
                &resource_limits,
                input.project_id.as_str(),
                input.user_id.as_str(),
            )
            .await?;
            (backend_id, None, None, None, network)
        }
    };
    let now = local_now_rfc3339();
    let ttl_seconds = normalized_local_sandbox_lease_ttl_seconds(input.ttl_seconds);
    let expires_at = (Utc::now() + ChronoDuration::seconds(ttl_seconds as i64)).to_rfc3339();
    let lease = LocalSandboxLease {
        id: lease_id.clone(),
        sandbox_id: sandbox_id.clone(),
        tenant_id: input.tenant_id,
        user_id: input.user_id,
        project_id: input.project_id,
        run_id: input.run_id,
        workspace_root: input.workspace_root,
        run_workspace: run_workspace.to_string_lossy().to_string(),
        backend: effective_policy.sandbox_mode.as_str().to_string(),
        backend_id: Some(backend_id),
        image_id,
        image_ref,
        status: LOCAL_SANDBOX_STATUS_READY.to_string(),
        agent_endpoint,
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
        effective_permissions,
    };
    let response = local_sandbox_lease_response(&lease);
    sandbox_runtime
        .leases
        .write()
        .await
        .insert(sandbox_id, lease);
    Ok((201, BTreeMap::new(), response))
}

fn normalized_local_sandbox_lease_ttl_seconds(requested: Option<u64>) -> u64 {
    requested
        .unwrap_or(DEFAULT_LOCAL_SANDBOX_LEASE_TTL_SECONDS)
        .clamp(
            MIN_LOCAL_SANDBOX_LEASE_TTL_SECONDS,
            MAX_LOCAL_SANDBOX_LEASE_TTL_SECONDS,
        )
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
        "effective_permissions": lease.effective_permissions,
    })
}

#[derive(Debug, Clone)]
struct LocalEffectivePolicySelection {
    policy: EffectiveSandboxPolicy,
    profile_name: String,
}

fn local_effective_policy(
    request: &chatos_sandbox_contract::SandboxLeasePolicyRequest,
    sandbox: &crate::sandbox::types::LocalSandboxState,
    project: Option<&chatos_sandbox_contract::CodexPermissionProfileDocument>,
) -> Result<LocalEffectivePolicySelection> {
    let effective_configuration = sandbox
        .effective_permission_profile_configuration_with_project(project)
        .map_err(anyhow::Error::msg)?;
    let mut maximum = sandbox.effective_policy_defaults_with_project(project);
    let mut maximum_profile_name = effective_configuration.default_profile_name;
    let requested_backend = request.sandbox_mode.unwrap_or(maximum.sandbox_mode);
    if requested_backend == SandboxBackendKind::Docker {
        if !maximum_profile_name.starts_with(':') {
            return Err(anyhow!(
                "custom permission profile {maximum_profile_name:?} requires the native local-process sandbox and cannot be projected onto Docker without widening or dropping filesystem rules"
            ));
        }
        if !maximum
            .permission_profile_id
            .is_no_broader_than(PermissionProfileId::WorkspaceWrite)
        {
            maximum.permission_profile_id = PermissionProfileId::WorkspaceWrite;
            maximum_profile_name = PermissionProfileId::WorkspaceWrite.codex_name().to_string();
        }
    }
    let mut constrained_request = request.clone();
    if let Some(requested_profile) = request.permission_profile_id {
        if !effective_configuration
            .configuration
            .profile_allowed(requested_profile.codex_name())
        {
            if maximum
                .permission_profile_id
                .is_no_broader_than(requested_profile)
            {
                constrained_request.permission_profile_id = Some(maximum.permission_profile_id);
            } else {
                return Err(anyhow!(
                    "requested permission profile {} is disabled by allowed_permission_profiles and the configured fallback would be broader",
                    requested_profile.codex_name()
                ));
            }
        }
    }
    let policy = EffectiveSandboxPolicy::resolve_no_broader_than(&constrained_request, &maximum);
    let profile_name = match request.permission_profile_id {
        Some(requested)
            if policy.permission_profile_id == requested
                && policy.permission_profile_id != maximum.permission_profile_id =>
        {
            requested.codex_name().to_string()
        }
        Some(requested)
            if policy.permission_profile_id == requested
                && maximum_profile_name.starts_with(':') =>
        {
            requested.codex_name().to_string()
        }
        _ => maximum_profile_name,
    };
    Ok(LocalEffectivePolicySelection {
        policy,
        profile_name,
    })
}

fn normalized_local_docker_network(
    network: &LocalSandboxNetworkPolicy,
    permissions: &NetworkPermissionPolicy,
) -> Result<LocalSandboxNetworkPolicy> {
    let mode = network.mode.trim();
    if !mode.is_empty()
        && !mode.eq_ignore_ascii_case("bridge")
        && !mode.eq_ignore_ascii_case("none")
    {
        return Err(anyhow!(
            "local Docker sandbox only supports bridge or isolated networking; requested network mode {mode:?} is not allowed"
        ));
    }
    match permissions {
        NetworkPermissionPolicy::Unrestricted => Ok(LocalSandboxNetworkPolicy {
            mode: if mode.eq_ignore_ascii_case("none") {
                "none".to_string()
            } else {
                "bridge".to_string()
            },
        }),
        NetworkPermissionPolicy::Restricted { requirements }
            if requirements.enabled == Some(true) =>
        {
            Err(anyhow!(
                "restricted domain network profiles require the native local-process sandbox; Docker cannot enforce a domain allowlist"
            ))
        }
        NetworkPermissionPolicy::Restricted { .. } => Ok(LocalSandboxNetworkPolicy {
            mode: "none".to_string(),
        }),
    }
}

fn normalized_native_process_network(
    network: &LocalSandboxNetworkPolicy,
    permission_profile: PermissionProfileId,
) -> Result<LocalSandboxNetworkPolicy> {
    let mode = network.mode.trim();
    if permission_profile == PermissionProfileId::FullAccess {
        if mode.is_empty()
            || mode.eq_ignore_ascii_case("bridge")
            || mode.eq_ignore_ascii_case("host")
        {
            return Ok(LocalSandboxNetworkPolicy {
                mode: "host".to_string(),
            });
        }
        return Err(anyhow!(
            "full-access native process mode cannot claim network isolation for requested mode {mode:?}"
        ));
    }
    if mode.is_empty() || mode.eq_ignore_ascii_case("bridge") || mode.eq_ignore_ascii_case("none") {
        return Ok(LocalSandboxNetworkPolicy {
            mode: "none".to_string(),
        });
    }
    Err(anyhow!(
        "native process sandbox only supports disabled networking for read-only/workspace-write; requested mode {mode:?} is not allowed"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_sandbox_contract::{
        ApprovalPolicy, ApprovalReviewer, CustomPermissionProfile, PermissionProfileId,
        SandboxBackendKind,
    };

    #[test]
    fn local_sandbox_lease_ttl_is_bounded() {
        assert_eq!(
            normalized_local_sandbox_lease_ttl_seconds(None),
            DEFAULT_LOCAL_SANDBOX_LEASE_TTL_SECONDS
        );
        assert_eq!(
            normalized_local_sandbox_lease_ttl_seconds(Some(0)),
            MIN_LOCAL_SANDBOX_LEASE_TTL_SECONDS
        );
        assert_eq!(
            normalized_local_sandbox_lease_ttl_seconds(Some(u64::MAX)),
            MAX_LOCAL_SANDBOX_LEASE_TTL_SECONDS
        );
    }

    #[test]
    fn local_effective_policy_caps_cloud_request_to_local_defaults() {
        let state = crate::sandbox::types::LocalSandboxState {
            policy_revision: Some("local-revision".to_string()),
            ..Default::default()
        };
        let request = chatos_sandbox_contract::SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::Docker),
            permission_profile_id: Some(PermissionProfileId::FullAccess),
            approval_policy: Some(ApprovalPolicy::Never),
            approval_reviewer: Some(ApprovalReviewer::AutoReview),
            policy_revision: Some("cloud-revision".to_string()),
            additional_writable_roots: vec!["C:/outside".to_string()],
        };

        let effective = local_effective_policy(&request, &state, None)
            .expect("effective policy")
            .policy;

        assert_eq!(effective.sandbox_mode, SandboxBackendKind::Docker);
        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );
        assert_eq!(effective.approval_policy, ApprovalPolicy::Never);
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

        let effective = local_effective_policy(
            &request,
            &crate::sandbox::types::LocalSandboxState::default(),
            None,
        )
        .expect("effective policy")
        .policy;

        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::ReadOnly
        );
        assert_eq!(effective.approval_policy, ApprovalPolicy::OnRequest);
        assert_eq!(effective.approval_reviewer, ApprovalReviewer::User);
    }

    #[test]
    fn local_docker_effective_policy_does_not_claim_full_access() {
        let state = crate::sandbox::types::LocalSandboxState {
            default_permission_profile_id: PermissionProfileId::FullAccess,
            policy_revision: Some("local-revision".to_string()),
            ..Default::default()
        };
        let request = chatos_sandbox_contract::SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::Docker),
            permission_profile_id: Some(PermissionProfileId::FullAccess),
            approval_policy: None,
            approval_reviewer: None,
            policy_revision: None,
            additional_writable_roots: Vec::new(),
        };

        let effective = local_effective_policy(&request, &state, None)
            .expect("effective policy")
            .policy;

        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );
        assert_eq!(effective.policy_revision.as_deref(), Some("local-revision"));
    }

    #[test]
    fn local_docker_rejects_custom_profile_projection_fail_closed() {
        let state = crate::sandbox::types::LocalSandboxState {
            default_backend: SandboxBackendKind::LocalProcess,
            default_permission_profile_name: Some("project-edit".to_string()),
            permission_profiles: BTreeMap::from([(
                "project-edit".to_string(),
                CustomPermissionProfile {
                    extends: Some(":workspace".to_string()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let request = chatos_sandbox_contract::SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::Docker),
            ..Default::default()
        };

        let error = local_effective_policy(&request, &state, None)
            .expect_err("Docker must not drop custom filesystem rules");
        assert!(error
            .to_string()
            .contains("cannot be projected onto Docker"));
    }

    #[test]
    fn local_docker_network_policy_fails_closed_for_restricted_profiles() {
        let restricted = NetworkPermissionPolicy::Restricted {
            requirements: Default::default(),
        };
        let network = normalized_local_docker_network(
            &LocalSandboxNetworkPolicy {
                mode: "bridge".to_string(),
            },
            &restricted,
        )
        .expect("restricted Docker network");
        assert_eq!(network.mode, "none");

        let unrestricted = NetworkPermissionPolicy::Unrestricted;
        let network = normalized_local_docker_network(
            &LocalSandboxNetworkPolicy {
                mode: "bridge".to_string(),
            },
            &unrestricted,
        )
        .expect("unrestricted Docker network");
        assert_eq!(network.mode, "bridge");

        assert!(normalized_local_docker_network(
            &LocalSandboxNetworkPolicy {
                mode: "host".to_string(),
            },
            &unrestricted,
        )
        .is_err());
        assert!(normalized_local_docker_network(
            &LocalSandboxNetworkPolicy {
                mode: "container:other".to_string(),
            },
            &unrestricted,
        )
        .is_err());
    }

    #[test]
    fn native_process_network_is_disabled_for_restricted_profiles() {
        let network = normalized_native_process_network(
            &LocalSandboxNetworkPolicy {
                mode: "bridge".to_string(),
            },
            PermissionProfileId::WorkspaceWrite,
        )
        .expect("restricted network");
        assert_eq!(network.mode, "none");

        let network = normalized_native_process_network(
            &LocalSandboxNetworkPolicy {
                mode: "bridge".to_string(),
            },
            PermissionProfileId::FullAccess,
        )
        .expect("full access network");
        assert_eq!(network.mode, "host");
    }

    #[test]
    fn local_effective_policy_applies_managed_profile_allowlist_without_widening() {
        let mut state = crate::sandbox::types::LocalSandboxState {
            default_permission_profile_id: PermissionProfileId::WorkspaceWrite,
            allowed_permission_profiles: Some(BTreeMap::from([
                (":read-only".to_string(), true),
                (":workspace".to_string(), true),
                (":danger-full-access".to_string(), false),
            ])),
            ..Default::default()
        };
        let full_request = chatos_sandbox_contract::SandboxLeasePolicyRequest {
            permission_profile_id: Some(PermissionProfileId::FullAccess),
            ..Default::default()
        };
        let effective = local_effective_policy(&full_request, &state, None)
            .expect("safe managed fallback")
            .policy;
        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );

        state.allowed_permission_profiles =
            Some(BTreeMap::from([(":workspace".to_string(), true)]));
        let read_request = chatos_sandbox_contract::SandboxLeasePolicyRequest {
            permission_profile_id: Some(PermissionProfileId::ReadOnly),
            ..Default::default()
        };
        assert!(local_effective_policy(&read_request, &state, None).is_err());
    }

    #[test]
    fn trusted_project_default_is_applied_without_bypassing_local_maximum() {
        let state = crate::sandbox::types::LocalSandboxState {
            default_backend: SandboxBackendKind::LocalProcess,
            ..Default::default()
        };
        let project = chatos_sandbox_contract::parse_codex_permission_profile_toml(
            r#"
default_permissions = "project-review"

[permissions.project-review]
extends = ":read-only"
"#,
        )
        .expect("parse project permissions");

        let effective = local_effective_policy(
            &chatos_sandbox_contract::SandboxLeasePolicyRequest::default(),
            &state,
            Some(&project),
        )
        .expect("project effective policy");

        assert_eq!(effective.profile_name, "project-review");
        assert_eq!(
            effective.policy.permission_profile_id,
            PermissionProfileId::ReadOnly
        );
        assert!(effective
            .policy
            .policy_revision
            .as_deref()
            .is_some_and(|revision| revision.starts_with("runtime-")));
    }
}
