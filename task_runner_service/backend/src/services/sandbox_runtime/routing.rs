// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::StatusCode;
use serde::Deserialize;

use super::*;
use crate::models::{TaskProjectRecord, PUBLIC_PROJECT_ID};
use crate::services::project_management_api_client::{
    self, ProjectRuntimeEnvironmentImage, ProjectSandboxRuntimeSettings,
};
use crate::trace_context::InternalTraceContextExt;
use chatos_project_execution::{parse_local_connector_workspace_root, LocalConnectorWorkspaceRef};
use chatos_sandbox_contract::{
    ApprovalPolicy, ApprovalReviewer, EffectiveSandboxPolicy, PermissionProfileId,
    SandboxBackendKind, SandboxLeasePolicyRequest,
};

#[derive(Debug, Deserialize)]
struct LocalConnectorSandboxPairing {
    id: String,
    device_id: String,
    workspace_id: String,
    enabled: bool,
    #[serde(default)]
    sandbox_mode: String,
    #[serde(default)]
    sandbox_readiness: String,
    #[serde(default)]
    permission_profile_id: String,
    #[serde(default)]
    approval_policy: String,
    #[serde(default)]
    approval_reviewer: String,
    #[serde(default)]
    policy_revision: Option<String>,
}

#[derive(Debug)]
struct LocalConnectorResolvedSandboxRoute {
    base_url: String,
    pairing_id: String,
    policy: SandboxLeasePolicyRequest,
}

impl RunService {
    pub(crate) async fn validate_sandbox_route_for_task(
        &self,
        task: &TaskRecord,
    ) -> Result<(), String> {
        self.sandbox_route_for_task(task).await.map(|_| ())
    }

    pub(super) async fn sandbox_route_for_task(
        &self,
        task: &TaskRecord,
    ) -> Result<SandboxTaskRoute, String> {
        if task
            .mcp_config
            .sandbox_manager_base_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        {
            return Err(
                "sandbox_manager_base_url is program-managed and cannot be overridden per task"
                    .to_string(),
            );
        }

        let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
        if project_id == PUBLIC_PROJECT_ID
            || !project_management_api_client::project_service_enabled(&self.config)
        {
            let base_url = self.effective_sandbox_manager_base_url().await?;
            return Ok(SandboxTaskRoute {
                auth: sandbox_auth_for_task(&self.config, task, base_url.as_str())?,
                base_url,
                image_id: Some(base_sandbox_image_id_for_task(task)?),
                environment_plan: None,
                provider: "cloud_sandbox_manager".to_string(),
                local_connector_pairing_id: None,
                policy: task.mcp_config.sandbox_policy_request(),
            });
        }

        let project =
            project_management_api_client::sync_get_project(&self.config, project_id.as_str())
                .await?
                .ok_or_else(|| {
                    format!("project not found while resolving sandbox route: {project_id}")
                })?;
        let runtime = project_management_api_client::get_project_sandbox_runtime_settings(
            &self.config,
            project_id.as_str(),
        )
        .await?;
        let task_policy = task.mcp_config.sandbox_policy_request();
        let local_project = local_connector_project_ref(&project)?;
        let (base_url, provider, local_connector_pairing_id, policy) = if let Some(project_ref) =
            local_project.as_ref()
        {
            let resolved =
                resolve_local_connector_sandbox_route(&self.config, task, &project, project_ref)
                    .await?;
            (
                resolved.base_url,
                "local_connector".to_string(),
                Some(resolved.pairing_id),
                resolved.policy,
            )
        } else {
            (
                self.effective_sandbox_manager_base_url().await?,
                "cloud_sandbox_manager".to_string(),
                None,
                task_policy,
            )
        };
        let environment_plan = if local_project.is_none() && runtime_topology_v2_enabled() {
            sandbox_environment_plan_for_task(task, &runtime, provider.as_str())?
        } else {
            None
        };
        let image_id = if local_project.is_some() {
            local_sandbox_image_id_for_task(task, &runtime, policy.sandbox_mode)?
        } else {
            sandbox_image_id_for_task(
                task,
                &runtime,
                provider.as_str(),
                crate::config::configured_sandbox_base_image_id().as_str(),
            )?
        };
        let auth = sandbox_auth_for_task(&self.config, task, base_url.as_str())?;
        Ok(SandboxTaskRoute {
            base_url,
            auth,
            image_id,
            environment_plan,
            provider,
            local_connector_pairing_id,
            policy,
        })
    }
}

