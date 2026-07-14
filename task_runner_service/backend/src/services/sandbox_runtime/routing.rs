// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::StatusCode;
use serde::Deserialize;

use super::*;
use crate::models::{TaskProjectRecord, PUBLIC_PROJECT_ID};
use crate::services::project_management_api_client::{
    self, ProjectRuntimeEnvironmentImage, ProjectSandboxRuntimeSettings,
};

const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local-connector://";

#[derive(Debug)]
struct LocalConnectorProjectRef {
    device_id: String,
    workspace_id: String,
}

#[derive(Debug, Deserialize)]
struct LocalConnectorSandboxPairing {
    id: String,
    device_id: String,
    workspace_id: String,
    enabled: bool,
    #[serde(default)]
    facade_base_url: Option<String>,
}

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
            let auth = sandbox_auth_for_task(&self.config, task, base_url.as_str())?;
            return Ok(SandboxTaskRoute {
                base_url,
                auth,
                image_id: (!task.mcp_config.requires_execution).then(|| "default".to_string()),
                provider: "task_override".to_string(),
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
        let local_ref = local_connector_project_ref(&project);
        let (base_url, provider) = if let Some(project_ref) = local_ref.as_ref() {
            (
                resolve_local_connector_sandbox_base_url(&self.config, task, &project, project_ref)
                    .await?,
                "local_connector".to_string(),
            )
        } else {
            (
                self.effective_sandbox_manager_base_url().await?,
                if runtime.environment.sandbox_provider.trim().is_empty() {
                    "cloud_sandbox_manager".to_string()
                } else {
                    runtime.environment.sandbox_provider.clone()
                },
            )
        };
        let image_id = sandbox_image_id_for_task(task, &runtime, provider.as_str())?;
        let auth = sandbox_auth_for_task(&self.config, task, base_url.as_str())?;
        Ok(SandboxTaskRoute {
            base_url,
            auth,
            image_id,
            provider,
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

fn local_connector_project_ref(project: &TaskProjectRecord) -> Option<LocalConnectorProjectRef> {
    let source_type = project
        .source_type
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    let root_path = project.root_path.as_deref()?.trim();
    if !source_type.eq_ignore_ascii_case("local_connector")
        && !root_path.starts_with(LOCAL_CONNECTOR_ROOT_PREFIX)
    {
        return None;
    }
    let rest = root_path.strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX)?;
    let mut parts = rest.split('/').filter(|part| !part.trim().is_empty());
    let device_id = parts.next()?.trim().to_string();
    let workspace_id = parts.next()?.trim().to_string();
    if device_id.is_empty() || workspace_id.is_empty() {
        return None;
    }
    Some(LocalConnectorProjectRef {
        device_id,
        workspace_id,
    })
}

async fn resolve_local_connector_sandbox_base_url(
    config: &crate::config::AppConfig,
    task: &TaskRecord,
    project: &TaskProjectRecord,
    project_ref: &LocalConnectorProjectRef,
) -> Result<String, String> {
    let owner_user_id = task_owner_user_id(task)
        .or_else(|| project.owner_user_id.as_deref().and_then(normalized_text))
        .ok_or_else(|| "Local Connector sandbox routing requires task owner user id".to_string())?;
    let secret = local_connector_internal_secret(config)?;
    let token = chatos_service_runtime::issue_internal_service_token(
        secret.as_str(),
        "task-runner",
        "local-connector-service",
        "sandbox-routing.read",
        60,
    )?;
    let service_base = local_connector_service_base_url();
    let response = reqwest::Client::builder()
        .timeout(local_connector_service_request_timeout())
        .build()
        .map_err(|err| format!("build Local Connector sandbox routing client failed: {err}"))?
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
        })
        .ok_or_else(|| {
            "no enabled and online Local Connector sandbox pairing was found for this project"
                .to_string()
        })?;
    let configured_facade = format!(
        "{}/api/local-connectors/sandbox-facade/{}",
        service_base.trim_end_matches('/'),
        urlencoding::encode(pairing.id.as_str())
    );
    Ok(pairing
        .facade_base_url
        .as_deref()
        .and_then(normalized_text)
        .filter(|url| url.starts_with(service_base.trim_end_matches('/')))
        .unwrap_or(configured_facade.as_str())
        .trim_end_matches('/')
        .to_string())
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
        return Ok(Some(SandboxManagerAuth {
            client_id: "task-runner".to_string(),
            client_key: local_connector_internal_secret(config)?,
            mode: SandboxManagerAuthMode::LocalConnector,
            owner_user_id: Some(owner_user_id.to_string()),
        }));
    }
    Ok(SandboxManagerAuth::from_config(config))
}

fn task_owner_user_id(task: &TaskRecord) -> Option<&str> {
    task.owner_user_id
        .as_deref()
        .and_then(normalized_text)
        .or_else(|| task.creator_user_id.as_deref().and_then(normalized_text))
        .or_else(|| normalized_text(task.subject_id.as_str()))
}

fn normalized_text(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn local_connector_service_base_url() -> String {
    std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL")
        .ok()
        .or_else(|| std::env::var("LOCAL_CONNECTOR_SERVICE_BASE_URL").ok())
        .or_else(|| std::env::var("CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL").ok())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:39230".to_string())
}

fn local_connector_service_request_timeout() -> std::time::Duration {
    let timeout_ms = std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS")
        .ok()
        .or_else(|| std::env::var("LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS").ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(5_000)
        .max(300);
    std::time::Duration::from_millis(timeout_ms)
}

fn local_connector_internal_secret(config: &crate::config::AppConfig) -> Result<String, String> {
    config
        .local_connector_internal_api_secret
        .clone()
        .or_else(|| std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required for local sandbox routing"
                .to_string()
        })
}
