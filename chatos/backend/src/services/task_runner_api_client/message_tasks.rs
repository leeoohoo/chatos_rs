// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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

#[derive(Debug, Serialize)]
struct RetryMessageRunRequest<'a> {
    source_session_id: &'a str,
    source_user_message_id: Option<&'a str>,
    source_turn_id: Option<&'a str>,
    retry_instruction: Option<&'a str>,
    execution_service_id: Option<&'a str>,
}

pub async fn retry_message_run(
    base_url: &str,
    run_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
    retry_instruction: Option<&str>,
    execution_service_id: Option<&str>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-runs/{}/retry",
        urlencoding::encode(run_id.trim())
    );
    post_internal_json_with_scope(
        base_url,
        path.as_str(),
        &RetryMessageRunRequest {
            source_session_id,
            source_user_message_id,
            source_turn_id,
            retry_instruction,
            execution_service_id,
        },
        "chatos.execution.start",
    )
    .await
}

pub async fn get_message_run_event(
    base_url: &str,
    run_id: &str,
    event_id: &str,
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Result<Value, String> {
    let path = format!(
        "/internal/chatos/message-runs/{}/events/{}",
        urlencoding::encode(run_id.trim()),
        urlencoding::encode(event_id.trim())
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