fn runtime_topology_v2_enabled() -> bool {
    runtime_topology_v2_enabled_from_value(
        std::env::var("TASK_RUNNER_RUNTIME_TOPOLOGY_V2")
            .ok()
            .as_deref(),
    )
}

fn runtime_topology_v2_enabled_from_value(value: Option<&str>) -> bool {
    !matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("0" | "false" | "off" | "no")
    )
}

#[cfg(test)]
pub(super) fn sandbox_environment_fallback_allowed(
    _task: &TaskRecord,
    _route: &SandboxTaskRoute,
) -> bool {
    false
}

fn sandbox_environment_plan_for_task(
    task: &TaskRecord,
    runtime: &ProjectSandboxRuntimeSettings,
    provider: &str,
) -> Result<Option<SandboxEnvironmentPlan>, String> {
    if !task.mcp_config.requires_execution {
        return Ok(None);
    }
    ensure_project_runtime_ready(runtime)?;
    let global_environment = json_object_to_string_map(&runtime.environment.env_vars);
    let mut services = Vec::new();
    let mut workspace_service_ids = Vec::new();
    for image in runtime
        .images
        .iter()
        .filter(|image| image_status_is_available(image.status.as_str()))
    {
        let service_id = image.service_id.trim();
        if service_id.is_empty() {
            continue;
        }
        if runtime_image_is_program_managed_target(image) {
            if !image.image_provider.trim().is_empty()
                && !image.image_provider.eq_ignore_ascii_case(provider)
            {
                continue;
            }
            let image_id = image
                .image_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("workspace service {service_id} has no ready image_id"))?;
            workspace_service_ids.push(service_id.to_string());
            services.push(SandboxEnvironmentServicePlan {
                service_id: service_id.to_string(),
                environment_key: image.environment_key.clone(),
                display_name: if image.display_name.trim().is_empty() {
                    service_id.to_string()
                } else {
                    image.display_name.clone()
                },
                service_role: "workspace".to_string(),
                image_id: Some(image_id.to_string()),
                image_ref: image.image_ref.clone(),
                dockerfile: None,
                environment: merged_environment(&global_environment, &image.env_vars),
                mcp_policy: SandboxEnvironmentMcpPolicyPlan {
                    managed_by: "system".to_string(),
                    attachment: "workspace_gateway_target".to_string(),
                    filesystem: true,
                    terminal: true,
                },
            });
        } else if runtime_image_is_program_managed_dependency(image) {
            let image_ref = image
                .image_ref
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("dependency service {service_id} has no image_ref"))?;
            services.push(SandboxEnvironmentServicePlan {
                service_id: service_id.to_string(),
                environment_key: image.environment_key.clone(),
                display_name: if image.display_name.trim().is_empty() {
                    service_id.to_string()
                } else {
                    image.display_name.clone()
                },
                service_role: "dependency".to_string(),
                image_id: None,
                image_ref: Some(image_ref.to_string()),
                dockerfile: None,
                environment: merged_environment(&global_environment, &image.env_vars),
                mcp_policy: SandboxEnvironmentMcpPolicyPlan::default(),
            });
        }
    }
    if workspace_service_ids.is_empty() {
        if let Some(requested) = normalized_execution_service_id(task) {
            return Err(format!(
                "execution_service_id is not the ready project workspace: {requested}"
            ));
        }
        return Err("project runtime environment has no ready workspace image".to_string());
    }
    if workspace_service_ids.len() != 1 {
        return Err("project runtime must contain exactly one ready workspace service".to_string());
    }
    let execution_service_id = workspace_service_ids[0].clone();
    if let Some(requested) = normalized_execution_service_id(task) {
        if requested != execution_service_id {
            return Err(format!(
                "execution_service_id is fixed to the project workspace: {execution_service_id}"
            ));
        }
    }
    if runtime
        .environment
        .execution_service_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|configured| configured != execution_service_id)
    {
        return Err(
            "project execution_service_id does not reference its workspace service".to_string(),
        );
    }
    Ok(Some(SandboxEnvironmentPlan {
        execution_service_id,
        services,
        generated_config_files: runtime
            .environment
            .generated_config_files
            .iter()
            .map(|file| SandboxGeneratedConfigFile {
                path: file.path.clone(),
                content: file.content.clone(),
            })
            .collect(),
    }))
}

