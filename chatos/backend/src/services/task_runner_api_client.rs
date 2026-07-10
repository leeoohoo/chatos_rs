// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use bytes::BytesMut;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::OnceLock;

static TASK_RUNNER_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

const TASK_RUNNER_DEFAULT_RESPONSE_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const TASK_RUNNER_INTERNAL_RESPONSE_LIMIT_BYTES: usize = 8 * 1024 * 1024;
const TASK_RUNNER_ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;

mod types;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use types::TaskRunnerMcpConfigRequest;
pub use types::{
    CancelTaskRunnerPromptRequest, CancelTaskRunnerTaskRequest, CreateTaskRunnerTaskRequest,
    SubmitTaskRunnerPromptRequest, TaskRunnerExecutionOptions, TaskRunnerTaskRecord,
    TaskRunnerTaskScheduleRequest, UserServiceTaskRunnerExchange,
};

use types::{TaskRunnerCapabilityCatalog, UserServiceTaskRunnerTokenResponse};

pub async fn exchange_task_runner_token_via_user_service(
    request: &UserServiceTaskRunnerExchange,
) -> Result<String, String> {
    let base_url = resolve_user_service_base_url(request.base_url.as_str()).await;
    let endpoint = format!(
        "{}/api/token/exchange/task-runner",
        base_url.trim().trim_end_matches('/')
    );
    let payload: UserServiceTaskRunnerTokenResponse = send_task_runner_response_with_limit(
        task_runner_http_client()
            .post(endpoint)
            .bearer_auth(request.access_token.trim())
            .json(&serde_json::json!({
                "task_runner_agent_account_id": request.task_runner_agent_account_id,
                "contact_id": request.contact_id,
            })),
        TASK_RUNNER_DEFAULT_RESPONSE_LIMIT_BYTES,
        "User service task runner token exchange failed",
    )
    .await?;
    let token = payload.access_token.trim();
    if token.is_empty() {
        return Err("User service task runner token exchange returned empty token".to_string());
    }
    Ok(token.to_string())
}

pub async fn fetch_task_runner_execution_options(
    base_url: &str,
    access_token: &str,
) -> Result<TaskRunnerExecutionOptions, String> {
    let catalog: TaskRunnerCapabilityCatalog = task_runner_json(
        base_url,
        access_token,
        reqwest::Method::GET,
        "/api/tasks/capabilities/catalog",
        None::<&()>,
    )
    .await?;

    let mut builtin_tool_ids = BTreeSet::new();
    for item in catalog.selectable_builtin_mcps {
        if let Some(kind) = normalize_optional(Some(item.kind)) {
            builtin_tool_ids.insert(kind);
        }
        if let Some(config_id) = item
            .config_id
            .and_then(|value| normalize_optional(Some(value)))
        {
            builtin_tool_ids.insert(config_id);
        }
    }
    let external_tool_ids = catalog
        .selectable_external_mcps
        .into_iter()
        .filter_map(|item| normalize_optional(Some(item.id)))
        .collect::<BTreeSet<_>>();
    Ok(TaskRunnerExecutionOptions {
        builtin_tool_ids,
        external_tool_ids,
    })
}

pub async fn create_task_runner_task(
    base_url: &str,
    access_token: &str,
    user_access_token: Option<&str>,
    source_session_id: Option<&str>,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
    request: &CreateTaskRunnerTaskRequest,
) -> Result<TaskRunnerTaskRecord, String> {
    let mut builder =
        task_runner_request(base_url, access_token, reqwest::Method::POST, "/api/tasks")
            .await
            .json(request);
    if let Some(value) = normalize_optional(source_session_id.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-Session-Id", value);
    }
    if let Some(value) = normalize_optional(source_user_message_id.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-User-Message-Id", value);
    }
    if let Some(value) = normalize_optional(source_turn_id.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-Turn-Id", value);
    }
    if let Some(value) = normalize_optional(user_access_token.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-User-Authorization", format!("Bearer {value}"));
    }
    send_task_runner_response(builder).await
}

