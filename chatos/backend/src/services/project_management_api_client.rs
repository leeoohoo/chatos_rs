// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

const PROJECT_SERVICE_HARNESS_FILE_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const PROJECT_SERVICE_PLAN_RESPONSE_LIMIT_BYTES: usize = 8 * 1024 * 1024;
const PROJECT_SERVICE_WORK_ITEMS_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const PROJECT_SERVICE_DOCUMENTS_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;

mod internal_auth;
mod transport;

pub(crate) use self::internal_auth::{insert_project_service_internal_headers, PROJECT_MCP_SCOPE};
use self::internal_auth::{
    signed_project_service_request, PROJECT_HARNESS_SCOPE, PROJECT_READ_SCOPE, PROJECT_SYNC_SCOPE,
};
use self::transport::{
    resolve_project_service_base_url, send_json, send_json_with_limit, send_optional_json,
};

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectServiceProjectRecord {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub name: String,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub source_type: Option<String>,
    pub execution_plane: Option<String>,
    pub cloud_import_source: Option<String>,
    pub import_status: Option<String>,
    pub source_git_url: Option<String>,
    pub harness_space_identifier: Option<String>,
    pub harness_repo_identifier: Option<String>,
    pub harness_repo_path: Option<String>,
    pub harness_git_url: Option<String>,
    pub harness_git_ssh_url: Option<String>,
    pub import_error: Option<String>,
    pub import_started_at: Option<String>,
    pub import_finished_at: Option<String>,
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

pub struct CreateCloudProjectServiceProjectRequest {
    pub name: String,
    pub git_url: Option<String>,
    pub description: Option<String>,
    pub zip: Option<(String, Vec<u8>)>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateProjectServiceProjectRequest {
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub git_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateProjectRuntimeEnvironmentSettingsRequest {
    pub sandbox_enabled: Option<bool>,
}

pub async fn list_project_service_projects(
    base_url: &str,
    access_token: &str,
    status: Option<&str>,
) -> Result<Vec<ProjectServiceProjectRecord>, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
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
    let base_url = resolve_project_service_base_url(base_url).await;
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
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!("{}/api/projects", base_url.trim().trim_end_matches('/'));
    send_json(
        reqwest::Client::new()
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

pub async fn create_cloud_project_service_project(
    base_url: &str,
    access_token: &str,
    request: &CreateCloudProjectServiceProjectRequest,
) -> Result<ProjectServiceProjectRecord, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/projects/cloud",
        base_url.trim().trim_end_matches('/')
    );
    let mut form = reqwest::multipart::Form::new().text("name", request.name.clone());
    if let Some(git_url) = request
        .git_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        form = form.text("git_url", git_url.to_string());
    }
    if let Some(description) = request
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        form = form.text("description", description.to_string());
    }
    if let Some((filename, bytes)) = request.zip.as_ref() {
        if !bytes.is_empty() {
            let part = reqwest::multipart::Part::bytes(bytes.clone())
                .file_name(filename.clone())
                .mime_str("application/zip")
                .map_err(|err| err.to_string())?;
            form = form.part("zip", part);
        }
    }
    send_json(
        reqwest::Client::new()
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .multipart(form),
    )
    .await
}

pub async fn update_project_service_project(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    request: &UpdateProjectServiceProjectRequest,
) -> Result<Option<ProjectServiceProjectRecord>, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
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
    let base_url = resolve_project_service_base_url(base_url).await;
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

pub async fn get_project_service_runtime_environment(
    base_url: &str,
    access_token: &str,
    project_id: &str,
) -> Result<Value, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/projects/{}/runtime-environment",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_json(
        reqwest::Client::new()
            .get(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
}

pub async fn update_project_service_runtime_environment_settings(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    request: &UpdateProjectRuntimeEnvironmentSettingsRequest,
) -> Result<Value, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/projects/{}/runtime-environment/settings",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_json(
        reqwest::Client::new()
            .put(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

pub async fn analyze_project_service_runtime_environment(
    base_url: &str,
    access_token: &str,
    project_id: &str,
) -> Result<Value, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/projects/{}/runtime-environment/analyze",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_json(
        reqwest::Client::new()
            .post(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
}

pub async fn generate_project_service_runtime_environment_image(
    base_url: &str,
    access_token: &str,
    project_id: &str,
    image_record_id: &str,
) -> Result<Value, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/projects/{}/runtime-environment/images/{}/generate",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim()),
        urlencoding::encode(image_record_id.trim()),
    );
    send_json(
        reqwest::Client::new()
            .post(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
}

pub async fn get_project_service_runtime_environment_progress(
    base_url: &str,
    access_token: &str,
    project_id: &str,
) -> Result<Value, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/projects/{}/runtime-environment/progress",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_json(
        reqwest::Client::new()
            .get(endpoint)
            .bearer_auth(access_token.trim()),
    )
    .await
}

