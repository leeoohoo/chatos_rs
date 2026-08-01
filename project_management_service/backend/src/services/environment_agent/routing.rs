// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) use chatos_project_execution::{
    parse_local_connector_workspace_root as parse_local_connector_project_root,
    LocalConnectorWorkspaceRef as LocalConnectorProjectRef,
};
use chatos_service_runtime::http_body::{
    read_response_json_limited, read_response_preview_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::config::AppConfig;
use crate::models::{
    ProjectImportStatus, ProjectRecord, ProjectRuntimeEnvironmentStatus, ProjectSourceType,
    RuntimeEnvironmentProvider,
};

#[derive(Debug)]
pub(super) enum RuntimeEnvironmentDecision {
    Ready(RuntimeEnvironmentPlan),
    Stop(StopDecision),
}

/// Business plan persisted on the project runtime environment.
///
/// This describes where the project workspace and its eventual runtime live. It
/// is not an MCP Provider route: MCP Management consumes the normalized Project
/// Execution Context and owns the actual tool Provider selection.
#[derive(Debug)]
pub(super) struct RuntimeEnvironmentPlan {
    pub(super) file_provider: RuntimeEnvironmentProvider,
    pub(super) sandbox_provider: RuntimeEnvironmentProvider,
}

#[derive(Debug)]
pub(super) struct StopDecision {
    pub(super) status: ProjectRuntimeEnvironmentStatus,
    pub(super) summary: String,
    pub(super) not_runnable_reason: Option<String>,
    pub(super) last_error: Option<String>,
}

pub(super) async fn resolve_runtime_environment_plan(
    project: &ProjectRecord,
    config: &AppConfig,
    user_access_token: Option<&str>,
) -> RuntimeEnvironmentDecision {
    match project.source_type {
        ProjectSourceType::Cloud => resolve_cloud_plan(project),
        ProjectSourceType::Local | ProjectSourceType::LocalConnector => {
            resolve_local_plan(project, config, user_access_token).await
        }
    }
}

fn resolve_cloud_plan(project: &ProjectRecord) -> RuntimeEnvironmentDecision {
    match project.import_status {
        ProjectImportStatus::Pending | ProjectImportStatus::Importing => {
            return RuntimeEnvironmentDecision::Stop(StopDecision {
                status: ProjectRuntimeEnvironmentStatus::Pending,
                summary: "云端项目代码仍在导入中，导入完成后再执行运行环境初始化。".to_string(),
                not_runnable_reason: None,
                last_error: None,
            });
        }
        ProjectImportStatus::Failed => {
            let reason = project
                .import_error
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("云端项目导入失败");
            return RuntimeEnvironmentDecision::Stop(not_runnable(format!(
                "云端项目导入失败，暂时不具备运行环境初始化条件：{reason}"
            )));
        }
        ProjectImportStatus::Ready | ProjectImportStatus::None => {}
    }
    if project
        .harness_repo_identifier
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return RuntimeEnvironmentDecision::Stop(not_runnable(
            "云端项目缺少 Harness 仓库信息，无法通过 Harness MCP 读取项目文件。",
        ));
    }
    // `cloud_import_source=empty` describes how the project was created, not
    // whether its Harness repository is still empty. Task Runner runs may add
    // code later, so the environment agent must inspect the current repository
    // instead of permanently short-circuiting on creation-time metadata.
    RuntimeEnvironmentDecision::Ready(RuntimeEnvironmentPlan {
        file_provider: RuntimeEnvironmentProvider::Harness,
        sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
    })
}

