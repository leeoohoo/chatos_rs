use serde_json::Value;

use crate::models::message::Message;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;

use super::current_access_token;
use super::dto::{
    ComposeContextResponse, CreateSessionRequest, MemorySession, PatchSessionRequest,
    SummaryJobConfigDto, SyncMessageRequest, SyncTurnRuntimeSnapshotRequestDto,
    TaskExecutionRollupJobConfigDto, TaskExecutionSummaryJobConfigDto, TurnRuntimeSnapshotDto,
    TurnRuntimeSnapshotLookupResponseDto, UpsertSummaryJobConfigRequestDto,
    UpsertTaskExecutionRollupJobConfigRequestDto, UpsertTaskExecutionSummaryJobConfigRequestDto,
};
use super::http::{
    build_url, client, context_timeout_duration, push_limit_offset_params, send_delete_result,
    send_json, send_json_without_service_token, send_list, send_optional_json, timeout_duration,
};
use super::mapping::map_memory_session;

pub async fn list_sessions(
    user_id: Option<&str>,
    project_id: Option<&str>,
    limit: Option<i64>,
    offset: i64,
    include_archived: bool,
    include_archiving: bool,
) -> Result<Vec<Session>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(v) = user_id {
        params.push(("user_id".to_string(), v.to_string()));
    }
    if let Some(v) = project_id {
        params.push(("project_id".to_string(), v.to_string()));
    }
    push_limit_offset_params(&mut params, limit, offset);

    if !include_archived && !include_archiving {
        params.push(("status".to_string(), "active".to_string()));
    }

    let memory_sessions: Vec<MemorySession> = send_list("/sessions", &params).await?;

    let mut sessions: Vec<Session> = memory_sessions
        .into_iter()
        .map(map_memory_session)
        .collect();

    if include_archiving && !include_archived {
        sessions.retain(|s| s.status != "archived");
    }

    Ok(sessions)
}

pub async fn create_session(
    user_id: String,
    title: String,
    project_id: Option<String>,
    metadata: Option<Value>,
) -> Result<Session, String> {
    let req = client()
        .post(build_url("/sessions").as_str())
        .timeout(timeout_duration())
        .json(&CreateSessionRequest {
            user_id,
            project_id,
            title: Some(title),
            metadata,
        });

    let resp: MemorySession = send_json(req).await?;
    Ok(map_memory_session(resp))
}

pub async fn get_session_by_id(session_id: &str) -> Result<Option<Session>, String> {
    let req = client()
        .get(build_url(&format!("/sessions/{}", urlencoding::encode(session_id))).as_str())
        .timeout(timeout_duration());

    match send_optional_json::<MemorySession>(req).await? {
        Some(session) => Ok(Some(map_memory_session(session))),
        None => Ok(None),
    }
}

pub async fn update_session(
    session_id: &str,
    title: Option<String>,
    status: Option<String>,
    metadata: Option<Value>,
) -> Result<Option<Session>, String> {
    let req = client()
        .patch(build_url(&format!("/sessions/{}", urlencoding::encode(session_id))).as_str())
        .timeout(timeout_duration())
        .json(&PatchSessionRequest {
            title,
            status,
            metadata,
        });

    match send_optional_json::<MemorySession>(req).await? {
        Some(session) => Ok(Some(map_memory_session(session))),
        None => Ok(None),
    }
}

pub async fn delete_session(session_id: &str) -> Result<bool, String> {
    let req = client()
        .delete(build_url(&format!("/sessions/{}", urlencoding::encode(session_id))).as_str())
        .timeout(timeout_duration());

    send_delete_result(req).await
}

