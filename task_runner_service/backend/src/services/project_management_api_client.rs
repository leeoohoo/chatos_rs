// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;

use crate::auth;
use crate::config::AppConfig;
use crate::http_body::{
    read_response_json_limited, read_response_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use crate::models::{
    ChatosProjectImportRequest, CreateTaskProjectRequest, TaskProjectRecord, TaskProjectStatus,
    UpdateTaskProjectRequest,
};

#[derive(Debug, Clone, Deserialize)]
struct ProjectServiceProjectRecord {
    id: String,
    owner_user_id: Option<String>,
    owner_username: Option<String>,
    owner_display_name: Option<String>,
    name: String,
    #[serde(default)]
    root_path: Option<String>,
    #[serde(default)]
    git_url: Option<String>,
    #[serde(default)]
    source_type: Option<String>,
    #[serde(default)]
    cloud_import_source: Option<String>,
    #[serde(default)]
    import_status: Option<String>,
    #[serde(default)]
    source_git_url: Option<String>,
    #[serde(default)]
    harness_space_identifier: Option<String>,
    #[serde(default)]
    harness_repo_identifier: Option<String>,
    #[serde(default)]
    harness_repo_path: Option<String>,
    #[serde(default)]
    harness_git_url: Option<String>,
    #[serde(default)]
    harness_git_ssh_url: Option<String>,
    #[serde(default)]
    harness_default_branch: Option<String>,
    #[serde(default)]
    harness_provision_status: Option<String>,
    #[serde(default)]
    harness_provision_error: Option<String>,
    #[serde(default)]
    harness_provisioned_at: Option<String>,
    #[serde(default)]
    description: Option<String>,
    status: TaskProjectStatus,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectHarnessGitAccess {
    pub project_id: String,
    pub repo_path: String,
    pub git_url: String,
    pub default_branch: String,
    pub space_identifier: String,
    pub access_username: String,
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
struct ProjectRuntimeEnvironmentResponse {
    environment: ProjectRuntimeEnvironmentSettings,
}

#[derive(Debug, Deserialize)]
struct ProjectRuntimeEnvironmentSettings {
    sandbox_enabled: bool,
}

pub async fn get_project_from_project_service(
    config: &AppConfig,
    project_id: &str,
) -> Result<Option<TaskProjectRecord>, String> {
    let Some(base_url) = project_service_base_url(config) else {
        return Ok(None);
    };
    let client = project_service_client(config)?;

    let project = if let Some(access_token) = auth::get_current_access_token() {
        get_project_with_access_token(&client, base_url, access_token.as_str(), project_id).await?
    } else {
        let Some(sync_secret) = config
            .project_service_sync_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(
                "project service is configured but no access token or sync secret is available"
                    .to_string(),
            );
        };
        get_project_with_sync_secret(&client, base_url, sync_secret, project_id).await?
    };

    Ok(project.map(Into::into))
}

pub fn project_service_enabled(config: &AppConfig) -> bool {
    project_service_base_url(config).is_some()
}

pub async fn list_projects_for_user(
    config: &AppConfig,
    status: Option<TaskProjectStatus>,
) -> Result<Vec<TaskProjectRecord>, String> {
    let base_url = required_project_service_base_url(config)?;
    let access_token = required_access_token()?;
    let client = project_service_client(config)?;
    let status = status.map(|status| status.as_str().to_string());
    let projects = list_projects_with_access_token(
        &client,
        base_url,
        access_token.as_str(),
        status.as_deref(),
    )
    .await?;
    Ok(projects.into_iter().map(Into::into).collect())
}

pub async fn get_project_for_user(
    config: &AppConfig,
    project_id: &str,
) -> Result<Option<TaskProjectRecord>, String> {
    let base_url = required_project_service_base_url(config)?;
    let access_token = required_access_token()?;
    let client = project_service_client(config)?;
    get_project_with_access_token(&client, base_url, access_token.as_str(), project_id)
        .await
        .map(|project| project.map(Into::into))
}

pub async fn create_project(
    config: &AppConfig,
    input: &CreateTaskProjectRequest,
) -> Result<TaskProjectRecord, String> {
    let base_url = required_project_service_base_url(config)?;
    let access_token = required_access_token()?;
    let endpoint = format!("{}/api/projects", base_url.trim().trim_end_matches('/'));
    send_json::<ProjectServiceProjectRecord>(
        project_service_client(config)?
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .json(input),
    )
    .await
    .map(Into::into)
}

pub async fn update_project(
    config: &AppConfig,
    project_id: &str,
    input: &UpdateTaskProjectRequest,
) -> Result<Option<TaskProjectRecord>, String> {
    let base_url = required_project_service_base_url(config)?;
    let access_token = required_access_token()?;
    let endpoint = format!(
        "{}/api/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json::<ProjectServiceProjectRecord>(
        project_service_client(config)?
            .patch(endpoint)
            .bearer_auth(access_token.trim())
            .json(input),
    )
    .await
    .map(|project| project.map(Into::into))
}

pub async fn archive_project(
    config: &AppConfig,
    project_id: &str,
) -> Result<Option<TaskProjectRecord>, String> {
    let base_url = required_project_service_base_url(config)?;
    let access_token = required_access_token()?;
    let endpoint = format!(
        "{}/api/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json::<ProjectServiceProjectRecord>(
        project_service_client(config)?
            .delete(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
    .map(|project| project.map(Into::into))
}

pub async fn sync_list_projects(
    config: &AppConfig,
    status: Option<TaskProjectStatus>,
) -> Result<Vec<TaskProjectRecord>, String> {
    let base_url = required_project_service_base_url(config)?;
    let sync_secret = required_sync_secret(config)?;
    let client = project_service_client(config)?;
    let status = status.map(|status| status.as_str().to_string());
    let projects =
        list_projects_with_sync_secret(&client, base_url, sync_secret, status.as_deref()).await?;
    Ok(projects.into_iter().map(Into::into).collect())
}

pub async fn sync_get_project(
    config: &AppConfig,
    project_id: &str,
) -> Result<Option<TaskProjectRecord>, String> {
    let base_url = required_project_service_base_url(config)?;
    let sync_secret = required_sync_secret(config)?;
    let client = project_service_client(config)?;
    get_project_with_sync_secret(&client, base_url, sync_secret, project_id)
        .await
        .map(|project| project.map(Into::into))
}

pub async fn get_project_harness_git_access(
    config: &AppConfig,
    project_id: &str,
) -> Result<ProjectHarnessGitAccess, String> {
    let base_url = required_project_service_base_url(config)?;
    let sync_secret = required_sync_secret(config)?;
    let endpoint = format!(
        "{}/api/chatos-sync/projects/{}/harness/git-access",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_json(
        project_service_client(config)?
            .get(endpoint)
            .header("X-Project-Service-Sync-Secret", sync_secret.trim()),
    )
    .await
}

pub async fn get_project_sandbox_enabled(
    config: &AppConfig,
    project_id: &str,
) -> Result<bool, String> {
    let base_url = required_project_service_base_url(config)?;
    let sync_secret = required_sync_secret(config)?;
    let endpoint = format!(
        "{}/api/chatos-sync/projects/{}/runtime-environment",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    let response = send_json::<ProjectRuntimeEnvironmentResponse>(
        project_service_client(config)?
            .get(endpoint)
            .header("X-Project-Service-Sync-Secret", sync_secret.trim()),
    )
    .await?;
    Ok(response.environment.sandbox_enabled)
}

pub async fn import_project(
    config: &AppConfig,
    input: &ChatosProjectImportRequest,
) -> Result<TaskProjectRecord, String> {
    let base_url = required_project_service_base_url(config)?;
    let sync_secret = required_sync_secret(config)?;
    let endpoint = format!(
        "{}/api/chatos-sync/projects",
        base_url.trim().trim_end_matches('/')
    );
    send_json::<ProjectServiceProjectRecord>(
        project_service_client(config)?
            .post(endpoint)
            .header("X-Project-Service-Sync-Secret", sync_secret.trim())
            .json(input),
    )
    .await
    .map(Into::into)
}

fn project_service_base_url(config: &AppConfig) -> Option<&str> {
    config
        .project_service_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required_project_service_base_url(config: &AppConfig) -> Result<&str, String> {
    project_service_base_url(config)
        .ok_or_else(|| "project service base url is not configured".to_string())
}

fn required_access_token() -> Result<String, String> {
    auth::get_current_access_token()
        .ok_or_else(|| "current access token is required for project service request".to_string())
}

fn required_sync_secret(config: &AppConfig) -> Result<&str, String> {
    config
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "project service sync secret is not configured".to_string())
}

fn project_service_client(config: &AppConfig) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(config.project_service_request_timeout)
        .build()
        .map_err(|err| err.to_string())
}

trait TaskProjectStatusExt {
    fn as_str(self) -> &'static str;
}

impl TaskProjectStatusExt for TaskProjectStatus {
    fn as_str(self) -> &'static str {
        match self {
            TaskProjectStatus::Active => "active",
            TaskProjectStatus::Archived => "archived",
        }
    }
}

async fn get_project_with_access_token(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &str,
    project_id: &str,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(client.get(endpoint).bearer_auth(access_token.trim())).await
}

async fn list_projects_with_access_token(
    client: &reqwest::Client,
    base_url: &str,
    access_token: &str,
    status: Option<&str>,
) -> Result<Vec<ProjectServiceProjectRecord>, String> {
    let endpoint = format!("{}/api/projects", base_url.trim().trim_end_matches('/'));
    let mut request = client.get(endpoint).bearer_auth(access_token.trim());
    if let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.query(&[("status", status)]);
    }
    send_json(request).await
}

async fn get_project_with_sync_secret(
    client: &reqwest::Client,
    base_url: &str,
    sync_secret: &str,
    project_id: &str,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/chatos-sync/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(
        client
            .get(endpoint)
            .header("X-Project-Service-Sync-Secret", sync_secret.trim()),
    )
    .await
}

async fn list_projects_with_sync_secret(
    client: &reqwest::Client,
    base_url: &str,
    sync_secret: &str,
    status: Option<&str>,
) -> Result<Vec<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/chatos-sync/projects",
        base_url.trim().trim_end_matches('/')
    );
    let mut request = client
        .get(endpoint)
        .header("X-Project-Service-Sync-Secret", sync_secret.trim());
    if let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.query(&[("status", status)]);
    }
    send_json(request).await
}