async fn resolve_local_plan(
    project: &ProjectRecord,
    config: &AppConfig,
    user_access_token: Option<&str>,
) -> RuntimeEnvironmentDecision {
    let Some(root_path) = project
        .root_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return RuntimeEnvironmentDecision::Stop(not_runnable(
            "本地项目缺少根目录，无法读取项目文件。",
        ));
    };
    let Some(local_connector_ref) = parse_local_connector_project_root(root_path) else {
        return RuntimeEnvironmentDecision::Stop(not_runnable(
            "本地项目必须使用 Local Connector 管理的逻辑 Workspace；服务器不会读取客户端绝对路径。",
        ));
    };

    let sandbox_provider = match choose_sandbox_provider(
        config,
        user_access_token,
        Some(&local_connector_ref),
    )
    .await
    {
        Ok(Some(provider)) => provider,
        Ok(None) => {
            return RuntimeEnvironmentDecision::Stop(waiting_for_local_sandbox(
                "等待该 Workspace 的 Local Connector Sandbox pairing 启用并上线；系统不会回退到 Cloud Sandbox。",
            ));
        }
        Err(err) => {
            return RuntimeEnvironmentDecision::Stop(failed_stop(
                "检查本地沙箱可用性失败，无法确定运行环境镜像后端。",
                err,
            ));
        }
    };
    RuntimeEnvironmentDecision::Ready(RuntimeEnvironmentPlan {
        file_provider: RuntimeEnvironmentProvider::LocalConnector,
        sandbox_provider,
    })
}

async fn choose_sandbox_provider(
    config: &AppConfig,
    user_access_token: Option<&str>,
    project_ref: Option<&LocalConnectorProjectRef>,
) -> Result<Option<RuntimeEnvironmentProvider>, String> {
    if has_enabled_local_sandbox_pairing(config, user_access_token, project_ref).await? {
        Ok(Some(RuntimeEnvironmentProvider::LocalConnector))
    } else {
        Ok(None)
    }
}

async fn has_enabled_local_sandbox_pairing(
    config: &AppConfig,
    user_access_token: Option<&str>,
    project_ref: Option<&LocalConnectorProjectRef>,
) -> Result<bool, String> {
    Ok(
        find_enabled_local_sandbox_pairing(config, user_access_token, project_ref)
            .await?
            .is_some(),
    )
}

pub(super) async fn find_enabled_local_sandbox_pairing(
    config: &AppConfig,
    user_access_token: Option<&str>,
    project_ref: Option<&LocalConnectorProjectRef>,
) -> Result<Option<LocalConnectorSandboxPairing>, String> {
    let Some(token) = user_access_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let base = config
        .local_connector_service_base_url
        .trim()
        .trim_end_matches('/');
    if base.is_empty() {
        return Ok(None);
    }
    let client = build_http_client(HttpClientTimeouts::new(
        config.local_connector_service_request_timeout,
    ))
    .map_err(|err| format!("build local connector client failed: {err}"))?;
    let mut request = client
        .get(format!("{base}/api/local-connectors/sandbox-pairings"))
        .bearer_auth(token)
        .query(&[("active_only", "true")]);
    if let Some(project_ref) = project_ref {
        request = request.query(&[
            ("device_id", project_ref.device_id.as_str()),
            ("workspace_id", project_ref.workspace_id.as_str()),
        ]);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("query local connector sandbox pairings failed: {err}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        let status = response.status();
        let detail =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(format!(
            "query local connector sandbox pairings returned status={status} detail={}",
            truncate_detail(detail.as_str(), 1024)
        ));
    }
    let pairings = read_response_json_limited::<Vec<LocalConnectorSandboxPairing>>(
        response,
        JSON_BODY_LIMIT_BYTES,
    )
    .await
    .map_err(|err| format!("parse local connector sandbox pairings failed: {err}"))?;
    Ok(pairings.into_iter().find(|pairing| {
        if !pairing.enabled {
            return false;
        }
        if !local_sandbox_pairing_is_ready(pairing) {
            return false;
        }
        if let Some(project_ref) = project_ref {
            pairing.device_id == project_ref.device_id
                && pairing.workspace_id == project_ref.workspace_id
        } else {
            true
        }
    }))
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalConnectorSandboxPairing {
    #[serde(default)]
    pub(super) id: Option<String>,
    pub(super) device_id: String,
    pub(super) workspace_id: String,
    pub(super) enabled: bool,
    pub(super) sandbox_readiness: Option<String>,
    #[serde(default)]
    pub(super) facade_base_url: Option<String>,
}

fn local_sandbox_pairing_is_ready(pairing: &LocalConnectorSandboxPairing) -> bool {
    pairing
        .sandbox_readiness
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.eq_ignore_ascii_case("ready"))
        .unwrap_or(false)
}

fn not_runnable(message: impl Into<String>) -> StopDecision {
    let message = message.into();
    StopDecision {
        status: ProjectRuntimeEnvironmentStatus::NotRunnable,
        summary: message.clone(),
        not_runnable_reason: Some(message),
        last_error: None,
    }
}

fn waiting_for_local_sandbox(message: impl Into<String>) -> StopDecision {
    StopDecision {
        status: ProjectRuntimeEnvironmentStatus::Pending,
        summary: message.into(),
        not_runnable_reason: None,
        last_error: None,
    }
}

fn failed_stop(summary: impl Into<String>, last_error: impl Into<String>) -> StopDecision {
    StopDecision {
        status: ProjectRuntimeEnvironmentStatus::Failed,
        summary: summary.into(),
        not_runnable_reason: None,
        last_error: Some(last_error.into()),
    }
}

fn truncate_detail(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max_chars {
            output.push_str("...<truncated>");
            break;
        }
        output.push(ch);
    }
    output
}