fn runtime_image_is_program_managed_dependency(image: &ProjectRuntimeEnvironmentImage) -> bool {
    image.service_role.eq_ignore_ascii_case("dependency")
        && image.mcp_policy.managed_by.eq_ignore_ascii_case("system")
        && image.mcp_policy.attachment.eq_ignore_ascii_case("none")
        && !image.mcp_policy.filesystem
        && !image.mcp_policy.terminal
}

fn json_object_to_string_map(
    value: &serde_json::Value,
) -> std::collections::BTreeMap<String, String> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, value)| {
            let value = match value {
                serde_json::Value::String(value) => value.clone(),
                serde_json::Value::Number(value) => value.to_string(),
                serde_json::Value::Bool(value) => value.to_string(),
                _ => return None,
            };
            Some((name.clone(), value))
        })
        .collect()
}

fn merged_environment(
    global: &std::collections::BTreeMap<String, String>,
    service: &serde_json::Value,
) -> std::collections::BTreeMap<String, String> {
    let expanded_global = global
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                expand_environment_value(value.as_str(), global),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut environment = expanded_global.clone();
    environment.extend(
        json_object_to_string_map(service)
            .into_iter()
            .map(|(name, value)| {
                (
                    name,
                    expand_environment_value(value.as_str(), &expanded_global),
                )
            }),
    );
    environment
}

fn expand_environment_value(
    value: &str,
    global: &std::collections::BTreeMap<String, String>,
) -> String {
    let mut expanded = value.to_string();
    for _ in 0..8 {
        let next = expand_environment_value_once(expanded.as_str(), global);
        if next == expanded {
            break;
        }
        expanded = next;
    }
    expanded
}

fn expand_environment_value_once(
    value: &str,
    global: &std::collections::BTreeMap<String, String>,
) -> String {
    let mut output = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let expression = &remaining[start + 2..];
        let Some(end) = expression.find('}') else {
            output.push_str(&remaining[start..]);
            return output;
        };
        let token = &expression[..end];
        let (name, default_value) = token
            .split_once(":-")
            .map_or((token, None), |(name, fallback)| (name, Some(fallback)));
        if valid_environment_variable_name(name) {
            let replacement = global
                .get(name)
                .filter(|value| default_value.is_none() || !value.is_empty())
                .map(String::as_str)
                .or(default_value);
            if let Some(replacement) = replacement {
                output.push_str(replacement);
            } else {
                output.push_str(&remaining[start..start + end + 3]);
            }
        } else {
            output.push_str(&remaining[start..start + end + 3]);
        }
        remaining = &expression[end + 1..];
    }
    output.push_str(remaining);
    output
}

fn valid_environment_variable_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_' | 'A'..='Z' | 'a'..='z'))
        && chars.all(|character| matches!(character, '_' | 'A'..='Z' | 'a'..='z' | '0'..='9'))
}

fn sandbox_image_id_for_task(
    task: &TaskRecord,
    runtime: &ProjectSandboxRuntimeSettings,
    provider: &str,
    base_image_id: &str,
) -> Result<Option<String>, String> {
    if !task.mcp_config.requires_execution {
        return Ok(Some(normalize_base_image_id(base_image_id)));
    }
    ensure_project_runtime_ready(runtime)?;
    sandbox_image_id_for_runtime(runtime, provider, normalized_execution_service_id(task))?
        .map(Some)
        .ok_or_else(|| "project runtime environment has no ready workspace image".to_string())
}