pub async fn upsert_message(message: &Message) -> Result<Message, String> {
    let internal_mode = current_access_token().is_none();
    let path = if internal_mode {
        format!(
            "/internal/sessions/{}/messages/{}/sync",
            urlencoding::encode(message.session_id.as_str()),
            urlencoding::encode(message.id.as_str())
        )
    } else {
        format!(
            "/sessions/{}/messages/{}/sync",
            urlencoding::encode(message.session_id.as_str()),
            urlencoding::encode(message.id.as_str())
        )
    };

    let req = client()
        .put(build_url(path.as_str()).as_str())
        .timeout(timeout_duration())
        .json(&SyncMessageRequest {
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

    if internal_mode {
        send_json_without_service_token(req).await
    } else {
        send_json(req).await
    }
}

pub async fn sync_turn_runtime_snapshot(
    session_id: &str,
    turn_id: &str,
    payload: &SyncTurnRuntimeSnapshotRequestDto,
) -> Result<TurnRuntimeSnapshotDto, String> {
    let path = format!(
        "/sessions/{}/turn-runtime-snapshots/{}/sync",
        urlencoding::encode(session_id),
        urlencoding::encode(turn_id)
    );

    let req = client()
        .put(build_url(path.as_str()).as_str())
        .timeout(timeout_duration())
        .json(payload);

    send_json(req).await
}

pub async fn get_latest_turn_runtime_snapshot(
    session_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    let path = format!(
        "/sessions/{}/turn-runtime-snapshots/latest",
        urlencoding::encode(session_id)
    );
    let req = client()
        .get(build_url(path.as_str()).as_str())
        .timeout(timeout_duration());
    send_json(req).await
}

pub async fn get_turn_runtime_snapshot_by_turn(
    session_id: &str,
    turn_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    let path = format!(
        "/sessions/{}/turn-runtime-snapshots/by-turn/{}",
        urlencoding::encode(session_id),
        urlencoding::encode(turn_id)
    );
    let req = client()
        .get(build_url(path.as_str()).as_str())
        .timeout(timeout_duration());
    send_json(req).await
}

pub async fn list_messages(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    let order = if asc { "asc" } else { "desc" };
    let mut params = vec![("order".to_string(), order.to_string())];
    push_limit_offset_params(&mut params, limit, offset);

    let path = format!("/sessions/{}/messages", urlencoding::encode(session_id));
    send_list(path.as_str(), &params).await
}

pub async fn delete_messages_by_session(session_id: &str) -> Result<i64, String> {
    let req = client()
        .delete(
            build_url(&format!(
                "/sessions/{}/messages",
                urlencoding::encode(session_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration());

    let resp: Value = send_json(req).await?;
    Ok(resp.get("deleted").and_then(|v| v.as_i64()).unwrap_or(0))
}

pub async fn get_message_by_id(message_id: &str) -> Result<Option<Message>, String> {
    let req = client()
        .get(build_url(&format!("/messages/{}", urlencoding::encode(message_id))).as_str())
        .timeout(timeout_duration());

    send_optional_json::<Message>(req).await
}

pub async fn delete_message(message_id: &str) -> Result<bool, String> {
    let req = client()
        .delete(build_url(&format!("/messages/{}", urlencoding::encode(message_id))).as_str())
        .timeout(timeout_duration());

    send_delete_result(req).await
}

pub async fn list_summaries(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<SessionSummaryV2>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    push_limit_offset_params(&mut params, limit, offset);

    let path = format!("/sessions/{}/summaries", urlencoding::encode(session_id));
    send_list(path.as_str(), &params).await
}

pub async fn delete_summary(session_id: &str, summary_id: &str) -> Result<bool, String> {
    let req = client()
        .delete(
            build_url(&format!(
                "/sessions/{}/summaries/{}",
                urlencoding::encode(session_id),
                urlencoding::encode(summary_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration());

    send_delete_result(req).await
}

pub async fn clear_summaries(session_id: &str) -> Result<i64, String> {
    let mut deleted = 0_i64;
    loop {
        let items = list_summaries(session_id, Some(200), 0).await?;
        if items.is_empty() {
            break;
        }
        for item in items {
            if delete_summary(session_id, item.id.as_str()).await? {
                deleted += 1;
            }
        }
    }
    Ok(deleted)
}

pub async fn compose_context(
    session_id: &str,
    memory_summary_limit: usize,
) -> Result<(Option<String>, usize, Vec<Message>), String> {
    let req = client()
        .post(build_url("/context/compose").as_str())
        .timeout(context_timeout_duration())
        .json(&serde_json::json!({
            "session_id": session_id,
            "summary_limit": memory_summary_limit.max(1),
            "include_raw_messages": true
        }));

    let resp: ComposeContextResponse = send_json(req).await?;
    Ok((resp.merged_summary, resp.summary_count, resp.messages))
}

pub async fn get_summary_job_config(user_id: &str) -> Result<SummaryJobConfigDto, String> {
    let req = client()
        .get(build_url("/configs/summary-job").as_str())
        .timeout(timeout_duration())
        .query(&[("user_id", user_id)]);
    send_json(req).await
}

pub async fn upsert_summary_job_config(
    req_body: &UpsertSummaryJobConfigRequestDto,
) -> Result<SummaryJobConfigDto, String> {
    let req = client()
        .put(build_url("/configs/summary-job").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

pub async fn get_task_execution_summary_job_config(
    user_id: &str,
) -> Result<TaskExecutionSummaryJobConfigDto, String> {
    let req = client()
        .get(build_url("/configs/task-execution-summary-job").as_str())
        .timeout(timeout_duration())
        .query(&[("user_id", user_id)]);
    send_json(req).await
}

pub async fn upsert_task_execution_summary_job_config(
    req_body: &UpsertTaskExecutionSummaryJobConfigRequestDto,
) -> Result<TaskExecutionSummaryJobConfigDto, String> {
    let req = client()
        .put(build_url("/configs/task-execution-summary-job").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

pub async fn get_task_execution_rollup_job_config(
    user_id: &str,
) -> Result<TaskExecutionRollupJobConfigDto, String> {
    let req = client()
        .get(build_url("/configs/task-execution-rollup-job").as_str())
        .timeout(timeout_duration())
        .query(&[("user_id", user_id)]);
    send_json(req).await
}

pub async fn upsert_task_execution_rollup_job_config(
    req_body: &UpsertTaskExecutionRollupJobConfigRequestDto,
) -> Result<TaskExecutionRollupJobConfigDto, String> {
    let req = client()
        .put(build_url("/configs/task-execution-rollup-job").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}