pub async fn call_project_harness_tool(
    base_url: &str,
    sync_secret: &str,
    project_id: &str,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/chatos-sync/projects/{}/harness/mcp",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    let response: Value = send_json_with_limit(
        signed_project_service_request(
            reqwest::Client::new().post(endpoint),
            sync_secret,
            PROJECT_HARNESS_SCOPE,
        )?
        .header("X-Task-Runner-Project-Id", project_id.trim())
        .header(
            "X-Harness-Code-Enabled-Builtin-Kinds",
            "CodeMaintainerWrite",
        )
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": format!("chatos-fs-{}", uuid::Uuid::new_v4()),
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments,
            }
        })),
        PROJECT_SERVICE_HARNESS_FILE_RESPONSE_LIMIT_BYTES,
    )
    .await?;
    parse_harness_tool_response(response)
}

fn parse_harness_tool_response(response: Value) -> Result<Value, String> {
    if let Some(error) = response.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Harness MCP request failed");
        return Err(message.to_string());
    }
    let result = response
        .get("result")
        .ok_or_else(|| "Harness MCP response is missing result".to_string())?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        let message = result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or("Harness MCP tool failed");
        return Err(message.to_string());
    }
    if let Some(structured) = result.get("_structured_result") {
        return Ok(structured.clone());
    }
    let Some(text) = result.pointer("/content/0/text").and_then(Value::as_str) else {
        return Ok(result.clone());
    };
    serde_json::from_str(text).map_err(|err| format!("parse Harness MCP tool result failed: {err}"))
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
    let base_url = resolve_project_service_base_url(base_url).await;
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
    let base_url = resolve_project_service_base_url(base_url).await;
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
    let base_url = resolve_project_service_base_url(base_url).await;
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
    let base_url = resolve_project_service_base_url(base_url).await;
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
pub struct SyncTaskRunnerWorkItemStatusRequest {
    pub task_runner_task_id: String,
    pub task_runner_run_id: Option<String>,
    pub task_runner_status: Option<String>,
    pub execution_group_id: Option<String>,
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
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/chatos-sync/work-items/{}/task-runner-status",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(work_item_id.trim())
    );
    send_json(
        signed_project_service_request(
            reqwest::Client::new().post(endpoint),
            sync_secret,
            PROJECT_SYNC_SCOPE,
        )?
        .json(request),
    )
    .await
}

pub async fn sync_task_runner_task_status(
    base_url: &str,
    sync_secret: &str,
    task_runner_task_id: &str,
    request: &SyncTaskRunnerWorkItemStatusRequest,
) -> Result<Value, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/chatos-sync/task-runner/tasks/{}/status",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(task_runner_task_id.trim())
    );
    send_json(
        signed_project_service_request(
            reqwest::Client::new().post(endpoint),
            sync_secret,
            PROJECT_SYNC_SCOPE,
        )?
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
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/chatos-sync/requirements/{}/execution-state",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(requirement_id.trim())
    );
    send_json(
        signed_project_service_request(
            reqwest::Client::new().post(endpoint),
            sync_secret,
            PROJECT_SYNC_SCOPE,
        )?
        .json(request),
    )
    .await
}

pub async fn sync_list_project_service_projects(
    base_url: &str,
    sync_secret: &str,
    status: Option<&str>,
) -> Result<Vec<ProjectServiceProjectRecord>, String> {
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/chatos-sync/projects",
        base_url.trim().trim_end_matches('/')
    );
    let mut request = signed_project_service_request(
        reqwest::Client::new().get(endpoint),
        sync_secret,
        PROJECT_READ_SCOPE,
    )?;
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
    let base_url = resolve_project_service_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/chatos-sync/projects/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(project_id.trim())
    );
    send_optional_json(signed_project_service_request(
        reqwest::Client::new().get(endpoint),
        sync_secret,
        PROJECT_READ_SCOPE,
    )?)
    .await
}

#[cfg(test)]
mod tests {
    use super::parse_harness_tool_response;
    use serde_json::{json, Value};

    #[test]
    fn harness_tool_response_prefers_structured_result() {
        let parsed = parse_harness_tool_response(json!({
            "result": {
                "content": [{ "type": "text", "text": "ignored" }],
                "_structured_result": { "entries": [{ "path": "src" }] },
                "isError": false
            }
        }))
        .expect("parse Harness tool response");
        assert_eq!(
            parsed.pointer("/entries/0/path").and_then(Value::as_str),
            Some("src")
        );
    }
}