async fn send_json<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<T, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(format!("Project service request failed: {status} {body}"));
    }
    read_response_json_limited::<T>(response, JSON_BODY_LIMIT_BYTES).await
}

async fn send_optional_json<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<Option<T>, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        let body =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(format!("Project service request failed: {status} {body}"));
    }
    read_response_json_limited::<T>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map(Some)
}

impl From<ProjectServiceProjectRecord> for TaskProjectRecord {
    fn from(value: ProjectServiceProjectRecord) -> Self {
        Self {
            id: value.id,
            owner_user_id: value.owner_user_id,
            owner_username: value.owner_username,
            owner_display_name: value.owner_display_name,
            name: value.name,
            root_path: value.root_path,
            git_url: value.git_url,
            source_type: value.source_type,
            cloud_import_source: value.cloud_import_source,
            import_status: value.import_status,
            source_git_url: value.source_git_url,
            harness_space_identifier: value.harness_space_identifier,
            harness_repo_identifier: value.harness_repo_identifier,
            harness_repo_path: value.harness_repo_path,
            harness_git_url: value.harness_git_url,
            harness_git_ssh_url: value.harness_git_ssh_url,
            harness_default_branch: value.harness_default_branch,
            harness_provision_status: value.harness_provision_status,
            harness_provision_error: value.harness_provision_error,
            harness_provisioned_at: value.harness_provisioned_at,
            description: value.description,
            status: value.status,
            created_at: value.created_at,
            updated_at: value.updated_at,
            archived_at: value.archived_at,
        }
    }
}
