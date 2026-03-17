use std::future::Future;
use std::time::Duration;

use once_cell::sync::Lazy;
use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;

static MEMORY_SERVER_HTTP: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

tokio::task_local! {
    static MEMORY_SERVER_ACCESS_TOKEN: Option<String>;
}

#[derive(Debug, Deserialize)]
pub struct MemoryAuthLoginResponse {
    pub token: String,
    #[serde(alias = "username")]
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct MemoryAuthMeResponse {
    #[serde(alias = "username")]
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
struct ListResponse<T> {
    items: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct MemorySession {
    id: String,
    user_id: String,
    project_id: Option<String>,
    title: Option<String>,
    metadata: Option<Value>,
    status: String,
    archived_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentSkillDto {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentDto {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    #[serde(default)]
    pub skills: Vec<MemoryAgentSkillDto>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub default_skill_ids: Vec<String>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryContactDto {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryProjectMemoryDto {
    pub id: String,
    pub user_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub memory_text: String,
    pub memory_version: i64,
    pub last_source_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentRecallDto {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub recall_key: String,
    pub recall_text: String,
    #[serde(default)]
    pub level: i64,
    #[serde(default)]
    pub source_project_ids: Vec<String>,
    pub confidence: Option<f64>,
    pub last_seen_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateMemoryContactRequestDto {
    pub user_id: Option<String>,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateMemoryContactResponseDto {
    pub created: bool,
    pub contact: MemoryContactDto,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentRuntimeContextDto {
    pub agent_id: String,
    pub name: String,
    pub role_definition: String,
    #[serde(default)]
    pub skills: Vec<MemoryAgentSkillDto>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
struct ComposeContextResponse {
    merged_summary: Option<String>,
    summary_count: usize,
    messages: Vec<Message>,
}

#[derive(Debug, Deserialize)]
pub struct SummaryJobConfigDto {
    pub user_id: String,
    pub enabled: i64,
    pub summary_model_config_id: Option<String>,
    pub token_limit: i64,
    pub round_limit: i64,
    pub target_summary_tokens: i64,
    pub job_interval_seconds: i64,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
struct CreateSessionRequest {
    user_id: String,
    project_id: Option<String>,
    title: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
struct PatchSessionRequest {
    title: Option<String>,
    status: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct CreateMemoryAgentRequestDto {
    pub user_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    pub skills: Option<Vec<MemoryAgentSkillDto>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct UpdateMemoryAgentRequestDto {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: Option<String>,
    pub skills: Option<Vec<MemoryAgentSkillDto>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SyncMessageRequest {
    role: String,
    content: String,
    message_mode: Option<String>,
    message_source: Option<String>,
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
    created_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpsertSummaryJobConfigRequestDto {
    pub user_id: String,
    pub enabled: Option<bool>,
    pub summary_model_config_id: Option<Option<String>>,
    pub token_limit: Option<i64>,
    pub round_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub job_interval_seconds: Option<i64>,
}

pub async fn with_access_token_scope<T, Fut>(access_token: Option<String>, future: Fut) -> T
where
    Fut: Future<Output = T>,
{
    MEMORY_SERVER_ACCESS_TOKEN
        .scope(normalize_optional_token(access_token), future)
        .await
}

pub fn spawn_with_current_access_token<Fut>(future: Fut) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    let access_token = current_access_token();
    tokio::spawn(async move { with_access_token_scope(access_token, future).await })
}

pub async fn auth_login(username: &str, password: &str) -> Result<MemoryAuthLoginResponse, String> {
    let req = MEMORY_SERVER_HTTP
        .post(build_url("/auth/login").as_str())
        .timeout(timeout_duration())
        .json(&serde_json::json!({
            "username": username,
            "password": password
        }));
    send_json_without_service_token(req).await
}

pub async fn auth_me(access_token: &str) -> Result<MemoryAuthMeResponse, String> {
    let trimmed = access_token.trim();
    if trimmed.is_empty() {
        return Err("access_token is required".to_string());
    }

    let req = MEMORY_SERVER_HTTP
        .get(build_url("/auth/me").as_str())
        .timeout(timeout_duration())
        .bearer_auth(trimmed);
    send_json_without_service_token(req).await
}

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
    if let Some(v) = limit {
        params.push(("limit".to_string(), v.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }

    if !include_archived && !include_archiving {
        params.push(("status".to_string(), "active".to_string()));
    }

    let req = MEMORY_SERVER_HTTP
        .get(build_url("/sessions").as_str())
        .timeout(timeout_duration())
        .query(&params);

    let resp: ListResponse<MemorySession> = send_json(req).await?;

    let mut sessions: Vec<Session> = resp.items.into_iter().map(map_memory_session).collect();

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
    let req = MEMORY_SERVER_HTTP
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
    let req = MEMORY_SERVER_HTTP
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
    let req = MEMORY_SERVER_HTTP
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
    let req = MEMORY_SERVER_HTTP
        .delete(build_url(&format!("/sessions/{}", urlencoding::encode(session_id))).as_str())
        .timeout(timeout_duration());

    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    Ok(true)
}

pub async fn upsert_message(message: &Message) -> Result<Message, String> {
    let path = format!(
        "/sessions/{}/messages/{}/sync",
        urlencoding::encode(message.session_id.as_str()),
        urlencoding::encode(message.id.as_str())
    );

    let req = MEMORY_SERVER_HTTP
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
    if let Some(v) = limit {
        params.push(("limit".to_string(), v.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }

    let req = MEMORY_SERVER_HTTP
        .get(
            build_url(&format!(
                "/sessions/{}/messages",
                urlencoding::encode(session_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .query(&params);

    let resp: ListResponse<Message> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn delete_messages_by_session(session_id: &str) -> Result<i64, String> {
    let req = MEMORY_SERVER_HTTP
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
    let req = MEMORY_SERVER_HTTP
        .get(build_url(&format!("/messages/{}", urlencoding::encode(message_id))).as_str())
        .timeout(timeout_duration());

    send_optional_json::<Message>(req).await
}

pub async fn delete_message(message_id: &str) -> Result<bool, String> {
    let req = MEMORY_SERVER_HTTP
        .delete(build_url(&format!("/messages/{}", urlencoding::encode(message_id))).as_str())
        .timeout(timeout_duration());

    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    Ok(true)
}

pub async fn list_summaries(
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<SessionSummaryV2>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(v) = limit {
        params.push(("limit".to_string(), v.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }

    let req = MEMORY_SERVER_HTTP
        .get(
            build_url(&format!(
                "/sessions/{}/summaries",
                urlencoding::encode(session_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .query(&params);

    let resp: ListResponse<SessionSummaryV2> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn delete_summary(session_id: &str, summary_id: &str) -> Result<bool, String> {
    let req = MEMORY_SERVER_HTTP
        .delete(
            build_url(&format!(
                "/sessions/{}/summaries/{}",
                urlencoding::encode(session_id),
                urlencoding::encode(summary_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration());

    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    Ok(true)
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
    let req = MEMORY_SERVER_HTTP
        .post(build_url("/context/compose").as_str())
        .timeout(Duration::from_millis(
            Config::get().memory_server_context_timeout_ms.max(300) as u64,
        ))
        .json(&serde_json::json!({
            "session_id": session_id,
            "summary_limit": memory_summary_limit.max(1),
            "include_raw_messages": true
        }));

    let resp: ComposeContextResponse = send_json(req).await?;
    Ok((resp.merged_summary, resp.summary_count, resp.messages))
}

pub async fn get_summary_job_config(user_id: &str) -> Result<SummaryJobConfigDto, String> {
    let req = MEMORY_SERVER_HTTP
        .get(build_url("/configs/summary-job").as_str())
        .timeout(timeout_duration())
        .query(&[("user_id", user_id)]);
    send_json(req).await
}

pub async fn upsert_summary_job_config(
    req_body: &UpsertSummaryJobConfigRequestDto,
) -> Result<SummaryJobConfigDto, String> {
    let req = MEMORY_SERVER_HTTP
        .put(build_url("/configs/summary-job").as_str())
        .timeout(timeout_duration())
        .json(req_body);
    send_json(req).await
}

pub async fn list_memory_agents(
    user_id: Option<&str>,
    enabled: Option<bool>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryAgentDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(value) = user_id {
        params.push(("user_id".to_string(), value.to_string()));
    }
    if let Some(value) = enabled {
        params.push(("enabled".to_string(), value.to_string()));
    }
    if let Some(value) = limit {
        params.push(("limit".to_string(), value.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }

    let req = MEMORY_SERVER_HTTP
        .get(build_url("/agents").as_str())
        .timeout(timeout_duration())
        .query(&params);
    let resp: ListResponse<MemoryAgentDto> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn list_memory_contacts(
    user_id: Option<&str>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryContactDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(value) = user_id {
        params.push(("user_id".to_string(), value.to_string()));
    }
    if let Some(value) = limit {
        params.push(("limit".to_string(), value.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }

    let req = MEMORY_SERVER_HTTP
        .get(build_url("/contacts").as_str())
        .timeout(timeout_duration())
        .query(&params);
    let resp: ListResponse<MemoryContactDto> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn create_memory_contact(
    payload: &CreateMemoryContactRequestDto,
) -> Result<CreateMemoryContactResponseDto, String> {
    let req = MEMORY_SERVER_HTTP
        .post(build_url("/contacts").as_str())
        .timeout(timeout_duration())
        .json(payload);
    send_json(req).await
}

pub async fn delete_memory_contact(contact_id: &str) -> Result<bool, String> {
    let req = MEMORY_SERVER_HTTP
        .delete(build_url(&format!("/contacts/{}", urlencoding::encode(contact_id))).as_str())
        .timeout(timeout_duration());

    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    Ok(true)
}

pub async fn list_contact_project_memories(
    contact_id: &str,
    project_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectMemoryDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(value) = limit {
        params.push(("limit".to_string(), value.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }

    let req = MEMORY_SERVER_HTTP
        .get(
            build_url(&format!(
                "/contacts/{}/project-memories/{}",
                urlencoding::encode(contact_id),
                urlencoding::encode(project_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .query(&params);
    let resp: ListResponse<MemoryProjectMemoryDto> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn list_contact_project_memories_by_contact(
    contact_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectMemoryDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(value) = limit {
        params.push(("limit".to_string(), value.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }

    let req = MEMORY_SERVER_HTTP
        .get(
            build_url(&format!(
                "/contacts/{}/project-memories",
                urlencoding::encode(contact_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .query(&params);
    let resp: ListResponse<MemoryProjectMemoryDto> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn list_contact_agent_recalls(
    contact_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryAgentRecallDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(value) = limit {
        params.push(("limit".to_string(), value.max(1).to_string()));
    }
    if offset > 0 {
        params.push(("offset".to_string(), offset.to_string()));
    }

    let req = MEMORY_SERVER_HTTP
        .get(
            build_url(&format!(
                "/contacts/{}/agent-recalls",
                urlencoding::encode(contact_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration())
        .query(&params);
    let resp: ListResponse<MemoryAgentRecallDto> = send_json(req).await?;
    Ok(resp.items)
}

pub async fn get_memory_agent(agent_id: &str) -> Result<Option<MemoryAgentDto>, String> {
    let req = MEMORY_SERVER_HTTP
        .get(build_url(&format!("/agents/{}", urlencoding::encode(agent_id))).as_str())
        .timeout(timeout_duration());
    send_optional_json(req).await
}

pub async fn create_memory_agent(
    payload: &CreateMemoryAgentRequestDto,
) -> Result<MemoryAgentDto, String> {
    let req = MEMORY_SERVER_HTTP
        .post(build_url("/agents").as_str())
        .timeout(timeout_duration())
        .json(payload);
    send_json(req).await
}

pub async fn update_memory_agent(
    agent_id: &str,
    payload: &UpdateMemoryAgentRequestDto,
) -> Result<Option<MemoryAgentDto>, String> {
    let req = MEMORY_SERVER_HTTP
        .patch(build_url(&format!("/agents/{}", urlencoding::encode(agent_id))).as_str())
        .timeout(timeout_duration())
        .json(payload);
    send_optional_json(req).await
}

pub async fn delete_memory_agent(agent_id: &str) -> Result<bool, String> {
    let req = MEMORY_SERVER_HTTP
        .delete(build_url(&format!("/agents/{}", urlencoding::encode(agent_id))).as_str())
        .timeout(timeout_duration());

    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(false);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    Ok(true)
}

pub async fn get_memory_agent_runtime_context(
    agent_id: &str,
) -> Result<Option<MemoryAgentRuntimeContextDto>, String> {
    let req = MEMORY_SERVER_HTTP
        .get(
            build_url(&format!(
                "/agents/{}/runtime-context",
                urlencoding::encode(agent_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration());
    send_optional_json(req).await
}

pub async fn ai_create_memory_agent(payload: &Value) -> Result<Value, String> {
    let req = MEMORY_SERVER_HTTP
        .post(build_url("/agents/ai-create").as_str())
        .timeout(timeout_duration())
        .json(payload);
    send_json(req).await
}

fn map_memory_session(value: MemorySession) -> Session {
    let (selected_model_id, selected_agent_id) =
        extract_selection_from_session_metadata(value.metadata.as_ref());
    Session {
        id: value.id,
        title: value.title.unwrap_or_else(|| "Untitled".to_string()),
        description: None,
        metadata: value.metadata,
        selected_model_id,
        selected_agent_id,
        user_id: Some(value.user_id),
        project_id: value.project_id,
        status: value.status,
        archived_at: value.archived_at,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}

fn extract_selection_from_session_metadata(
    metadata: Option<&Value>,
) -> (Option<String>, Option<String>) {
    let Some(Value::Object(metadata_map)) = metadata else {
        return (None, None);
    };
    let selected_model_id = metadata_map
        .get("chat_runtime")
        .and_then(Value::as_object)
        .and_then(|runtime| {
            runtime
                .get("selected_model_id")
                .or_else(|| runtime.get("selectedModelId"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            metadata_map
                .get("ui_chat_selection")
                .and_then(Value::as_object)
                .and_then(|selection| {
                    selection
                        .get("selected_model_id")
                        .or_else(|| selection.get("selectedModelId"))
                })
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });

    let selected_agent_id = metadata_map
        .get("contact")
        .and_then(Value::as_object)
        .and_then(|contact| contact.get("agent_id"))
        .or_else(|| {
            metadata_map
                .get("ui_contact")
                .and_then(Value::as_object)
                .and_then(|contact| contact.get("agent_id"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            metadata_map
                .get("ui_chat_selection")
                .and_then(Value::as_object)
                .and_then(|selection| {
                    selection
                        .get("selected_agent_id")
                        .or_else(|| selection.get("selectedAgentId"))
                })
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });

    (selected_model_id, selected_agent_id)
}

fn build_url(path: &str) -> String {
    format!(
        "{}{}",
        Config::get().memory_server_base_url.trim_end_matches('/'),
        path
    )
}

fn timeout_duration() -> Duration {
    Duration::from_millis(Config::get().memory_server_request_timeout_ms.max(300) as u64)
}

fn apply_auth(req: RequestBuilder) -> RequestBuilder {
    if let Some(access_token) = current_access_token() {
        return req.bearer_auth(access_token);
    }
    let token = Config::get().memory_server_service_token.trim();
    if token.is_empty() {
        req
    } else {
        req.header("X-Service-Token", token)
    }
}

async fn send_json<T: DeserializeOwned>(req: RequestBuilder) -> Result<T, String> {
    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

async fn send_optional_json<T: DeserializeOwned>(req: RequestBuilder) -> Result<Option<T>, String> {
    let resp = apply_auth(req).send().await.map_err(|e| e.to_string())?;
    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map(Some).map_err(|e| e.to_string())
}

async fn send_json_without_service_token<T: DeserializeOwned>(
    req: RequestBuilder,
) -> Result<T, String> {
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("status={} detail={}", status, detail));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

fn current_access_token() -> Option<String> {
    MEMORY_SERVER_ACCESS_TOKEN
        .try_with(|token| token.clone())
        .ok()
        .flatten()
        .and_then(|token| normalize_optional_token(Some(token)))
}

fn normalize_optional_token(token: Option<String>) -> Option<String> {
    token.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{MemoryAuthLoginResponse, MemoryAuthMeResponse};

    #[test]
    fn auth_login_response_supports_user_id_field() {
        let value = serde_json::json!({
            "token": "t1",
            "user_id": "alice",
            "role": "user"
        });
        let parsed: MemoryAuthLoginResponse =
            serde_json::from_value(value).expect("login response with user_id should parse");
        assert_eq!(parsed.user_id, "alice");
    }

    #[test]
    fn auth_login_response_supports_username_alias() {
        let value = serde_json::json!({
            "token": "t1",
            "username": "alice",
            "role": "user"
        });
        let parsed: MemoryAuthLoginResponse =
            serde_json::from_value(value).expect("login response with username should parse");
        assert_eq!(parsed.user_id, "alice");
    }

    #[test]
    fn auth_me_response_supports_user_id_field() {
        let value = serde_json::json!({
            "user_id": "alice",
            "role": "user"
        });
        let parsed: MemoryAuthMeResponse =
            serde_json::from_value(value).expect("me response with user_id should parse");
        assert_eq!(parsed.user_id, "alice");
    }

    #[test]
    fn auth_me_response_supports_username_alias() {
        let value = serde_json::json!({
            "username": "alice",
            "role": "user"
        });
        let parsed: MemoryAuthMeResponse =
            serde_json::from_value(value).expect("me response with username should parse");
        assert_eq!(parsed.user_id, "alice");
    }
}