pub async fn get_task_runner_task(
    base_url: &str,
    access_token: &str,
    task_id: &str,
) -> Result<TaskRunnerTaskRecord, String> {
    let path = format!("/api/tasks/{}", urlencoding::encode(task_id.trim()));
    task_runner_json(
        base_url,
        access_token,
        reqwest::Method::GET,
        path.as_str(),
        None::<&()>,
    )
    .await
}

pub async fn cancel_task_runner_task(
    base_url: &str,
    access_token: &str,
    user_access_token: Option<&str>,
    task_id: &str,
    request: &CancelTaskRunnerTaskRequest,
) -> Result<Value, String> {
    let path = format!("/api/tasks/{}/cancel", urlencoding::encode(task_id.trim()));
    let mut builder =
        task_runner_request(base_url, access_token, reqwest::Method::POST, path.as_str())
            .await
            .json(request);
    if let Some(value) = normalize_optional(user_access_token.map(ToOwned::to_owned)) {
        builder = builder.header("X-Chatos-User-Authorization", format!("Bearer {value}"));
    }
    send_task_runner_response(builder).await
}

pub async fn submit_task_runner_prompt(
    base_url: &str,
    access_token: &str,
    prompt_id: &str,
    request: &SubmitTaskRunnerPromptRequest,
) -> Result<Value, String> {
    let base_url = resolve_task_runner_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/prompts/{}/submit",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(prompt_id.trim())
    );
    send_json(
        task_runner_http_client()
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

pub async fn cancel_task_runner_prompt(
    base_url: &str,
    access_token: &str,
    prompt_id: &str,
    request: &CancelTaskRunnerPromptRequest,
) -> Result<Value, String> {
    let base_url = resolve_task_runner_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/prompts/{}/cancel",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(prompt_id.trim())
    );
    send_json(
        task_runner_http_client()
            .post(endpoint)
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

async fn send_json<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<T, String> {
    send_task_runner_response(request).await
}

async fn task_runner_json<T, B>(
    base_url: &str,
    access_token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<&B>,
) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
    B: Serialize + ?Sized,
{
    let mut request = task_runner_request(base_url, access_token, method, path).await;
    if let Some(body) = body {
        request = request.json(body);
    }
    send_task_runner_response(request).await
}

async fn task_runner_request(
    base_url: &str,
    access_token: &str,
    method: reqwest::Method,
    path: &str,
) -> reqwest::RequestBuilder {
    let base_url = resolve_task_runner_base_url(base_url).await;
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    task_runner_http_client()
        .request(method, endpoint)
        .bearer_auth(access_token.trim())
}

async fn send_task_runner_response<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<T, String> {
    send_task_runner_response_with_limit(
        request,
        TASK_RUNNER_DEFAULT_RESPONSE_LIMIT_BYTES,
        "Task Runner request failed",
    )
    .await
}

async fn send_task_runner_response_with_limit<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
    response_limit_bytes: usize,
    error_prefix: &str,
) -> Result<T, String> {
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = read_task_runner_body_limited(response, TASK_RUNNER_ERROR_BODY_PREVIEW_BYTES)
            .await
            .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
            .unwrap_or_default();
        return Err(format!("{error_prefix}: {status} {body}"));
    }
    let body = read_task_runner_body_limited(response, response_limit_bytes).await?;
    serde_json::from_slice::<T>(body.as_ref()).map_err(|err| err.to_string())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn task_runner_http_client() -> &'static reqwest::Client {
    TASK_RUNNER_HTTP_CLIENT.get_or_init(reqwest::Client::new)
}

