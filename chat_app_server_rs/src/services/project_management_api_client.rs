use serde::{Deserialize, Serialize};
use serde_json::Value;

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

pub async fn list_project_service_requirements(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    include_archived: bool,
) -> Result<Vec<Value>, String> {
    let endpoint = format!(
        "{}/api/projects/{}/requirements",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    let request = reqwest::Client::new()
        .get(endpoint)
        .bearer_auth(access_token.trim())
        .query(&[("include_archived", include_archived)]);
    send_json(request).await
}

pub async fn list_project_service_work_items(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    include_archived: bool,
) -> Result<Vec<Value>, String> {
    let endpoint = format!(
        "{}/api/projects/{}/work-items",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    let request = reqwest::Client::new()
        .get(endpoint)
        .bearer_auth(access_token.trim())
        .query(&[("include_archived", include_archived)]);
    send_json(request).await
}

pub async fn get_project_service_dependency_graph(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    include_archived: bool,
) -> Result<Value, String> {
    let endpoint = format!(
        "{}/api/projects/{}/dependency-graph",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    let request = reqwest::Client::new()
        .get(endpoint)
        .bearer_auth(access_token.trim())
        .query(&[("include_archived", include_archived)]);
    send_json(request).await
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
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Project service request failed: {status} {body}"));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
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
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Project service request failed: {status} {body}"));
    }
    response
        .json::<T>()
        .await
        .map(Some)
        .map_err(|err| err.to_string())
}
