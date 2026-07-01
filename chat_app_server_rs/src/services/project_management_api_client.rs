// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use bytes::BytesMut;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const PROJECT_SERVICE_DEFAULT_RESPONSE_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const PROJECT_SERVICE_PLAN_RESPONSE_LIMIT_BYTES: usize = 8 * 1024 * 1024;
const PROJECT_SERVICE_WORK_ITEMS_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const PROJECT_SERVICE_DOCUMENTS_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const PROJECT_SERVICE_ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectServiceProjectRecord {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct CreateProjectServiceProjectRequest {
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateProjectServiceProjectRequest {
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
}

pub async fn list_project_service_projects(
    base_url: &str,
    access_token: &str,
    status: Option<&str>,
) -> Result<Vec<ProjectServiceProjectRecord>, String> {
    let endpoint = format!("{}/api/projects", base_url.trim().trim_end_matches('/'));
    let mut request = reqwest::Client::new()
        .get(endpoint)
        .bearer_auth(access_token.trim());
    if let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.query(&[("status", status)]);
    }
    send_json(request).await
}

pub async fn get_project_service_project(
    base_url: &str,
    access_token: &str,
    project_id: &str,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(
        reqwest::Client::new()
            .get(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
}

pub async fn create_project_service_project(
    base_url: &str,
    access_token: &str,
    request: &CreateProjectServiceProjectRequest,
) -> Result<ProjectServiceProjectRecord, String> {
    let endpoint = format!("{}/api/projects", base_url.trim().trim_end_matches('/'));
    send_json(
        reqwest::Client::new()
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

pub async fn update_project_service_project(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    request: &UpdateProjectServiceProjectRequest,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(
        reqwest::Client::new()
            .patch(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

pub async fn archive_project_service_project(
    base_url: &str,
    access_token: &str,
    project_id: &str,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(
        reqwest::Client::new()
            .delete(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
}

pub async fn get_project_service_plan(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    include_archived: bool,
) -> Result<Value, String> {
    get_project_service_plan_with_options(
        base_url,
        access_token,
        project_id,
        ProjectServicePlanOptions {
            include_archived,
            include_work_items: None,
        },
    )
    .await
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectServicePlanOptions {
    pub include_archived: bool,
    pub include_work_items: Option<bool>,
}

pub async fn get_project_service_plan_with_options(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    options: ProjectServicePlanOptions,
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/api/projects/{}/plan",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    let mut request = reqwest::Client::new()
        .get(endpoint)
        .bearer_auth(access_token.trim())
        .query(&[("include_archived", options.include_archived)]);
    if let Some(include_work_items) = options.include_work_items {
        request = request.query(&[("include_work_items", include_work_items)]);
    }
    send_json_with_limit(request, PROJECT_SERVICE_PLAN_RESPONSE_LIMIT_BYTES).await
}

pub async fn list_project_service_requirement_work_items(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    requirement_id: &str,
    include_archived: bool,
    include_dependency_graph: bool,
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/api/projects/{}/requirements/{}/work-items",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim()),
        urlencoding::encode(requirement_id.trim())
    );
    let request = reqwest::Client::new()
        .get(endpoint)
        .bearer_auth(access_token.trim())
        .query(&[
            ("include_archived", include_archived),
            ("include_dependency_graph", include_dependency_graph),
        ]);
    send_json_with_limit(request, PROJECT_SERVICE_WORK_ITEMS_RESPONSE_LIMIT_BYTES).await
}

pub async fn list_project_service_requirement_documents(
    base_url: &str,
    access_token: &str,
    requirement_id: &str,
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/api/requirements/{}/documents",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(requirement_id.trim())
    );
    send_json_with_limit(
        reqwest::Client::new()
            .get(endpoint)
            .bearer_auth(access_token.trim()),
        PROJECT_SERVICE_DOCUMENTS_RESPONSE_LIMIT_BYTES,
    )
    .await
}

pub async fn list_work_item_task_runner_links(
    base_url: &str,
    access_token: &str,
    work_item_id: &str,
) -> Result<Vec<Value>, String> {
    let endpoint = format!(
        "{}/api/work-items/{}/task-runner-links",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(work_item_id.trim())
    );
    send_json(
        reqwest::Client::new()
            .get(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
}

#[derive(Debug, Default, Serialize)]
pub struct LinkTaskRunnerTaskRequest {
    pub task_runner_task_id: String,
    pub task_runner_run_id: Option<String>,
    pub link_type: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub task_runner_status: Option<String>,
    pub last_callback_event: Option<String>,
    pub last_callback_at: Option<String>,
    pub last_error_message: Option<String>,
}

pub async fn link_work_item_task_runner_task(
    base_url: &str,
    access_token: &str,
    work_item_id: &str,
    request: &LinkTaskRunnerTaskRequest,
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/api/work-items/{}/task-runner-links",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(work_item_id.trim())
    );
    send_json(
        reqwest::Client::new()
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

#[derive(Debug, Default, Serialize)]
pub struct SyncTaskRunnerWorkItemStatusRequest {
    pub task_runner_task_id: String,
    pub task_runner_run_id: Option<String>,
    pub task_runner_status: Option<String>,
    pub last_callback_event: Option<String>,
    pub last_callback_at: Option<String>,
    pub last_error_message: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct SyncRequirementExecutionStateRequest {
    pub requirement_status: Option<String>,
    pub work_item_ids: Vec<String>,
    pub work_item_status: Option<String>,
    pub skip_done_work_items: bool,
}

pub async fn sync_work_item_task_runner_status(
    base_url: &str,
    sync_secret: &str,
    work_item_id: &str,
    request: &SyncTaskRunnerWorkItemStatusRequest,
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/api/chatos-sync/work-items/{}/task-runner-status",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(work_item_id.trim())
    );
    send_json(
        reqwest::Client::new()
            .post(endpoint)
            .header("X-Project-Service-Sync-Secret", sync_secret.trim())
            .json(request),
    )
    .await
}

pub async fn sync_requirement_execution_state(
    base_url: &str,
    sync_secret: &str,
    requirement_id: &str,
    request: &SyncRequirementExecutionStateRequest,
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/api/chatos-sync/requirements/{}/execution-state",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(requirement_id.trim())
    );
    send_json(
        reqwest::Client::new()
            .post(endpoint)
            .header("X-Project-Service-Sync-Secret", sync_secret.trim())
            .json(request),
    )
    .await
}

pub async fn sync_list_project_service_projects(
    base_url: &str,
    sync_secret: &str,
    status: Option<&str>,
) -> Result<Vec<ProjectServiceProjectRecord>, String> {
    let endpoint = format!(
        "{}/api/chatos-sync/projects",
        base_url.trim().trim_end_matches('/')
    );
    let mut request = reqwest::Client::new()
        .get(endpoint)
        .header("X-Project-Service-Sync-Secret", sync_secret.trim());
    if let Some(status) = status.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.query(&[("status", status)]);
    }
    send_json(request).await
}

pub async fn sync_get_project_service_project(
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
        reqwest::Client::new()
            .get(endpoint)
            .header("X-Project-Service-Sync-Secret", sync_secret.trim()),
    )
    .await
}

async fn send_json<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<T, String> {
    send_json_with_limit(request, PROJECT_SERVICE_DEFAULT_RESPONSE_LIMIT_BYTES).await
}

async fn send_json_with_limit<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
    response_limit_bytes: usize,
) -> Result<T, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_project_service_body_limited(response, PROJECT_SERVICE_ERROR_BODY_PREVIEW_BYTES)
                .await
                .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
                .unwrap_or_default();
        return Err(format!("Project service request failed: {status} {body}"));
    }
    let body = read_project_service_body_limited(response, response_limit_bytes).await?;
    serde_json::from_slice::<T>(body.as_ref()).map_err(|err| err.to_string())
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
            read_project_service_body_limited(response, PROJECT_SERVICE_ERROR_BODY_PREVIEW_BYTES)
                .await
                .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
                .unwrap_or_default();
        return Err(format!("Project service request failed: {status} {body}"));
    }
    let body =
        read_project_service_body_limited(response, PROJECT_SERVICE_DEFAULT_RESPONSE_LIMIT_BYTES)
            .await?;
    serde_json::from_slice::<T>(body.as_ref())
        .map(Some)
        .map_err(|err| err.to_string())
}

async fn read_project_service_body_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<bytes::Bytes, String> {
    if let Some(content_length) = response.content_length() {
        ensure_project_service_body_within_limit(content_length as usize, limit_bytes)?;
    }

    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let next_len = body.len().saturating_add(chunk.len());
        ensure_project_service_body_within_limit(next_len, limit_bytes)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body.freeze())
}

fn ensure_project_service_body_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "Project service response exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_project_service_body_within_limit;

    #[test]
    fn project_service_body_limit_accepts_boundary_size() {
        assert!(ensure_project_service_body_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn project_service_body_limit_rejects_oversized_body() {
        let err = ensure_project_service_body_within_limit(1025, 1024)
            .expect_err("oversized body should fail");

        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
