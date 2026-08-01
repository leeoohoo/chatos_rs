// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::Config;
use bytes::BytesMut;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{sync::OnceLock, time::Duration};

static TASK_RUNNER_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

const TASK_RUNNER_DEFAULT_RESPONSE_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const TASK_RUNNER_INTERNAL_RESPONSE_LIMIT_BYTES: usize = 8 * 1024 * 1024;
const TASK_RUNNER_ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;

mod message_tasks;
mod types;

#[cfg(test)]
mod tests;

pub use message_tasks::{
    get_message_graph_run, get_message_run, get_message_run_event, get_message_run_output_changes,
    get_message_run_output_diff, get_message_task, get_message_task_graph, list_message_tasks,
    list_session_active_message_tasks, retry_message_run,
};
pub use types::{
    CancelTaskRunnerPromptRequest, CancelTaskRunnerTaskRequest, SubmitTaskRunnerPromptRequest,
    TaskRunnerTaskRecord, UserServiceTaskRunnerExchange,
};

use types::UserServiceTaskRunnerTokenResponse;

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

pub async fn list_task_runner_available_plugins(
    base_url: &str,
    access_token: &str,
    device_id: Option<&str>,
    plan_mode: bool,
) -> Result<Value, String> {
    let mut query = vec![
        (
            "task_profile",
            if plan_mode { "chatos_plan" } else { "default" },
        ),
        (
            "requires_execution",
            if plan_mode { "false" } else { "true" },
        ),
    ];
    if let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty()) {
        query.push(("device_id", device_id));
    }
    let request = task_runner_request(
        base_url,
        access_token,
        reqwest::Method::GET,
        "/api/tasks/capabilities/catalog",
    )
    .await
    .query(&query);
    send_task_runner_response(request).await
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
            .timeout(task_runner_request_timeout())
            .bearer_auth(access_token.trim())
            .json(request),
    )
    .await
}