pub(super) fn provider_label(provider: RuntimeEnvironmentProvider) -> &'static str {
    match provider {
        RuntimeEnvironmentProvider::None => "none",
        RuntimeEnvironmentProvider::LocalConnector => "Local Connector",
        RuntimeEnvironmentProvider::Harness => "Harness",
        RuntimeEnvironmentProvider::CloudSandboxManager => "Cloud Sandbox Manager",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_project_created_empty_still_inspects_current_harness_repository() {
        let project = cloud_project(Some("repo"));
        assert!(matches!(
            resolve_cloud_plan(&project),
            RuntimeEnvironmentDecision::Ready(RuntimeEnvironmentPlan {
                file_provider: RuntimeEnvironmentProvider::Harness,
                sandbox_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
            })
        ));
    }

    #[test]
    fn cloud_project_without_harness_repository_remains_not_runnable() {
        let project = cloud_project(None);
        assert!(matches!(
            resolve_cloud_plan(&project),
            RuntimeEnvironmentDecision::Stop(StopDecision {
                status: ProjectRuntimeEnvironmentStatus::NotRunnable,
                ..
            })
        ));
    }

    #[test]
    fn local_sandbox_pairing_requires_explicit_ready_state() {
        let mut pairing = LocalConnectorSandboxPairing {
            id: Some("pairing-1".to_string()),
            device_id: "device-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            enabled: true,
            sandbox_readiness: None,
            facade_base_url: None,
        };
        assert!(!local_sandbox_pairing_is_ready(&pairing));
        pairing.sandbox_readiness = Some("ready".to_string());
        assert!(local_sandbox_pairing_is_ready(&pairing));
    }

    fn cloud_project(harness_repo_identifier: Option<&str>) -> ProjectRecord {
        ProjectRecord {
            id: "project-1".to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: None,
            owner_display_name: None,
            name: "Example".to_string(),
            root_path: None,
            git_url: None,
            source_type: ProjectSourceType::Cloud,
            execution_plane: crate::models::ProjectExecutionPlane::Cloud,
            cloud_import_source: crate::models::CloudImportSource::Empty,
            import_status: ProjectImportStatus::Ready,
            source_git_url: None,
            harness_space_identifier: Some("space".to_string()),
            harness_repo_identifier: harness_repo_identifier.map(ToOwned::to_owned),
            harness_repo_path: Some("repo-path".to_string()),
            harness_git_url: None,
            harness_git_ssh_url: None,
            harness_default_branch: Some("main".to_string()),
            harness_provision_status: None,
            harness_provision_error: None,
            harness_provisioned_at: None,
            import_error: None,
            import_started_at: None,
            import_finished_at: None,
            description: None,
            status: crate::models::ProjectStatus::Active,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }
}