fn ensure_project_runtime_ready(runtime: &ProjectSandboxRuntimeSettings) -> Result<(), String> {
    if !runtime.environment.sandbox_enabled {
        return Err("project sandbox environment is disabled".to_string());
    }
    if let Some(reason) = runtime
        .environment
        .not_runnable_reason
        .as_deref()
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
    {
        return Err(format!(
            "project sandbox environment is not runnable: {reason}"
        ));
    }
    let status = runtime.environment.status.trim().to_ascii_lowercase();
    if status != "ready" {
        return Err(format!(
            "project sandbox environment is not ready: status={}",
            if status.is_empty() {
                "pending"
            } else {
                status.as_str()
            }
        ));
    }
    Ok(())
}

fn local_sandbox_image_id_for_task(
    task: &TaskRecord,
    runtime: &ProjectSandboxRuntimeSettings,
    sandbox_mode: Option<SandboxBackendKind>,
) -> Result<Option<String>, String> {
    if sandbox_mode == Some(SandboxBackendKind::LocalProcess) {
        return Ok(None);
    }
    if !task.mcp_config.requires_execution {
        return Ok(Some(normalize_base_image_id(
            crate::config::configured_sandbox_base_image_id().as_str(),
        )));
    }
    sandbox_image_id_for_runtime(
        runtime,
        "local_connector",
        normalized_execution_service_id(task),
    )?
    .map(Some)
    .ok_or_else(|| {
        "Local Connector project has no ready program-managed local workspace image".to_string()
    })
}

fn sandbox_image_id_for_runtime(
    runtime: &ProjectSandboxRuntimeSettings,
    provider: &str,
    requested_service_id: Option<&str>,
) -> Result<Option<String>, String> {
    let images = runtime
        .images
        .iter()
        .filter(|image| image_status_is_available(image.status.as_str()))
        .filter(|image| runtime_image_is_program_managed_target(image))
        .filter(|image| {
            image.image_provider.trim().is_empty()
                || image.image_provider.eq_ignore_ascii_case(provider)
        })
        .filter(|image| {
            image
                .image_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
        })
        .collect::<Vec<_>>();
    if let Some(requested) = requested_service_id {
        if requested != "workspace" {
            return Err(
                "execution_service_id is fixed to the project workspace: workspace".to_string(),
            );
        }
        return images
            .into_iter()
            .find(|image| image.service_id.trim() == requested)
            .and_then(|image| image.image_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| {
                format!("execution_service_id is not the ready project workspace: {requested}")
            });
    }
    let project_workspace = runtime
        .environment
        .execution_service_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|workspace| {
            images
                .iter()
                .any(|image| image.service_id.trim() == *workspace)
        })
        .map(ToOwned::to_owned);
    let selected_service_id = project_workspace
        .or_else(|| (images.len() == 1).then(|| images[0].service_id.trim().to_string()));
    if let Some(selected_service_id) = selected_service_id {
        return Ok(images
            .iter()
            .find(|image| image.service_id.trim() == selected_service_id)
            .and_then(|image| image.image_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned));
    }
    Ok(images
        .first()
        .and_then(|image| image.image_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned))
}

fn normalized_execution_service_id(task: &TaskRecord) -> Option<&str> {
    task.mcp_config
        .execution_service_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn base_sandbox_image_id_for_task(task: &TaskRecord) -> Result<String, String> {
    if let Some(requested) = normalized_execution_service_id(task) {
        if requested != "workspace" {
            return Err(
                "execution_service_id is fixed to the project workspace: workspace".to_string(),
            );
        }
    }
    Ok(crate::config::configured_sandbox_base_image_id())
}

fn normalize_base_image_id(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "default".to_string()
    } else {
        value.to_string()
    }
}

fn image_status_is_available(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "ready" | "local" | "available" | "succeeded"
    )
}