async fn get_internal_json(
    base_url: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, String> {
    let base_url = resolve_task_runner_base_url(base_url).await;
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    send_task_runner_response_with_limit(
        task_runner_http_client().get(endpoint).query(query),
        TASK_RUNNER_INTERNAL_RESPONSE_LIMIT_BYTES,
        "Task Runner internal request failed",
    )
    .await
}

async fn post_internal_json<T: Serialize + ?Sized>(
    base_url: &str,
    path: &str,
    body: &T,
) -> Result<Value, String> {
    let base_url = resolve_task_runner_base_url(base_url).await;
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    send_task_runner_response_with_limit(
        task_runner_http_client().post(endpoint).json(body),
        TASK_RUNNER_INTERNAL_RESPONSE_LIMIT_BYTES,
        "Task Runner internal request failed",
    )
    .await
}

async fn resolve_task_runner_base_url(base_url: &str) -> String {
    chatos_service_runtime::resolve_service_base_url("task-runner", base_url).await
}

async fn resolve_user_service_base_url(base_url: &str) -> String {
    chatos_service_runtime::resolve_service_base_url("user-service", base_url).await
}

async fn read_task_runner_body_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<bytes::Bytes, String> {
    if let Some(content_length) = response.content_length() {
        ensure_task_runner_body_within_limit(content_length as usize, limit_bytes)?;
    }

    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let next_len = body.len().saturating_add(chunk.len());
        ensure_task_runner_body_within_limit(next_len, limit_bytes)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body.freeze())
}

fn ensure_task_runner_body_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "Task Runner response exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct SessionActiveMessageTasksRequest<'a> {
    source_session_id: &'a str,
    source_user_message_ids: &'a [String],
    source_turn_ids: &'a [String],
}

pub async fn list_session_active_message_tasks(
    base_url: &str,
    source_session_id: &str,
    source_user_message_ids: &[String],
    source_turn_ids: &[String],
) -> Result<Value, String> {
    post_internal_json(
        base_url,
        "/internal/chatos/session-active-message-tasks",
        &SessionActiveMessageTasksRequest {
            source_session_id,
            source_user_message_ids,
            source_turn_ids,
        },
    )
    .await
}

pub async fn list_message_tasks(
    base_url: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, "/internal/chatos/message-tasks", query.as_slice()).await
}

pub async fn get_message_task_graph(
    base_url: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, "/internal/chatos/message-graph", query.as_slice()).await
}

pub async fn get_message_task(
    base_url: &str,
    task_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-tasks/{}",
        urlencoding::encode(task_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

pub async fn get_message_run(
    base_url: &str,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
    event_limit: Option<usize>,
    event_offset: Option<usize>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-runs/{}",
        urlencoding::encode(run_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    let event_limit = event_limit.map(|value| value.to_string());
    let event_offset = event_offset.map(|value| value.to_string());
    if let Some(value) = event_limit.as_deref() {
        query.push(("event_limit", value));
    }
    if let Some(value) = event_offset.as_deref() {
        query.push(("event_offset", value));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

pub async fn get_message_run_output_changes(
    base_url: &str,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-runs/{}/output/changes",
        urlencoding::encode(run_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    let limit = limit.map(|value| value.to_string());
    let offset = offset.map(|value| value.to_string());
    if let Some(value) = limit.as_deref() {
        query.push(("limit", value));
    }
    if let Some(value) = offset.as_deref() {
        query.push(("offset", value));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

pub async fn get_message_run_output_diff(
    base_url: &str,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
    diff_path: &str,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-runs/{}/output/diff",
        urlencoding::encode(run_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    query.push(("path", diff_path));
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}

pub async fn get_message_graph_run(
    base_url: &str,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
    event_limit: Option<usize>,
    event_offset: Option<usize>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-graph/runs/{}",
        urlencoding::encode(run_id.trim())
    );
    let mut query = vec![("source_session_id", source_session_id)];
    if let Some(source_user_message_id) = source_user_message_id {
        query.push(("source_user_message_id", source_user_message_id));
    }
    if let Some(source_turn_id) = source_turn_id {
        query.push(("source_turn_id", source_turn_id));
    }
    let event_limit = event_limit.map(|value| value.to_string());
    let event_offset = event_offset.map(|value| value.to_string());
    if let Some(value) = event_limit.as_deref() {
        query.push(("event_limit", value));
    }
    if let Some(value) = event_offset.as_deref() {
        query.push(("event_offset", value));
    }
    get_internal_json(base_url, path.as_str(), query.as_slice()).await
}
