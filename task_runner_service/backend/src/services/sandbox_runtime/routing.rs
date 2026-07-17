// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::{TaskProjectRecord, PUBLIC_PROJECT_ID};
use crate::services::project_management_api_client::{
    self, ProjectRuntimeEnvironmentImage, ProjectSandboxRuntimeSettings,
};

const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";

impl RunService {
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
                image_id: (!task.mcp_config.requires_execution).then(|| "default".to_string()),
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
                image_id: (!task.mcp_config.requires_execution).then(|| "default".to_string()),
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
        let image_id = if environment_plan.is_some() {
            None
        } else {
            sandbox_image_id_for_task(task, &runtime, provider.as_str())?
        };
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
    let mut application_service_ids = Vec::new();
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
                .ok_or_else(|| format!("application service {service_id} has no ready image_id"))?;
            let dockerfile = image
                .dockerfile
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("application service {service_id} has no Dockerfile"))?;
            application_service_ids.push(service_id.to_string());
            services.push(SandboxEnvironmentServicePlan {
                service_id: service_id.to_string(),
                environment_key: image.environment_key.clone(),
                display_name: if image.display_name.trim().is_empty() {
                    service_id.to_string()
                } else {
                    image.display_name.clone()
                },
                service_role: "application".to_string(),
                image_id: Some(image_id.to_string()),
                image_ref: image.image_ref.clone(),
                dockerfile: Some(dockerfile.to_string()),
                environment: merged_environment(&global_environment, &image.env_vars),
                mcp_policy: SandboxEnvironmentMcpPolicyPlan {
                    managed_by: "system".to_string(),
                    attachment: "project_gateway_target".to_string(),
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
                mcp_policy: SandboxEnvironmentMcpPolicyPlan {
                    managed_by: "system".to_string(),
                    attachment: "none".to_string(),
                    filesystem: false,
                    terminal: false,
                },
            });
        }
    }
    if application_service_ids.is_empty() {
        return Ok(None);
    }
    let primary_service_id = resolve_execution_service_id(
        task.mcp_config.execution_service_id.as_deref(),
        application_service_ids.as_slice(),
    )?;
    Ok(Some(SandboxEnvironmentPlan {
        primary_service_id,
        services,
    }))
}

fn resolve_execution_service_id(
    requested: Option<&str>,
    application_service_ids: &[String],
) -> Result<String, String> {
    match requested.map(str::trim).filter(|value| !value.is_empty()) {
        Some(requested) if application_service_ids.iter().any(|value| value == requested) => {
            Ok(requested.to_string())
        }
        Some(requested) => Err(format!(
            "execution_service_id is not a ready application service: {requested}"
        )),
        None if application_service_ids.len() == 1 => Ok(application_service_ids[0].clone()),
        None => Err(format!(
                "project runtime has multiple application services ({}); execution_service_id must be selected by the user or program",
                application_service_ids.join(", ")
            )),
    }
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
    environment.extend(json_object_to_string_map(service));
    environment
}

fn sandbox_image_id_for_task(
    task: &TaskRecord,
    runtime: &ProjectSandboxRuntimeSettings,
    provider: &str,
) -> Result<Option<String>, String> {
    if !task.mcp_config.requires_execution {
        return Ok(Some("default".to_string()));
    }
    sandbox_image_id_for_runtime(runtime, provider).map(Some)
}

fn sandbox_image_id_for_runtime(
    runtime: &ProjectSandboxRuntimeSettings,
    provider: &str,
) -> Result<String, String> {
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
    if images.len() > 1 {
        let service_ids = images
            .iter()
            .map(|image| {
                if image.service_id.trim().is_empty() {
                    image.environment_key.as_str()
                } else {
                    image.service_id.as_str()
                }
            })
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "project runtime has multiple ready system-managed application targets ({service_ids}); program-controlled service selection is required before cloud execution"
        ));
    }
    images
        .first()
        .and_then(|image| image.image_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            format!(
                "project runtime has no ready system-managed application MCP target (environment_status={}); reinitialize the project environment image or create the task with requires_execution=false for file-only work",
                runtime.environment.status.trim()
            )
        })
}

fn image_status_is_available(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "ready" | "local" | "available" | "succeeded"
    )
}

fn runtime_image_is_program_managed_target(image: &ProjectRuntimeEnvironmentImage) -> bool {
    image.service_role.eq_ignore_ascii_case("application")
        && image.mcp_policy.managed_by.eq_ignore_ascii_case("system")
        && image
            .mcp_policy
            .attachment
            .eq_ignore_ascii_case("project_gateway_target")
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
    fn only_system_managed_application_targets_are_routable() {
        assert!(runtime_image_is_program_managed_target(&image(
            "application",
            "project_gateway_target",
            true,
            true,
        )));
        assert!(!runtime_image_is_program_managed_target(&image(
            "dependency",
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
    fn multiple_application_targets_are_never_selected_by_array_order() {
        let api = image("application", "project_gateway_target", true, true);
        let mut worker = image("application", "project_gateway_target", true, true);
        worker.environment_key = "services/worker".to_string();
        worker.service_id = "services-worker".to_string();
        worker.image_id = Some("image-2".to_string());
        let runtime = ProjectSandboxRuntimeSettings {
            environment: ProjectRuntimeEnvironmentSettings {
                sandbox_enabled: true,
                status: "ready".to_string(),
                env_vars: serde_json::json!({}),
            },
            images: vec![api, worker],
        };

        let error = sandbox_image_id_for_runtime(&runtime, "cloud_sandbox_manager")
            .expect_err("ambiguous application targets must be rejected");
        assert!(error.contains("services-api, services-worker"));
        assert!(error.contains("program-controlled service selection"));
    }

    #[test]
    fn environment_execution_service_is_selected_only_explicitly_or_for_single_application() {
        let service_ids = vec!["api".to_string(), "worker".to_string()];
        let error = resolve_execution_service_id(None, service_ids.as_slice())
            .expect_err("multiple applications must be explicit");
        assert!(error.contains("user or program"));
        assert_eq!(
            resolve_execution_service_id(Some("worker"), service_ids.as_slice())
                .expect("explicit selection"),
            "worker"
        );
        assert!(resolve_execution_service_id(Some("redis"), service_ids.as_slice()).is_err());
        assert_eq!(
            resolve_execution_service_id(None, &["api".to_string()])
                .expect("single application auto selection"),
            "api"
        );
    }

    #[test]
    fn runtime_topology_v2_feature_flag_defaults_on_and_can_fail_back() {
        assert!(runtime_topology_v2_enabled_from_value(None));
        assert!(runtime_topology_v2_enabled_from_value(Some("true")));
        assert!(!runtime_topology_v2_enabled_from_value(Some("false")));
        assert!(!runtime_topology_v2_enabled_from_value(Some("0")));
    }
}