pub async fn get_task_runner_prompt(
    base_url: &str,
    access_token: &str,
    prompt_id: &str,
) -> Result<Value, String> {
    let base_url = resolve_task_runner_base_url(base_url).await;
    let endpoint = format!(
        "{}/api/prompts/{}",
        base_url.trim().trim_end_matches('/'),
        urlencoding::encode(prompt_id.trim())
    );
    send_json(
        task_runner_http_client()
            .get(endpoint)
            .timeout(task_runner_request_timeout())
            .bearer_auth(access_token.trim()),
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
            .timeout(task_runner_request_timeout())
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
        .timeout(task_runner_request_timeout())
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

fn task_runner_request_timeout() -> Duration {
    let timeout_ms = Config::try_get()
        .map(|cfg| cfg.task_runner_request_timeout_ms)
        .unwrap_or(30_000)
        .max(300) as u64;
    Duration::from_millis(timeout_ms)
}

async fn get_internal_json(
    base_url: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, String> {
    let base_url = resolve_task_runner_base_url(base_url).await;
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    send_task_runner_response_with_limit(
        signed_chatos_internal_request(
            task_runner_http_client()
                .get(endpoint)
                .timeout(task_runner_request_timeout()),
        )?
        .query(query),
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
    post_internal_json_with_scope(base_url, path, body, "chatos.messages.read").await
}

async fn post_internal_json_with_scope<T: Serialize + ?Sized>(
    base_url: &str,
    path: &str,
    body: &T,
    scope: &str,
) -> Result<Value, String> {
    let base_url = resolve_task_runner_base_url(base_url).await;
    let endpoint = format!("{}{}", base_url.trim().trim_end_matches('/'), path);
    send_task_runner_response_with_limit(
        signed_chatos_internal_request_with_scope(
            task_runner_http_client()
                .post(endpoint)
                .timeout(task_runner_request_timeout()),
            scope,
        )?
        .json(body),
        TASK_RUNNER_INTERNAL_RESPONSE_LIMIT_BYTES,
        "Task Runner internal request failed",
    )
    .await
}

fn signed_chatos_internal_request(
    request: reqwest::RequestBuilder,
) -> Result<reqwest::RequestBuilder, String> {
    signed_chatos_internal_request_with_scope(request, "chatos.messages.read")
}

fn signed_chatos_internal_request_with_scope(
    request: reqwest::RequestBuilder,
    scope: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let config = crate::config::Config::try_get()?;
    let secret = config
        .task_runner_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "CHATOS_TASK_RUNNER_INTERNAL_API_SECRET is required".to_string())?;
    signed_chatos_internal_request_with_secret_and_scope(request, secret, scope)
}

#[cfg(test)]
fn signed_chatos_internal_request_with_secret(
    request: reqwest::RequestBuilder,
    secret: &str,
) -> Result<reqwest::RequestBuilder, String> {
    signed_chatos_internal_request_with_secret_and_scope(request, secret, "chatos.messages.read")
}

fn signed_chatos_internal_request_with_secret_and_scope(
    request: reqwest::RequestBuilder,
    secret: &str,
    scope: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let token = chatos_service_runtime::issue_internal_service_token(
        secret,
        "chatos-backend",
        "task-runner",
        scope,
        60,
    )?;
    Ok(request
        .header("X-Task-Runner-Caller", "chatos-backend")
        .header("X-Task-Runner-Internal-Token", token))
}

#[derive(Debug, Serialize)]
struct ConfirmProjectExecutionRequest<'a> {
    project_id: &'a str,
    requirement_id: &'a str,
    source_session_id: &'a str,
    source_user_message_id: &'a str,
}

type MutateProjectExecutionRequest<'a> = ConfirmProjectExecutionRequest<'a>;

pub async fn confirm_project_execution(
    base_url: &str,
    project_id: &str,
    requirement_id: &str,
    source_session_id: &str,
    source_user_message_id: &str,
) -> Result<Value, String> {
    post_internal_json_with_scope(
        base_url,
        "/internal/chatos/project-execution/confirm",
        &ConfirmProjectExecutionRequest {
            project_id,
            requirement_id,
            source_session_id,
            source_user_message_id,
        },
        "chatos.execution.start",
    )
    .await
}

pub async fn pause_project_execution(
    base_url: &str,
    project_id: &str,
    requirement_id: &str,
    source_session_id: &str,
    source_user_message_id: &str,
) -> Result<Value, String> {
    mutate_project_execution_dispatch(
        base_url,
        "/internal/chatos/project-execution/pause",
        project_id,
        requirement_id,
        source_session_id,
        source_user_message_id,
    )
    .await
}

pub async fn resume_project_execution(
    base_url: &str,
    project_id: &str,
    requirement_id: &str,
    source_session_id: &str,
    source_user_message_id: &str,
) -> Result<Value, String> {
    mutate_project_execution_dispatch(
        base_url,
        "/internal/chatos/project-execution/resume",
        project_id,
        requirement_id,
        source_session_id,
        source_user_message_id,
    )
    .await
}

async fn mutate_project_execution_dispatch(
    base_url: &str,
    path: &str,
    project_id: &str,
    requirement_id: &str,
    source_session_id: &str,
    source_user_message_id: &str,
) -> Result<Value, String> {
    post_internal_json_with_scope(
        base_url,
        path,
        &MutateProjectExecutionRequest {
            project_id,
            requirement_id,
            source_session_id,
            source_user_message_id,
        },
        "chatos.execution.start",
    )
    .await
}

#[derive(Debug, Serialize)]
struct CloneProjectExecutionRequest<'a> {
    project_id: &'a str,
    requirement_id: &'a str,
    old_source_session_id: &'a str,
    old_source_user_message_id: &'a str,
    new_source_session_id: &'a str,
    new_source_user_message_id: &'a str,
}

pub async fn clone_project_execution(
    base_url: &str,
    project_id: &str,
    requirement_id: &str,
    old_source_session_id: &str,
    old_source_user_message_id: &str,
    new_source_session_id: &str,
    new_source_user_message_id: &str,
) -> Result<Value, String> {
    post_internal_json_with_scope(
        base_url,
        "/internal/chatos/project-execution/clone",
        &CloneProjectExecutionRequest {
            project_id,
            requirement_id,
            old_source_session_id,
            old_source_user_message_id,
            new_source_session_id,
            new_source_user_message_id,
        },
        "chatos.execution.start",
    )
    .await
}

#[derive(Debug, Serialize)]
struct RetireProjectExecutionRequest<'a> {
    project_id: &'a str,
    requirement_id: &'a str,
    source_session_id: &'a str,
    source_user_message_id: &'a str,
}

pub async fn retire_project_execution(
    base_url: &str,
    project_id: &str,
    requirement_id: &str,
    source_session_id: &str,
    source_user_message_id: &str,
) -> Result<Value, String> {
    post_internal_json_with_scope(
        base_url,
        "/internal/chatos/project-execution/retire",
        &RetireProjectExecutionRequest {
            project_id,
            requirement_id,
            source_session_id,
            source_user_message_id,
        },
        "chatos.execution.start",
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
