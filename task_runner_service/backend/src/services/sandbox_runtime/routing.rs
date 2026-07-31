// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::{TaskProjectRecord, PUBLIC_PROJECT_ID};
use crate::services::project_management_api_client::{
    self, ProjectRuntimeEnvironmentImage, ProjectSandboxRuntimeSettings,
};

const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";

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
        if let Some(base_url) = task
            .mcp_config
            .sandbox_manager_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let base_url = base_url.trim_end_matches('/').to_string();
            if is_local_connector_sandbox_manager(base_url.as_str()) {
                return Err(
                    "Local Connector Sandbox is unavailable in cloud Task Runner".to_string(),
                );
            }
            let auth = sandbox_auth_for_task(&self.config, task, base_url.as_str())?;
            return Ok(SandboxTaskRoute {
                base_url,
                auth,
                image_id: Some(base_sandbox_image_id_for_task(task)?),
                environment_plan: None,
                provider: "task_override".to_string(),
                policy: task.mcp_config.sandbox_policy_request(),
            });
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
        let local_project = project_uses_local_runtime(&project);
        let task_policy = task.mcp_config.sandbox_policy_request();
        let (base_url, provider, policy) = if local_project {
            return Err(
                "local_runtime_required: Local Connector 项目不能进入云端 Sandbox".to_string(),
            );
        } else {
            (
                self.effective_sandbox_manager_base_url().await?,
                "cloud_sandbox_manager".to_string(),
                task_policy,
            )
        };
        let environment_plan = if runtime_topology_v2_enabled() {
            sandbox_environment_plan_for_task(task, &runtime, provider.as_str())?
        } else {
            None
        };
        // Keep the resolved workspace/base image even when topology v2 is selected.
        // The environment plan may be stale or temporarily unbuildable after a previous
        // task changes the repository. Generic implementation tasks must still be able to
        // enter a plain execution sandbox and repair the project instead of failing before
        // the model gets any tools. `create_lease` ignores this value while the environment
        // plan succeeds and uses it only for the guarded fallback path.
        let image_id = sandbox_image_id_for_task(
            task,
            &runtime,
            provider.as_str(),
            crate::config::configured_sandbox_base_image_id().as_str(),
        )?;
        let auth = sandbox_auth_for_task(&self.config, task, base_url.as_str())?;
        Ok(SandboxTaskRoute {
            base_url,
            auth,
            image_id,
            environment_plan,
            provider,
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

pub(super) fn sandbox_environment_fallback_allowed(
    task: &TaskRecord,
    route: &SandboxTaskRoute,
) -> bool {
    route.environment_plan.is_some()
        && route.image_id.is_some()
        && normalized_execution_service_id(task).is_none_or(|service_id| service_id == "workspace")
}

fn sandbox_environment_plan_for_task(
    task: &TaskRecord,
    runtime: &ProjectSandboxRuntimeSettings,
    provider: &str,
) -> Result<Option<SandboxEnvironmentPlan>, String> {
    if !task.mcp_config.requires_execution {
        return Ok(None);
    }
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
        return Ok(None);
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
    let mut environment = global.clone();
    environment.extend(
        json_object_to_string_map(service)
            .into_iter()
            .map(|(name, value)| (name, expand_environment_value(value.as_str(), global))),
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
    match sandbox_image_id_for_runtime(runtime, provider, normalized_execution_service_id(task))? {
        Some(image_id) => Ok(Some(image_id)),
        None => Ok(Some(normalize_base_image_id(base_image_id))),
    }
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

fn project_uses_local_runtime(project: &TaskProjectRecord) -> bool {
    let source_type = project
        .source_type
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    source_type.eq_ignore_ascii_case("local")
        || source_type.eq_ignore_ascii_case("local_connector")
        || project
            .root_path
            .as_deref()
            .map(str::trim)
            .is_some_and(|root| root.starts_with(LOCAL_CONNECTOR_ROOT_PREFIX))
}

fn sandbox_auth_for_task(
    config: &crate::config::AppConfig,
    task: &TaskRecord,
    base_url: &str,
) -> Result<Option<SandboxManagerAuth>, String> {
    if is_local_connector_sandbox_manager(base_url) {
        return Err("Local Connector Sandbox is unavailable in cloud Task Runner".to_string());
    }
    let _ = task;
    Ok(SandboxManagerAuth::from_config(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::project_management_api_client::{
        ProjectRuntimeEnvironmentMcpPolicy, ProjectRuntimeEnvironmentSettings,
    };

    fn image(
        service_role: &str,
        attachment: &str,
        filesystem: bool,
        terminal: bool,
    ) -> ProjectRuntimeEnvironmentImage {
        ProjectRuntimeEnvironmentImage {
            environment_key: "services/api".to_string(),
            service_id: "services-api".to_string(),
            display_name: "API".to_string(),
            service_role: service_role.to_string(),
            mcp_policy: ProjectRuntimeEnvironmentMcpPolicy {
                managed_by: "system".to_string(),
                attachment: attachment.to_string(),
                filesystem,
                terminal,
            },
            image_id: Some("image-1".to_string()),
            image_ref: None,
            image_provider: "cloud_sandbox_manager".to_string(),
            status: "ready".to_string(),
            dockerfile: Some("FROM alpine\n".to_string()),
            env_vars: serde_json::json!({}),
        }
    }

    #[test]
    fn only_system_managed_workspace_targets_are_routable() {
        assert!(runtime_image_is_program_managed_target(&image(
            "workspace",
            "workspace_gateway_target",
            true,
            true,
        )));
        assert!(!runtime_image_is_program_managed_target(&image(
            "application",
            "project_gateway_target",
            true,
            true,
        )));
        assert!(!runtime_image_is_program_managed_target(&image(
            "application",
            "none",
            false,
            false,
        )));
    }

    #[test]
    fn project_workspace_is_used_without_task_level_selection() {
        let mut workspace = image("workspace", "workspace_gateway_target", true, true);
        workspace.environment_key = "workspace".to_string();
        workspace.service_id = "workspace".to_string();
        workspace.display_name = "Project Workspace".to_string();
        workspace.dockerfile = None;
        let mut api = image("application", "none", false, false);
        api.image_id = None;
        let runtime = ProjectSandboxRuntimeSettings {
            environment: ProjectRuntimeEnvironmentSettings {
                sandbox_enabled: true,
                status: "ready".to_string(),
                execution_service_id: Some("workspace".to_string()),
                env_vars: serde_json::json!({}),
                generated_config_files: Vec::new(),
            },
            images: vec![api, workspace],
        };

        assert_eq!(
            sandbox_image_id_for_runtime(&runtime, "cloud_sandbox_manager", None)
                .expect("project workspace image"),
            Some("image-1".to_string())
        );
    }

    #[test]
    fn environment_plan_contains_only_workspace_and_dependencies() {
        let mut workspace = image("workspace", "workspace_gateway_target", true, true);
        workspace.environment_key = "workspace".to_string();
        workspace.service_id = "workspace".to_string();
        workspace.display_name = "Project Workspace".to_string();
        workspace.dockerfile = None;
        let mut application = image("application", "none", false, false);
        application.image_id = None;
        let mut artifact = image("artifact", "none", false, false);
        artifact.service_id = "web-prototype".to_string();
        artifact.image_id = None;
        let mut dependency = image("dependency", "none", false, false);
        dependency.environment_key = "postgresql".to_string();
        dependency.service_id = "postgresql".to_string();
        dependency.image_id = None;
        dependency.image_ref = Some("postgres:16-alpine".to_string());
        dependency.dockerfile = None;
        let runtime = ProjectSandboxRuntimeSettings {
            environment: ProjectRuntimeEnvironmentSettings {
                sandbox_enabled: true,
                status: "ready".to_string(),
                execution_service_id: Some("workspace".to_string()),
                env_vars: serde_json::json!({}),
                generated_config_files: Vec::new(),
            },
            images: vec![workspace, application, artifact, dependency],
        };
        let mut task = task();
        task.mcp_config.requires_execution = true;

        let plan = sandbox_environment_plan_for_task(&task, &runtime, "cloud_sandbox_manager")
            .expect("workspace plan")
            .expect("environment topology");
        assert_eq!(plan.execution_service_id, "workspace");
        assert_eq!(
            plan.services
                .iter()
                .map(|service| service.service_id.as_str())
                .collect::<Vec<_>>(),
            vec!["workspace", "postgresql"]
        );
    }

    #[test]
    fn runtime_topology_v2_feature_flag_defaults_on_and_can_fail_back() {
        assert!(runtime_topology_v2_enabled_from_value(None));
        assert!(runtime_topology_v2_enabled_from_value(Some("true")));
        assert!(!runtime_topology_v2_enabled_from_value(Some("false")));
        assert!(!runtime_topology_v2_enabled_from_value(Some("0")));
    }

    #[test]
    fn workspace_route_can_fall_back_but_business_service_cannot() {
        let mut task = task();
        task.mcp_config.requires_execution = true;
        let route = SandboxTaskRoute {
            base_url: "http://sandbox.example".to_string(),
            auth: None,
            image_id: Some("dev-node24".to_string()),
            environment_plan: Some(SandboxEnvironmentPlan {
                execution_service_id: "workspace".to_string(),
                services: Vec::new(),
                generated_config_files: Vec::new(),
            }),
            provider: "cloud_sandbox_manager".to_string(),
            policy: task.mcp_config.sandbox_policy_request(),
        };

        assert!(sandbox_environment_fallback_allowed(&task, &route));

        task.mcp_config.execution_service_id = Some("workspace".to_string());
        assert!(sandbox_environment_fallback_allowed(&task, &route));

        task.mcp_config.execution_service_id = Some("api".to_string());
        assert!(!sandbox_environment_fallback_allowed(&task, &route));
    }

    #[test]
    fn code_or_terminal_execution_without_workspace_uses_configured_base_image() {
        let runtime = ProjectSandboxRuntimeSettings {
            environment: ProjectRuntimeEnvironmentSettings {
                sandbox_enabled: true,
                status: "pending".to_string(),
                execution_service_id: None,
                env_vars: serde_json::json!({}),
                generated_config_files: Vec::new(),
            },
            images: Vec::new(),
        };
        let mut task = task();
        task.mcp_config.requires_execution = true;

        assert!(
            sandbox_environment_plan_for_task(&task, &runtime, "cloud_sandbox_manager")
                .expect("empty project runtime must fall back")
                .is_none()
        );
        assert_eq!(
            sandbox_image_id_for_task(&task, &runtime, "cloud_sandbox_manager", "dev-java21",)
                .expect("configured base image"),
            Some("dev-java21".to_string())
        );
    }

    #[test]
    fn explicit_business_service_execution_is_rejected() {
        let runtime = ProjectSandboxRuntimeSettings {
            environment: ProjectRuntimeEnvironmentSettings {
                sandbox_enabled: true,
                status: "pending".to_string(),
                execution_service_id: None,
                env_vars: serde_json::json!({}),
                generated_config_files: Vec::new(),
            },
            images: Vec::new(),
        };
        let mut task = task();
        task.mcp_config.requires_execution = true;
        task.mcp_config.execution_service_id = Some("services-api".to_string());

        let error = sandbox_environment_plan_for_task(&task, &runtime, "cloud_sandbox_manager")
            .expect_err("business service must not become execution target");
        assert!(error.contains("services-api"));
        assert!(error.contains("not the ready project workspace"));
    }

    #[test]
    fn dependency_services_do_not_serialize_an_mcp_policy() {
        let mut workspace = image("workspace", "workspace_gateway_target", true, true);
        workspace.environment_key = "workspace".to_string();
        workspace.service_id = "workspace".to_string();
        workspace.display_name = "Project Workspace".to_string();
        workspace.dockerfile = None;
        let mut dependency = image("dependency", "none", false, false);
        dependency.environment_key = "postgresql".to_string();
        dependency.service_id = "postgresql".to_string();
        dependency.display_name = "PostgreSQL".to_string();
        dependency.image_id = None;
        dependency.image_ref = Some("postgres:16-alpine".to_string());
        dependency.dockerfile = None;
        let runtime = ProjectSandboxRuntimeSettings {
            environment: ProjectRuntimeEnvironmentSettings {
                sandbox_enabled: true,
                status: "ready".to_string(),
                execution_service_id: Some("workspace".to_string()),
                env_vars: serde_json::json!({}),
                generated_config_files: Vec::new(),
            },
            images: vec![workspace, dependency],
        };
        let mut task = task();
        task.mcp_config.requires_execution = true;
        task.mcp_config.execution_service_id = Some("workspace".to_string());

        let plan = sandbox_environment_plan_for_task(&task, &runtime, "cloud_sandbox_manager")
            .expect("environment plan")
            .expect("environment topology");
        let services = serde_json::to_value(plan.services).expect("serialize services");
        let dependency = services
            .as_array()
            .and_then(|items| {
                items.iter().find(|item| {
                    item.get("service_role").and_then(Value::as_str) == Some("dependency")
                })
            })
            .expect("dependency service");
        assert!(dependency.get("mcp_policy").is_none());
    }

    #[test]
    fn service_environment_templates_resolve_from_project_values() {
        let global = json_object_to_string_map(&serde_json::json!({
            "POSTGRES_DB": "mdm_service",
            "POSTGRES_USER": "mdm_service",
            "POSTGRES_PASSWORD": "generated-secret",
        }));
        let environment = merged_environment(
            &global,
            &serde_json::json!({
                "POSTGRES_DB": "${POSTGRES_DB:-app}",
                "POSTGRES_USER": "${POSTGRES_USER}",
                "POSTGRES_PASSWORD": "${POSTGRES_PASSWORD}",
                "DATABASE_URL": "postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgresql:5432/${POSTGRES_DB}",
                "UNRESOLVED": "${MISSING_VALUE}",
            }),
        );

        assert_eq!(
            environment.get("POSTGRES_DB").map(String::as_str),
            Some("mdm_service")
        );
        assert_eq!(
            environment.get("POSTGRES_USER").map(String::as_str),
            Some("mdm_service")
        );
        assert_eq!(
            environment.get("POSTGRES_PASSWORD").map(String::as_str),
            Some("generated-secret")
        );
        assert_eq!(
            environment.get("DATABASE_URL").map(String::as_str),
            Some("postgresql://mdm_service:generated-secret@postgresql:5432/mdm_service")
        );
        assert_eq!(
            environment.get("UNRESOLVED").map(String::as_str),
            Some("${MISSING_VALUE}")
        );
    }

    fn task() -> TaskRecord {
        TaskRecord {
            id: "task-1".to_string(),
            title: "Task".to_string(),
            description: None,
            objective: "Test sandbox routing".to_string(),
            input_payload: None,
            status: crate::models::TaskStatus::Ready,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: None,
            memory_thread_id: "task-task-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            subject_id: "user-1".to_string(),
            project_id: "project-1".to_string(),
            task_profile: crate::models::default_task_profile(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: None,
            owner_username: None,
            owner_display_name: None,
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: crate::models::TaskScheduleConfig::default(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: None,
            source_turn_id: None,
            source_user_message_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: crate::models::TaskToolState::default(),
            plugin_config: Default::default(),
            mcp_config: crate::models::TaskMcpConfig::default(),
            created_at: "2026-07-19T00:00:00Z".to_string(),
            updated_at: "2026-07-19T00:00:00Z".to_string(),
            deleted_at: None,
        }
    }
}