fn runtime_image_is_program_managed_target(image: &ProjectRuntimeEnvironmentImage) -> bool {
    image.service_role.eq_ignore_ascii_case("workspace")
        && image.mcp_policy.managed_by.eq_ignore_ascii_case("system")
        && image
            .mcp_policy
            .attachment
            .eq_ignore_ascii_case("workspace_gateway_target")
        && image.mcp_policy.filesystem
        && image.mcp_policy.terminal
}

fn local_connector_project_ref(
    project: &TaskProjectRecord,
) -> Result<Option<LocalConnectorWorkspaceRef>, String> {
    let source_type = project
        .source_type
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    let local_source = source_type.eq_ignore_ascii_case("local")
        || source_type.eq_ignore_ascii_case("local_connector")
        || project
            .root_path
            .as_deref()
            .map(str::trim)
            .is_some_and(|root| {
                root.starts_with(chatos_project_execution::LOCAL_CONNECTOR_ROOT_PREFIX)
            });
    if !local_source {
        return Ok(None);
    }
    let root_path = project
        .root_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "Local Connector project is missing its managed workspace root".to_string()
        })?;
    parse_local_connector_workspace_root(root_path)
        .map(Some)
        .ok_or_else(|| "Local Connector project workspace root is invalid".to_string())
}

fn local_connector_policy_for_pairing(
    task: &TaskRecord,
    pairing: &LocalConnectorSandboxPairing,
) -> Result<SandboxLeasePolicyRequest, String> {
    let maximum = EffectiveSandboxPolicy {
        sandbox_mode: parse_sandbox_backend_kind(pairing.sandbox_mode.as_str())?,
        permission_profile_id: parse_permission_profile_id(pairing.permission_profile_id.as_str())?,
        approval_policy: parse_approval_policy(pairing.approval_policy.as_str())?,
        approval_reviewer: parse_approval_reviewer(pairing.approval_reviewer.as_str())?,
        policy_revision: pairing
            .policy_revision
            .as_deref()
            .and_then(normalized_text)
            .map(ToOwned::to_owned),
        additional_writable_roots: Vec::new(),
    };
    let mut requested = task.mcp_config.sandbox_policy_request();
    // Pairing/backend selection is a control-plane decision, never a model/task choice.
    requested.sandbox_mode = Some(maximum.sandbox_mode);
    let effective = EffectiveSandboxPolicy::resolve_no_broader_than(&requested, &maximum);
    Ok(SandboxLeasePolicyRequest {
        sandbox_mode: Some(effective.sandbox_mode),
        permission_profile_id: Some(effective.permission_profile_id),
        approval_policy: Some(effective.approval_policy),
        approval_reviewer: Some(effective.approval_reviewer),
        policy_revision: effective.policy_revision,
        additional_writable_roots: effective.additional_writable_roots,
    })
}

fn parse_sandbox_backend_kind(value: &str) -> Result<SandboxBackendKind, String> {
    value
        .parse::<SandboxBackendKind>()
        .map_err(|_| "Local Connector sandbox pairing has an invalid sandbox_mode".to_string())
}

fn parse_permission_profile_id(value: &str) -> Result<PermissionProfileId, String> {
    value.parse::<PermissionProfileId>().map_err(|_| {
        "Local Connector sandbox pairing has an invalid permission_profile_id".to_string()
    })
}

fn parse_approval_policy(value: &str) -> Result<ApprovalPolicy, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "never" => Ok(ApprovalPolicy::Never),
        "on_request" => Ok(ApprovalPolicy::OnRequest),
        _ => Err("Local Connector sandbox pairing has an invalid approval_policy".to_string()),
    }
}

fn parse_approval_reviewer(value: &str) -> Result<ApprovalReviewer, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto_review" => Ok(ApprovalReviewer::AutoReview),
        "user" => Ok(ApprovalReviewer::User),
        _ => Err("Local Connector sandbox pairing has an invalid approval_reviewer".to_string()),
    }
}

