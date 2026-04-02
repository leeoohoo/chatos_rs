use crate::models::message::Message;

use super::dto::{
    SyncTaskExecutionMessageRequestDto, TaskExecutionComposeResponseDto, TaskExecutionMessageDto,
    TaskExecutionSummaryDto,
};
use super::http::{
    build_url, client, context_timeout_duration, push_limit_offset_params, send_delete_result,
    send_json, send_json_without_service_token, send_list, send_list_without_service_token,
    timeout_duration,
};
use super::current_access_token;

#[derive(Debug, Clone)]
pub struct TaskExecutionScopeBinding {
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub source_session_id: Option<String>,
}

fn map_task_execution_message(dto: TaskExecutionMessageDto) -> Message {
    Message {
        id: dto.id,
        session_id: dto
            .source_session_id
            .clone()
            .unwrap_or_else(|| dto.scope_key.clone()),
        role: dto.role,
        content: dto.content,
        message_mode: dto.message_mode,
        message_source: dto.message_source,
        summary: None,
        tool_calls: dto.tool_calls,
        tool_call_id: dto.tool_call_id,
        reasoning: dto.reasoning,
        metadata: dto.metadata,
        created_at: dto.created_at,
    }
}

pub async fn upsert_task_execution_message(
    scope: &TaskExecutionScopeBinding,
    message: &Message,
) -> Result<Message, String> {
    let internal_mode = current_access_token().is_none();
    let path = if internal_mode {
        format!(
            "/internal/task-executions/messages/{}/sync",
            urlencoding::encode(message.id.as_str())
        )
    } else {
        format!(
            "/task-executions/messages/{}/sync",
            urlencoding::encode(message.id.as_str())
        )
    };
    let req = client()
        .put(build_url(path.as_str()).as_str())
        .timeout(timeout_duration())
        .json(&SyncTaskExecutionMessageRequestDto {
            user_id: scope.user_id.clone(),
            contact_agent_id: scope.contact_agent_id.clone(),
            project_id: scope.project_id.clone(),
            task_id: scope.task_id.clone(),
            source_session_id: scope.source_session_id.clone(),
            role: message.role.clone(),
            content: message.content.clone(),
            message_mode: message.message_mode.clone(),
            message_source: message.message_source.clone(),
            tool_calls: message.tool_calls.clone(),
            tool_call_id: message.tool_call_id.clone(),
            reasoning: message.reasoning.clone(),
            metadata: message.metadata.clone(),
            created_at: Some(message.created_at.clone()),
        });

    let dto: TaskExecutionMessageDto = if internal_mode {
        send_json_without_service_token(req).await?
    } else {
        send_json(req).await?
    };
    Ok(map_task_execution_message(dto))
}

pub async fn list_task_execution_messages(
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    limit: Option<i64>,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    let internal_mode = current_access_token().is_none();
    let order = if asc { "asc" } else { "desc" };
    let mut params = vec![
        ("user_id".to_string(), user_id.to_string()),
        ("contact_agent_id".to_string(), contact_agent_id.to_string()),
        ("project_id".to_string(), project_id.to_string()),
        ("order".to_string(), order.to_string()),
    ];
    push_limit_offset_params(&mut params, limit, offset);
    let path = if internal_mode {
        "/internal/task-executions/messages"
    } else {
        "/task-executions/messages"
    };
    let dtos: Vec<TaskExecutionMessageDto> = if internal_mode {
        send_list_without_service_token(path, &params).await?
    } else {
        send_list(path, &params).await?
    };
    Ok(dtos.into_iter().map(map_task_execution_message).collect())
}

pub async fn delete_task_execution_messages(
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
) -> Result<i64, String> {
    let req = client()
        .delete(build_url("/task-executions/messages").as_str())
        .timeout(timeout_duration())
        .query(&[
            ("user_id", user_id),
            ("contact_agent_id", contact_agent_id),
            ("project_id", project_id),
        ]);

    let resp: serde_json::Value = send_json(req).await?;
    Ok(resp.get("deleted").and_then(|v| v.as_i64()).unwrap_or(0))
}

pub async fn list_task_execution_summaries(
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<TaskExecutionSummaryDto>, String> {
    let mut params = vec![
        ("user_id".to_string(), user_id.to_string()),
        ("contact_agent_id".to_string(), contact_agent_id.to_string()),
        ("project_id".to_string(), project_id.to_string()),
    ];
    push_limit_offset_params(&mut params, limit, offset);
    send_list("/task-executions/summaries", &params).await
}

pub async fn delete_task_execution_summary(
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    summary_id: &str,
) -> Result<bool, String> {
    let req = client()
        .delete(
            build_url(
                format!(
                    "/task-executions/summaries/{}",
                    urlencoding::encode(summary_id)
                )
                .as_str(),
            )
            .as_str(),
        )
        .timeout(timeout_duration())
        .query(&[
            ("user_id", user_id),
            ("contact_agent_id", contact_agent_id),
            ("project_id", project_id),
        ]);
    send_delete_result(req).await
}

pub async fn compose_task_execution_context(
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    memory_summary_limit: usize,
) -> Result<(Option<String>, usize, Vec<Message>), String> {
    let internal_mode = current_access_token().is_none();
    let req = client()
        .post(
            build_url(if internal_mode {
                "/internal/task-executions/context/compose"
            } else {
                "/task-executions/context/compose"
            })
            .as_str(),
        )
        .timeout(context_timeout_duration())
        .json(&serde_json::json!({
            "user_id": user_id,
            "contact_agent_id": contact_agent_id,
            "project_id": project_id,
            "summary_limit": memory_summary_limit.max(1),
            "include_raw_messages": true
        }));

    let resp: TaskExecutionComposeResponseDto = if internal_mode {
        send_json_without_service_token(req).await?
    } else {
        send_json(req).await?
    };
    Ok((
        resp.merged_summary,
        resp.summary_count,
        resp.messages
            .into_iter()
            .map(map_task_execution_message)
            .collect(),
    ))
}
