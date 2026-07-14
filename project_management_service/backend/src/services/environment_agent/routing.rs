// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

use reqwest::StatusCode;
use serde::Deserialize;

use crate::config::AppConfig;
use crate::models::{
    CloudImportSource, ProjectImportStatus, ProjectRecord, ProjectRuntimeEnvironmentStatus,
    ProjectSourceType, RuntimeEnvironmentProvider,
};

use super::LOCAL_CONNECTOR_ROOT_PREFIX;

#[derive(Debug)]
pub(super) enum RoutingDecision {
    Ready(RoutingPlan),
    Stop(StopDecision),
}

#[derive(Debug)]
pub(super) struct RoutingPlan {
    pub(super) file_provider: RuntimeEnvironmentProvider,
    pub(super) sandbox_provider: RuntimeEnvironmentProvider,
    pub(super) summary: String,
}

#[derive(Debug)]
pub(super) struct StopDecision {
    pub(super) status: ProjectRuntimeEnvironmentStatus,
    pub(super) summary: String,
    pub(super) not_runnable_reason: Option<String>,
    pub(super) last_error: Option<String>,
}

pub(super) async fn resolve_runtime_environment_routing(
    project: &ProjectRecord,
    config: &AppConfig,
    user_access_token: Option<&str>,
) -> RoutingDecision {
    match project.source_type {
        ProjectSourceType::Cloud => resolve_cloud_routing(project, config, user_access_token).await,
        ProjectSourceType::Local | ProjectSourceType::LocalConnector => {
            resolve_local_routing(project, config, user_access_token).await
        }
    }
}