async fn resolve_local_connector_sandbox_route(
    config: &crate::config::AppConfig,
    task: &TaskRecord,
    project: &TaskProjectRecord,
    project_ref: &LocalConnectorWorkspaceRef,
) -> Result<LocalConnectorResolvedSandboxRoute, String> {
    let owner_user_id = task_owner_user_id(task)
        .ok_or_else(|| "Local Connector sandbox routing requires task owner user id".to_string())?;
    let project_owner_user_id = project
        .owner_user_id
        .as_deref()
        .and_then(normalized_text)
        .ok_or_else(|| "Local Connector project owner is not initialized".to_string())?;
    if project_owner_user_id != owner_user_id {
        return Err("Local Connector task and project owners do not match".to_string());
    }
    let secret = local_connector_internal_secret(config)?;
    let token = chatos_service_runtime::issue_internal_service_token_for_owner(
        secret.as_str(),
        "task-runner",
        "local-connector-service",
        "sandbox-routing.read",
        60,
        owner_user_id,
    )?;
    let service_base = local_connector_service_base_url(config)?;
    let response = config
        .local_connector_http_client
        .get(format!(
            "{}/api/local-connectors/sandbox-pairings",
            service_base.trim_end_matches('/')
        ))
        .query(&[
            ("active_only", "true"),
            ("device_id", project_ref.device_id.as_str()),
            ("workspace_id", project_ref.workspace_id.as_str()),
        ])
        .header("x-local-connector-caller", "task-runner")
        .header("x-local-connector-internal-token", token)
        .header("x-local-connector-owner-user-id", owner_user_id)
        .with_internal_trace_context()
        .send()
        .await
        .map_err(|err| format!("query Local Connector sandbox pairing failed: {err}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(
            "no active Local Connector sandbox pairing was found for this project".to_string(),
        );
    }
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!(
            "query Local Connector sandbox pairing returned HTTP {status}: {detail}"
        ));
    }
    let pairing = response
        .json::<Vec<LocalConnectorSandboxPairing>>()
        .await
        .map_err(|err| format!("decode Local Connector sandbox pairing failed: {err}"))?
        .into_iter()
        .find(|pairing| {
            pairing.enabled
                && pairing.device_id == project_ref.device_id
                && pairing.workspace_id == project_ref.workspace_id
                && pairing.sandbox_readiness.trim().eq_ignore_ascii_case("ready")
        })
        .ok_or_else(|| {
            "no enabled, ready, and online Local Connector sandbox pairing was found for this project"
                .to_string()
        })?;
    let pairing_id = normalized_text(pairing.id.as_str())
        .ok_or_else(|| "Local Connector sandbox pairing id is empty".to_string())?
        .to_string();
    let base_url = format!(
        "{}/api/local-connectors/sandbox-facade/{}",
        service_base.trim_end_matches('/'),
        urlencoding::encode(pairing_id.as_str())
    );
    Ok(LocalConnectorResolvedSandboxRoute {
        base_url,
        pairing_id,
        policy: local_connector_policy_for_pairing(task, &pairing)?,
    })
}

fn sandbox_auth_for_task(
    config: &crate::config::AppConfig,
    task: &TaskRecord,
    base_url: &str,
) -> Result<Option<SandboxManagerAuth>, String> {
    if is_local_connector_sandbox_manager(base_url) {
        let owner_user_id = task_owner_user_id(task).ok_or_else(|| {
            "Local Connector sandbox auth requires task owner user id".to_string()
        })?;
        return Ok(Some(SandboxManagerAuth::local_connector(
            local_connector_internal_secret(config)?,
            owner_user_id.to_string(),
            config.local_connector_http_client.clone(),
        )));
    }
    Ok(SandboxManagerAuth::from_config(config))
}

fn local_connector_service_base_url(config: &crate::config::AppConfig) -> Result<String, String> {
    config
        .local_connector_service_base_url
        .as_deref()
        .map(str::trim)
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL is required from configuration center for local sandbox routing"
                .to_string()
        })
}

fn local_connector_internal_secret(config: &crate::config::AppConfig) -> Result<String, String> {
    config
        .local_connector_internal_api_secret
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required for local sandbox routing"
                .to_string()
        })
}

#[cfg(test)]
mod tests;
