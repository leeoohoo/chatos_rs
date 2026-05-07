use serde_json::Value;

use crate::models::message::Message;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;

use super::dto::{
    ComposeContextResponse, CreateSessionRequest, DeleteSummaryResultDto, MemorySession,
    PatchSessionRequest, ReviewRepairStatusDto, ReviewRepairSummaryRunResultDto,
    RunReviewRepairSummaryRequestDto, SummaryJobConfigDto, SyncMessageRequest,
    SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotDto,
    TurnRuntimeSnapshotLookupResponseDto, UpsertSummaryJobConfigRequestDto,
};
use super::http::{
    client, push_limit_offset_params, read_status_detail_error, send_delete_result, send_json,
    send_list, send_optional_json, try_apply_auth, try_background_job_timeout_duration,
    try_build_url, try_context_timeout_duration, try_timeout_duration,
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
        .post(try_build_url("/sessions")?)
        .timeout(try_timeout_duration()?)
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
        .get(try_build_url(&format!(
            "/sessions/{}",
            urlencoding::encode(session_id)
        ))?)
        .timeout(try_timeout_duration()?);

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
        .patch(try_build_url(&format!(
            "/sessions/{}",
            urlencoding::encode(session_id)
        ))?)
        .timeout(try_timeout_duration()?)
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
        .delete(try_build_url(&format!(
            "/sessions/{}",
            urlencoding::encode(session_id)
        ))?)
        .timeout(try_timeout_duration()?);

    send_delete_result(req).await
}

pub async fn upsert_message(message: &Message) -> Result<Message, String> {
    let path = format!(
        "/sessions/{}/messages/{}/sync",
        urlencoding::encode(message.session_id.as_str()),
        urlencoding::encode(message.id.as_str())
    );

    let req = client()
        .put(try_build_url(path.as_str())?)
        .timeout(try_timeout_duration()?)
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

    send_json(req).await
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
        .put(try_build_url(path.as_str())?)
        .timeout(try_timeout_duration()?)
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
        .get(try_build_url(path.as_str())?)
        .timeout(try_timeout_duration()?);
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
        .get(try_build_url(path.as_str())?)
        .timeout(try_timeout_duration()?);
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
        .delete(try_build_url(&format!(
            "/sessions/{}/messages",
            urlencoding::encode(session_id)
        ))?)
        .timeout(try_timeout_duration()?);

    let resp: Value = send_json(req).await?;
    Ok(resp.get("deleted").and_then(|v| v.as_i64()).unwrap_or(0))
}

pub async fn get_message_by_id(message_id: &str) -> Result<Option<Message>, String> {
    let req = client()
        .get(try_build_url(&format!(
            "/messages/{}",
            urlencoding::encode(message_id)
        ))?)
        .timeout(try_timeout_duration()?);

    send_optional_json::<Message>(req).await
}

pub async fn delete_message(message_id: &str) -> Result<bool, String> {
    let req = client()
        .delete(try_build_url(&format!(
            "/messages/{}",
            urlencoding::encode(message_id)
        ))?)
        .timeout(try_timeout_duration()?);

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

pub async fn delete_summary(
    session_id: &str,
    summary_id: &str,
) -> Result<super::DeleteSummaryResultDto, String> {
    let req = client()
        .delete(try_build_url(&format!(
            "/sessions/{}/summaries/{}",
            urlencoding::encode(session_id),
            urlencoding::encode(summary_id)
        ))?)
        .timeout(try_timeout_duration()?);

    let resp = try_apply_auth(req)?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(DeleteSummaryResultDto {
            success: false,
            reset_messages: 0,
        });
    }
    if !resp.status().is_success() {
        return Err(read_status_detail_error(resp).await);
    }
    let payload: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    serde_json::from_value(payload).map_err(|err| err.to_string())
}

pub async fn clear_summaries(session_id: &str) -> Result<i64, String> {
    let mut deleted = 0_i64;
    loop {
        let items = list_summaries(session_id, Some(200), 0).await?;
        if items.is_empty() {
            break;
        }
        for item in items {
            if delete_summary(session_id, item.id.as_str()).await?.success {
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
        .post(try_build_url("/context/compose")?)
        .timeout(try_context_timeout_duration()?)
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
        .get(try_build_url("/configs/summary-job")?)
        .timeout(try_timeout_duration()?)
        .query(&[("user_id", user_id)]);
    send_json(req).await
}

pub async fn upsert_summary_job_config(
    req_body: &UpsertSummaryJobConfigRequestDto,
) -> Result<SummaryJobConfigDto, String> {
    let req = client()
        .put(try_build_url("/configs/summary-job")?)
        .timeout(try_timeout_duration()?)
        .json(req_body);
    send_json(req).await
}

pub async fn run_review_repair_summary(
    req_body: &RunReviewRepairSummaryRequestDto,
) -> Result<ReviewRepairSummaryRunResultDto, String> {
    let req = client()
        .post(try_build_url("/jobs/summary/review-repair-run-once")?)
        .timeout(try_background_job_timeout_duration()?)
        .json(req_body);

    let resp: Value = send_json(req).await?;
    let data = resp.get("data").cloned().unwrap_or(resp);
    serde_json::from_value(data).map_err(|err| err.to_string())
}

pub async fn get_review_repair_status(
    req_body: &RunReviewRepairSummaryRequestDto,
) -> Result<ReviewRepairStatusDto, String> {
    let mut req = client()
        .get(try_build_url("/jobs/summary/review-repair-status")?)
        .timeout(try_timeout_duration()?);

    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(value) = req_body.user_id.as_deref() {
        params.push(("user_id".to_string(), value.to_string()));
    }
    if let Some(value) = req_body.project_id.as_deref() {
        params.push(("project_id".to_string(), value.to_string()));
    }
    if let Some(value) = req_body.contact_id.as_deref() {
        params.push(("contact_id".to_string(), value.to_string()));
    }
    if let Some(value) = req_body.agent_id.as_deref() {
        params.push(("agent_id".to_string(), value.to_string()));
    }
    if !params.is_empty() {
        req = req.query(&params);
    }

    let resp: Value = send_json(req).await?;
    let data = resp.get("data").cloned().unwrap_or(resp);
    serde_json::from_value(data).map_err(|err| err.to_string())
}