async fn resolve_cloud_routing(
    project: &ProjectRecord,
    config: &AppConfig,
    user_access_token: Option<&str>,
) -> RoutingDecision {
    if project.cloud_import_source == CloudImportSource::Empty {
        return RoutingDecision::Stop(not_runnable(
            "云端项目当前为空，暂无可分析的项目文件。请先上传代码或导入仓库后再初始化运行环境。",
        ));
    }
    match project.import_status {
        ProjectImportStatus::Pending | ProjectImportStatus::Importing => {
            return RoutingDecision::Stop(StopDecision {
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
            return RoutingDecision::Stop(not_runnable(format!(
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
        return RoutingDecision::Stop(not_runnable(
            "云端项目缺少 Harness 仓库信息，无法通过 Harness MCP 读取项目文件。",
        ));
    }
    let sandbox_provider = match choose_sandbox_provider(config, user_access_token, None).await {
        Ok(provider) => provider,
        Err(err) => {
            return RoutingDecision::Stop(failed_stop(
                "检查本地沙箱可用性失败，无法确定运行环境镜像 MCP。",
                err,
            ));
        }
    };
    RoutingDecision::Ready(RoutingPlan {
        file_provider: RuntimeEnvironmentProvider::Harness,
        sandbox_provider,
        summary: "云端项目将通过 Harness MCP 读取文件，并按本地沙箱可用性选择沙箱镜像 MCP。"
            .to_string(),
    })
}

async fn resolve_local_routing(
    project: &ProjectRecord,
    config: &AppConfig,
    user_access_token: Option<&str>,
) -> RoutingDecision {
    let Some(root_path) = project
        .root_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return RoutingDecision::Stop(not_runnable("本地项目缺少根目录，无法读取项目文件。"));
    };
    let local_connector_ref = parse_local_connector_project_root(root_path);
    if root_path.starts_with(LOCAL_CONNECTOR_ROOT_PREFIX) && local_connector_ref.is_none() {
        return RoutingDecision::Stop(not_runnable(
            "本地项目的 Local Connector 根目录格式不正确，无法读取项目文件。",
        ));
    }
    if local_connector_ref.is_none() {
        let path = Path::new(root_path);
        if !path.exists() {
            return RoutingDecision::Stop(not_runnable("本地项目根目录不存在，无法读取项目文件。"));
        }
        if !path.is_dir() {
            return RoutingDecision::Stop(not_runnable(
                "本地项目根目录不是目录，无法读取项目文件。",
            ));
        }
        if directory_is_effectively_empty(path) {
            return RoutingDecision::Stop(not_runnable(
                "本地项目根目录为空，暂无可分析的项目文件。",
            ));
        }
    }

    let sandbox_provider = match choose_sandbox_provider(
        config,
        user_access_token,
        local_connector_ref.as_ref(),
    )
    .await
    {
        Ok(provider) => provider,
        Err(err) => {
            return RoutingDecision::Stop(failed_stop(
                "检查本地沙箱可用性失败，无法确定运行环境镜像 MCP。",
                err,
            ));
        }
    };
    RoutingDecision::Ready(RoutingPlan {
        file_provider: RuntimeEnvironmentProvider::LocalConnector,
        sandbox_provider,
        summary:
            "本地项目将通过 Local Connector 文件 MCP 读取文件，并按本地沙箱可用性选择沙箱镜像 MCP。"
                .to_string(),
    })
}

async fn choose_sandbox_provider(
    config: &AppConfig,
    user_access_token: Option<&str>,
    project_ref: Option<&LocalConnectorProjectRef>,
) -> Result<RuntimeEnvironmentProvider, String> {
    if has_enabled_local_sandbox_pairing(config, user_access_token, project_ref).await? {
        Ok(RuntimeEnvironmentProvider::LocalConnector)
    } else {
        Ok(RuntimeEnvironmentProvider::CloudSandboxManager)
    }
}

#[derive(Debug, Clone)]
pub(super) struct LocalConnectorProjectRef {
    pub(super) device_id: String,
    pub(super) workspace_id: String,
    pub(super) relative_path: Option<String>,
}

pub(super) fn parse_local_connector_project_root(
    project_root: &str,
) -> Option<LocalConnectorProjectRef> {
    let rest = project_root
        .trim()
        .strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX)?;
    let mut parts = rest.splitn(3, '/');
    let device_id = normalize_non_empty(parts.next())?;
    let workspace_id = normalize_non_empty(parts.next())?;
    let relative_path = match parts.next() {
        Some(path) => Some(decode_local_connector_relative_path(path)?),
        None => None,
    };
    Some(LocalConnectorProjectRef {
        device_id,
        workspace_id,
        relative_path,
    })
}

fn decode_local_connector_relative_path(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.split('/').filter(|part| !part.trim().is_empty()) {
        let decoded = urlencoding::decode(part).ok()?.into_owned();
        parts.push(decoded);
    }
    let joined = parts.join("/");
    normalize_local_relative_path(joined.as_str()).filter(|path| local_relative_path_is_safe(path))
}

fn normalize_local_relative_path(value: &str) -> Option<String> {
    let value = value.trim().replace('\\', "/");
    let value = value.trim_matches('/');
    if value.is_empty() || value == "." {
        return None;
    }
    let parts = value
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn local_relative_path_is_safe(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && path.split('/').all(|part| {
            let part = part.trim();
            !part.is_empty() && part != "." && part != ".."
        })
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
    let client = reqwest::Client::builder()
        .timeout(config.local_connector_service_request_timeout)
        .build()
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
        let detail = response.text().await.unwrap_or_default();
        return Err(format!(
            "query local connector sandbox pairings returned status={status} detail={}",
            truncate_detail(detail.as_str(), 1024)
        ));
    }
    let pairings = response
        .json::<Vec<LocalConnectorSandboxPairing>>()
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
        .unwrap_or(true)
}

fn directory_is_effectively_empty(path: &Path) -> bool {
    let Ok(mut entries) = fs::read_dir(path) else {
        return false;
    };
    entries.all(|entry| {
        entry
            .ok()
            .and_then(|entry| entry.file_name().into_string().ok())
            .is_some_and(|name| matches!(name.as_str(), ".git" | ".DS_Store"))
    })
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

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn provider_label(provider: RuntimeEnvironmentProvider) -> &'static str {
    match provider {
        RuntimeEnvironmentProvider::None => "none",
        RuntimeEnvironmentProvider::LocalConnector => "Local Connector",
        RuntimeEnvironmentProvider::Harness => "Harness",
        RuntimeEnvironmentProvider::CloudSandboxManager => "Cloud Sandbox Manager",
    }
}
