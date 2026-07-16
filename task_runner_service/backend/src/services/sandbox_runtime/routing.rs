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
        let image_id = sandbox_image_id_for_task(task, &runtime, provider.as_str())?;
        let auth = sandbox_auth_for_task(&self.config, task, base_url.as_str())?;
        Ok(SandboxTaskRoute {
            base_url,
            auth,
            image_id,
            provider,
            policy,
        })
    }
}

fn sandbox_image_id_for_task(
    task: &TaskRecord,
    runtime: &ProjectSandboxRuntimeSettings,
    provider: &str,
) -> Result<Option<String>, String> {
    if !task.mcp_config.requires_execution {
        return Ok(Some("default".to_string()));
    }
    let image = runtime
        .images
        .iter()
        .filter(|image| image_status_is_available(image.status.as_str()))
        .filter(|image| {
            image.image_provider.trim().is_empty()
                || image.image_provider.eq_ignore_ascii_case(provider)
        })
        .filter_map(|image| {
            image
                .image_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|image_id| (runtime_image_rank(image), image_id.to_string()))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, image_id)| image_id);
    image.map(Some).ok_or_else(|| {
        format!(
            "project runtime image is not ready (environment_status={}); reinitialize the project environment image or create the task with requires_execution=false for file-only work",
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

fn runtime_image_rank(image: &ProjectRuntimeEnvironmentImage) -> u8 {
    let kind = image.environment_type.trim().to_ascii_lowercase();
    if kind.contains("runtime") || kind.contains("application") || kind.contains("project") {
        0
    } else if kind.contains("service") || kind.contains("database") || kind.contains("cache") {
        20
    } else {
        10
    }
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
