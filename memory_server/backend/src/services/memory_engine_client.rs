use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::models::{
    AgentMemorySummarySource, AgentRecall, ComposeContextRequest, ComposeContextResponse, Message,
    ProjectMemory, Session, SessionSummary,
    SummaryRollupJobConfig,
};
use crate::repositories::{project_agent_links, sessions};

fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn metadata_string(metadata: Option<&serde_json::Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalized_text(cursor.as_str())
}

fn contact_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "contact_id"])
        .or_else(|| metadata_string(metadata, &["contact", "contactId"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "contact_id"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "contactId"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contact_id"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contactId"]))
}

fn agent_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "agent_id"])
        .or_else(|| metadata_string(metadata, &["contact", "agentId"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "agentId"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selected_agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selectedAgentId"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contact_agent_id"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contactAgentId"]))
}

fn project_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["chat_runtime", "project_id"])
        .or_else(|| metadata_string(metadata, &["chat_runtime", "projectId"]))
}

fn project_id_from_summary_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["legacy_session_mapping", "project_id"])
        .or_else(|| metadata_string(metadata, &["project_id"]))
        .or_else(|| metadata_string(metadata, &["projectId"]))
}

fn build_session_mapping_metadata(session: &Session) -> Option<serde_json::Value> {
    let original = session.metadata.clone();
    let metadata_ref = original.as_ref();
    let project_id = normalized_text(session.project_id.as_deref())
        .or_else(|| project_id_from_metadata(metadata_ref));
    let contact_id = contact_id_from_metadata(metadata_ref);
    let agent_id = agent_id_from_metadata(metadata_ref);

    Some(serde_json::json!({
        "legacy_session_mapping": {
            "session_id": session.id,
            "project_id": project_id,
            "contact_id": contact_id,
            "agent_id": agent_id,
        },
        "source_metadata": original
    }))
}

fn build_related_subject_ids(session: &Session) -> Vec<String> {
    let metadata_ref = session.metadata.as_ref();
    let project_id = normalized_text(session.project_id.as_deref())
        .or_else(|| project_id_from_metadata(metadata_ref));
    let contact_id = contact_id_from_metadata(metadata_ref);
    let agent_id = agent_id_from_metadata(metadata_ref);

    let mut out = Vec::new();
    if let Some(contact_id) = contact_id.clone() {
        out.push(format!("contact:{contact_id}"));
    }
    if let Some(agent_id) = agent_id.clone() {
        out.push(format!("agent:{agent_id}"));
    }
    if let Some(project_id) = project_id {
        out.push(format!("project:{project_id}"));
        if let Some(contact_id) = contact_id.clone() {
            out.push(format!("contact_project:{contact_id}:{project_id}"));
        }
        if let Some(agent_id) = agent_id.clone() {
            out.push(format!("agent_project:{agent_id}:{project_id}"));
        }
    }
    out
}

fn build_thread_labels(session: &Session) -> Option<Vec<String>> {
    let metadata_ref = session.metadata.as_ref();
    let project_id = normalized_text(session.project_id.as_deref())
        .or_else(|| project_id_from_metadata(metadata_ref));
    let contact_id = contact_id_from_metadata(metadata_ref);
    let agent_id = agent_id_from_metadata(metadata_ref);

    let mut labels = Vec::new();
    if let Some(project_id) = project_id.clone() {
        labels.push(format!("project:{project_id}"));
        if let Some(contact_id) = contact_id.clone() {
            labels.push(format!("contact_project:{contact_id}:{project_id}"));
        }
        if let Some(agent_id) = agent_id.clone() {
            labels.push(format!("agent_project:{agent_id}:{project_id}"));
        }
    }
    if let Some(contact_id) = contact_id {
        labels.push(format!("contact:{contact_id}"));
    }
    if let Some(agent_id) = agent_id {
        labels.push(format!("agent:{agent_id}"));
    }

    if labels.is_empty() {
        None
    } else {
        Some(labels)
    }
}

fn engine_base_url(config: &AppConfig) -> &str {
    config.memory_engine_base_url.trim_end_matches('/')
}

#[derive(Debug, Clone, Serialize)]
struct EngineComposeContextRequest {
    tenant_id: String,
    source_id: String,
    subject_id: Option<String>,
    related_subject_ids: Option<Vec<String>>,
    thread_id: String,
    policy: Option<EngineComposeContextPolicy>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineComposeContextPolicy {
    include_recent_records: Option<bool>,
    include_thread_summary: Option<bool>,
    include_subject_memory: Option<bool>,
    recent_record_limit: Option<usize>,
    summary_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineUpsertThreadRequest {
    tenant_id: String,
    source_id: String,
    subject_id: String,
    thread_type: String,
    external_thread_id: Option<String>,
    title: Option<String>,
    labels: Option<Vec<String>>,
    metadata: Option<serde_json::Value>,
    status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EngineThread {
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineListThreadsByLabelRequest {
    tenant_id: String,
    source_id: String,
    thread_label: String,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineBatchSyncRecordsRequest {
    tenant_id: String,
    source_id: String,
    records: Vec<EngineUpsertRecordInput>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineRunThreadSummaryRequest {
    tenant_id: String,
    source_id: String,
    max_records: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineRunThreadRepairScopeRequest {
    tenant_id: String,
    source_id: String,
    thread_label: String,
    thread_status: Option<String>,
    pending_record_type: Option<String>,
    max_threads: Option<i64>,
    max_records_per_thread: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineGetThreadRepairScopeStatusRequest {
    tenant_id: String,
    source_id: String,
    thread_label: String,
    thread_status: Option<String>,
    pending_record_type: Option<String>,
    max_threads: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineRunPendingSummariesRequest {
    tenant_id: Option<String>,
    source_id: Option<String>,
    max_threads: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineUpsertRecordInput {
    id: String,
    external_record_id: Option<String>,
    role: String,
    record_type: String,
    content: String,
    structured_payload: Option<serde_json::Value>,
    metadata: Option<serde_json::Value>,
    created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineComposeContextResponse {
    thread_id: String,
    blocks: Vec<EngineComposeContextBlock>,
    recent_records: Vec<EngineRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRunThreadSummaryResponse {
    pub thread_id: String,
    pub generated: bool,
    pub summary_id: Option<String>,
    pub source_record_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRunThreadRepairScopeResponse {
    pub thread_label: String,
    pub scope_thread_count: usize,
    pub processed_threads: usize,
    pub summarized_threads: usize,
    pub generated_summaries: usize,
    pub failed_threads: usize,
    pub pending_record_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineGetThreadRepairScopeStatusResponse {
    pub thread_label: String,
    pub running: bool,
    pub running_job_count: i64,
    pub pending_record_count: i64,
    pub scope_thread_count: usize,
    pub job_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRunPendingSummariesResponse {
    pub processed_threads: usize,
    pub summarized_threads: usize,
}

#[derive(Debug, Clone, Serialize)]
struct EngineRunPendingRollupsRequest {
    tenant_id: Option<String>,
    source_id: Option<String>,
    summary_prompt: Option<String>,
    max_threads: Option<i64>,
    round_limit: Option<i64>,
    token_limit: Option<i64>,
    target_summary_tokens: Option<i64>,
    keep_level0_count: Option<i64>,
    max_level: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRunPendingRollupsResponse {
    pub processed_threads: usize,
    pub rolled_up_threads: usize,
    pub generated_summaries: usize,
    pub marked_summaries: usize,
    pub failed_threads: usize,
}

#[derive(Debug, Clone, Serialize)]
struct EngineRunSubjectMemoryJobRequest {
    tenant_id: String,
    source_id: String,
    subject_id: String,
    memory_type: String,
    source_thread_label: String,
    relation_subject_id: Option<String>,
    source_summary_type: Option<String>,
    summary_prompt: Option<String>,
    prompt_title: Option<String>,
    round_limit: Option<i64>,
    token_limit: Option<i64>,
    target_summary_tokens: Option<i64>,
    keep_level0_count: Option<i64>,
    max_level: Option<i64>,
    memory_metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineRunSubjectMemoryJobResponse {
    pub subject_id: String,
    pub generated_level0: usize,
    pub generated_rollups: usize,
    pub generated_memories: usize,
    pub marked_source_summaries: usize,
    pub marked_source_memories: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct EngineSubjectMemory {
    id: String,
    tenant_id: String,
    subject_id: String,
    memory_key: String,
    text: String,
    level: i64,
    source_digest: Option<String>,
    confidence: Option<f64>,
    last_seen_at: Option<String>,
    metadata: Option<serde_json::Value>,
    rollup_status: String,
    rollup_memory_key: Option<String>,
    rolled_up_at: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineListSubjectMemoriesResponse {
    items: Vec<EngineSubjectMemory>,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineComposeContextBlock {
    block_type: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineRecord {
    id: String,
    thread_id: String,
    #[allow(dead_code)]
    external_record_id: Option<String>,
    role: String,
    #[allow(dead_code)]
    record_type: String,
    content: String,
    #[allow(dead_code)]
    summary_status: Option<String>,
    #[allow(dead_code)]
    summary_id: Option<String>,
    #[allow(dead_code)]
    summarized_at: Option<String>,
    metadata: Option<serde_json::Value>,
    created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineSummary {
    id: String,
    thread_id: String,
    summary_type: String,
    level: i64,
    summary_text: String,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: i64,
    status: String,
    rollup_summary_id: Option<String>,
    subject_memory_summarized: i64,
    #[allow(dead_code)]
    subject_memory_summarized_at: Option<String>,
    metadata: Option<serde_json::Value>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineListRecordsResponse {
    items: Vec<EngineRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineDeleteRecordResponse {
    deleted: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineDeleteRecordsResponse {
    deleted: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineListSummariesResponse {
    items: Vec<EngineSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineListSummariesByThreadLabelRequest {
    tenant_id: String,
    source_id: String,
    thread_label: String,
    summary_type: Option<String>,
    status: Option<String>,
    level: Option<i64>,
    subject_memory_summarized: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineQuerySubjectMemoriesRequest {
    tenant_id: String,
    source_id: String,
    subject_id: String,
    memory_type: Option<String>,
    level: Option<i64>,
    max_level_exclusive: Option<i64>,
    rollup_status: Option<String>,
    relation_subject_id: Option<String>,
    source_digest: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineDeleteSummaryResponse {
    reset_records: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct EngineListThreadsResponse {
    items: Vec<EngineThread>,
}

pub async fn compose_context(
    config: &AppConfig,
    db: &crate::db::Db,
    req: &ComposeContextRequest,
) -> Result<ComposeContextResponse, String> {
    let base_url = engine_base_url(config);
    let session = sessions::get_session_by_id(db, req.session_id.as_str())
        .await?
        .ok_or_else(|| format!("session not found: {}", req.session_id))?;

    let tenant_id = session.user_id.trim().to_string();
    if tenant_id.is_empty() {
        return Err(format!("session {} has empty user_id", session.id));
    }

    let engine_req = EngineComposeContextRequest {
        tenant_id,
        source_id: "memory_server".to_string(),
        subject_id: Some(format!("session:{}", req.session_id)),
        related_subject_ids: {
            let related = build_related_subject_ids(&session);
            if related.is_empty() {
                None
            } else {
                Some(related)
            }
        },
        thread_id: req.session_id.clone(),
        policy: Some(EngineComposeContextPolicy {
            include_recent_records: req.include_raw_messages,
            include_thread_summary: Some(true),
            include_subject_memory: Some(true),
            recent_record_limit: req.pending_limit,
            summary_limit: req.summary_limit,
        }),
    };

    let client = Client::builder()
        .timeout(Duration::from_secs(config.memory_engine_timeout_secs))
        .build()
        .map_err(|err| err.to_string())?;

    let endpoint = format!(
        "{}/api/memory-engine/v1/context/compose",
        base_url
    );

    let response = client
        .post(endpoint.as_str())
        .json(&engine_req)
        .send()
        .await
        .map_err(|err| format!("memory engine request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine compose failed: status={} body={}",
            status, body
        ));
    }

    let engine_response: EngineComposeContextResponse = response
        .json()
        .await
        .map_err(|err| format!("memory engine decode failed: {err}"))?;

    let summary_count = engine_response.blocks.len();

    let merged_summary = if engine_response.blocks.is_empty() {
        None
    } else {
        Some(
            engine_response
                .blocks
                .iter()
                .map(|block| format!("[{}]\n{}", block.block_type, block.text))
                .collect::<Vec<_>>()
                .join("\n\n===\n\n"),
        )
    };

    let messages = engine_response
        .recent_records
        .into_iter()
        .map(engine_record_to_message)
        .collect::<Vec<_>>();

    Ok(ComposeContextResponse {
        session_id: engine_response.thread_id,
        merged_summary,
        summary_count,
        messages,
        meta: crate::models::ComposeContextMeta {
            used_levels: Vec::new(),
            filtered_rollup_count: 0,
            kept_raw_level0_count: 0,
        },
    })
}

pub async fn sync_session(
    config: &AppConfig,
    session: &Session,
) -> Result<(), String> {
    let base_url = engine_base_url(config);
    let tenant_id = session.user_id.trim();
    if tenant_id.is_empty() {
        return Ok(());
    }

    let request = EngineUpsertThreadRequest {
        tenant_id: tenant_id.to_string(),
        source_id: "memory_server".to_string(),
        subject_id: format!("session:{}", session.id),
        thread_type: "chat".to_string(),
        external_thread_id: Some(session.id.clone()),
        title: session.title.clone(),
        labels: build_thread_labels(session),
        metadata: build_session_mapping_metadata(session),
        status: Some(session.status.clone()),
    };

    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}",
        base_url,
        urlencoding::encode(session.id.as_str())
    );

    let response = build_client(config)?
        .put(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine sync session request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine sync session failed: status={} body={}",
            status, body
        ));
    }

    Ok(())
}

pub async fn list_subject_memories(
    config: &AppConfig,
    tenant_id: &str,
    subject_id: &str,
    memory_type: Option<&str>,
    level: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSubjectMemory>, String> {
    let mut endpoint = format!(
        "{}/api/memory-engine/v1/subjects/{}/memories?tenant_id={}&source_id=memory_server&limit={}&offset={}",
        engine_base_url(config),
        urlencoding::encode(subject_id),
        urlencoding::encode(tenant_id),
        limit.max(1).min(1000),
        offset.max(0),
    );
    if let Some(memory_type) = memory_type.map(str::trim).filter(|value| !value.is_empty()) {
        endpoint.push_str("&memory_type=");
        endpoint.push_str(urlencoding::encode(memory_type).as_ref());
    }
    if let Some(level) = level {
        endpoint.push_str("&level=");
        endpoint.push_str(level.to_string().as_str());
    }

    let response = build_client(config)?
        .get(endpoint.as_str())
        .send()
        .await
        .map_err(|err| format!("memory engine list subject memories request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine list subject memories failed: status={} body={}",
            status, body
        ));
    }

    response
        .json::<EngineListSubjectMemoriesResponse>()
        .await
        .map(|payload| payload.items)
        .map_err(|err| format!("memory engine list subject memories decode failed: {err}"))
}

pub async fn list_project_memories_for_contact(
    config: &AppConfig,
    tenant_id: &str,
    contact_id: &str,
    project_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ProjectMemory>, String> {
    let mut items = Vec::new();
    if let Some(project_id) = project_id.map(str::trim).filter(|value| !value.is_empty()) {
        let subject_id = format!("contact_project:{contact_id}:{project_id}");
        let rows = list_subject_memories(
            config,
            tenant_id,
            subject_id.as_str(),
            Some("project_memory"),
            None,
            limit,
            offset,
        )
        .await?;
        items.extend(rows.into_iter().map(engine_subject_memory_to_project_memory));
    } else {
        return Ok(Vec::new());
    }

    items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(items)
}

pub async fn list_project_memories_by_contact(
    config: &AppConfig,
    db: &crate::db::Db,
    tenant_id: &str,
    contact_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ProjectMemory>, String> {
    let mut project_ids = Vec::new();

    let links = project_agent_links::list_project_agent_links_by_contact(
        db,
        tenant_id,
        contact_id,
        Some("active"),
        500,
        0,
    )
    .await?;
    for link in links {
        let project_id = normalized_text(Some(link.project_id.as_str()))
            .unwrap_or_else(|| "0".to_string());
        if !project_ids.iter().any(|existing| existing == &project_id) {
            project_ids.push(project_id);
        }
    }

    let threads = list_threads_by_label(
        config,
        tenant_id,
        format!("contact:{contact_id}").as_str(),
        Some("active"),
        5_000,
        0,
    )
    .await?;

    for thread in threads {
        let project_id = thread
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("legacy_session_mapping"))
            .and_then(|mapping| mapping.get("project_id"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "0".to_string());
        if !project_ids.iter().any(|existing| existing == &project_id) {
            project_ids.push(project_id);
        }
    }

    let mut items = Vec::new();
    for project_id in project_ids {
        let mut rows = list_project_memories_for_contact(
            config,
            tenant_id,
            contact_id,
            Some(project_id.as_str()),
            limit,
            0,
        )
        .await?;
        items.append(&mut rows);
    }

    items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    items.dedup_by(|left, right| {
        left.project_id == right.project_id && left.contact_id == right.contact_id
    });
    let skip = offset.max(0) as usize;
    Ok(items
        .into_iter()
        .skip(skip)
        .take(limit.max(1).min(1000) as usize)
        .collect())
}

pub async fn list_agent_recalls(
    config: &AppConfig,
    tenant_id: &str,
    agent_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<AgentRecall>, String> {
    let subject_id = format!("agent:{agent_id}");
    let items = list_subject_memories(
        config,
        tenant_id,
        subject_id.as_str(),
        Some("agent_recall"),
        None,
        limit,
        offset,
    )
    .await?;
    Ok(items
        .into_iter()
        .map(|item| engine_subject_memory_to_agent_recall(item, agent_id))
        .collect())
}

async fn query_engine_subject_memories(
    config: &AppConfig,
    request: &EngineQuerySubjectMemoriesRequest,
) -> Result<Vec<EngineSubjectMemory>, String> {
    let endpoint = format!(
        "{}/api/memory-engine/v1/subject-memories/query",
        engine_base_url(config),
    );

    let response = build_client(config)?
        .post(endpoint.as_str())
        .json(request)
        .send()
        .await
        .map_err(|err| format!("memory engine query subject memories failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine query subject memories failed: status={} body={}",
            status, body
        ));
    }

    response
        .json::<EngineListSubjectMemoriesResponse>()
        .await
        .map(|payload| payload.items)
        .map_err(|err| format!("memory engine query subject memories decode failed: {err}"))
}

pub async fn has_pending_agent_recalls_before_level(
    config: &AppConfig,
    tenant_id: &str,
    agent_id: &str,
    max_level: i64,
) -> Result<bool, String> {
    if max_level <= 0 {
        return Ok(false);
    }

    let subject_id = format!("agent:{agent_id}");
    let items = query_engine_subject_memories(
        config,
        &EngineQuerySubjectMemoriesRequest {
            tenant_id: tenant_id.to_string(),
            source_id: "memory_server".to_string(),
            subject_id: subject_id.clone(),
            memory_type: Some("agent_recall".to_string()),
            level: None,
            max_level_exclusive: Some(max_level.max(1)),
            rollup_status: Some("pending".to_string()),
            relation_subject_id: Some(subject_id),
            source_digest: None,
            limit: Some(1),
            offset: Some(0),
        },
    )
    .await?;

    Ok(!items.is_empty())
}

pub async fn sync_message(
    config: &AppConfig,
    db: &crate::db::Db,
    message: &Message,
) -> Result<(), String> {
    let base_url = engine_base_url(config);
    let session = sessions::get_session_by_id(db, message.session_id.as_str())
        .await?
        .ok_or_else(|| format!("session not found for message sync: {}", message.session_id))?;
    let tenant_id = session.user_id.trim();
    if tenant_id.is_empty() {
        return Ok(());
    }

    let request = EngineBatchSyncRecordsRequest {
        tenant_id: tenant_id.to_string(),
        source_id: "memory_server".to_string(),
        records: vec![EngineUpsertRecordInput {
            id: message.id.clone(),
            external_record_id: Some(message.id.clone()),
            role: message.role.clone(),
            record_type: "message".to_string(),
            content: message.content.clone(),
            structured_payload: None,
            metadata: message.metadata.clone(),
            created_at: message.created_at.clone(),
        }],
    };

    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}/records/batch-sync",
        base_url,
        urlencoding::encode(message.session_id.as_str())
    );

    let response = build_client(config)?
        .put(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine sync message request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine sync message failed: status={} body={}",
            status, body
        ));
    }

    Ok(())
}

fn engine_record_to_message(record: EngineRecord) -> Message {
    let (message_mode, message_source, tool_calls, tool_call_id, reasoning, metadata) =
        unpack_record_metadata(record.metadata);
    Message {
        id: record.id,
        session_id: record.thread_id,
        role: record.role,
        content: record.content,
        message_mode,
        message_source,
        tool_calls,
        tool_call_id,
        reasoning,
        metadata,
        summary_status: record.summary_status.unwrap_or_else(|| "pending".to_string()),
        summary_id: record.summary_id,
        summarized_at: record.summarized_at,
        created_at: record.created_at,
    }
}

fn engine_summary_to_session_summary(item: EngineSummary) -> SessionSummary {
    let summary_model = item
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("summary_model"))
        .and_then(|value| value.as_str())
        .unwrap_or("memory_engine")
        .to_string();
    let trigger_type = match item.summary_type.as_str() {
        "thread_repair" => "review_repair".to_string(),
        other => other.to_string(),
    };

    let mapped_status = match item.status.as_str() {
        "done" => "summarized".to_string(),
        other => other.to_string(),
    };

    SessionSummary {
        id: item.id,
        session_id: item.thread_id,
        source_digest: None,
        summary_text: item.summary_text,
        summary_model,
        trigger_type,
        source_start_message_id: item.source_record_start_id,
        source_end_message_id: item.source_record_end_id,
        source_message_count: item.source_record_count,
        source_estimated_tokens: 0,
        status: mapped_status,
        error_message: None,
        level: item.level,
        rollup_summary_id: item.rollup_summary_id,
        rolled_up_at: None,
        agent_memory_summarized: item.subject_memory_summarized,
        agent_memory_summarized_at: item.subject_memory_summarized_at,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

fn engine_summary_to_agent_memory_summary_source(
    item: EngineSummary,
    project_id: Option<String>,
) -> AgentMemorySummarySource {
    let summary_model = item
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("summary_model"))
        .and_then(|value| value.as_str())
        .unwrap_or("memory_engine")
        .to_string();

    AgentMemorySummarySource {
        id: item.id,
        session_id: item.thread_id,
        summary_text: item.summary_text,
        summary_model,
        trigger_type: item.summary_type,
        source_start_message_id: item.source_record_start_id,
        source_end_message_id: item.source_record_end_id,
        source_message_count: item.source_record_count,
        source_estimated_tokens: 0,
        status: item.status,
        level: item.level,
        project_id,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

fn project_id_from_subject_id(subject_id: &str) -> Option<String> {
    subject_id
        .strip_prefix("project:")
        .or_else(|| subject_id.split("contact_project:").nth(1))
        .or_else(|| subject_id.split("agent_project:").nth(1))
        .and_then(|tail| tail.rsplit(':').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn engine_subject_memory_to_project_memory(item: EngineSubjectMemory) -> ProjectMemory {
    let mapping = item
        .metadata
        .as_ref()
        .and_then(|value| value.get("legacy_session_mapping"));
    let contact_id = mapping
        .and_then(|value| value.get("contact_id"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let agent_id = mapping
        .and_then(|value| value.get("agent_id"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let project_id = mapping
        .and_then(|value| value.get("project_id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .or_else(|| project_id_from_subject_id(item.subject_id.as_str()))
        .unwrap_or_else(|| "0".to_string());

    ProjectMemory {
        id: item.id,
        user_id: item.tenant_id,
        contact_id,
        agent_id,
        project_id,
        memory_text: item.text,
        memory_version: 1,
        recall_summarized: if item.rollup_status == "done" { 1 } else { 0 },
        recall_summarized_at: item.rolled_up_at,
        last_source_at: item.last_seen_at,
        updated_at: item.updated_at,
    }
}

fn engine_subject_memory_to_agent_recall(item: EngineSubjectMemory, agent_id: &str) -> AgentRecall {
    AgentRecall {
        id: item.id,
        user_id: item.tenant_id,
        agent_id: agent_id.to_string(),
        recall_key: item.memory_key,
        source_digest: item.source_digest,
        recall_text: item.text,
        level: item.level,
        rolled_up: if item.rollup_status == "done" { 1 } else { 0 },
        rollup_recall_key: item.rollup_memory_key,
        rolled_up_at: item.rolled_up_at,
        confidence: item.confidence,
        last_seen_at: item.last_seen_at,
        updated_at: item.updated_at,
    }
}

fn build_record_metadata(
    message_mode: Option<String>,
    message_source: Option<String>,
    tool_calls: Option<serde_json::Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut merged = match metadata {
        Some(serde_json::Value::Object(map)) => map,
        Some(other) => {
            let mut map = serde_json::Map::new();
            map.insert("legacy_metadata".to_string(), other);
            map
        }
        None => serde_json::Map::new(),
    };

    if let Some(value) = message_mode.filter(|v| !v.trim().is_empty()) {
        merged.insert("message_mode".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = message_source.filter(|v| !v.trim().is_empty()) {
        merged.insert("message_source".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = tool_calls {
        merged.insert("tool_calls".to_string(), value);
    }
    if let Some(value) = tool_call_id.filter(|v| !v.trim().is_empty()) {
        merged.insert("tool_call_id".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = reasoning.filter(|v| !v.trim().is_empty()) {
        merged.insert("reasoning".to_string(), serde_json::Value::String(value));
    }

    if merged.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(merged))
    }
}

fn unpack_record_metadata(
    metadata: Option<serde_json::Value>,
) -> (
    Option<String>,
    Option<String>,
    Option<serde_json::Value>,
    Option<String>,
    Option<String>,
    Option<serde_json::Value>,
) {
    let Some(serde_json::Value::Object(mut map)) = metadata else {
        return (None, Some("memory_engine".to_string()), None, None, None, None);
    };

    let message_mode = map
        .remove("message_mode")
        .and_then(|value| value.as_str().map(ToOwned::to_owned));
    let message_source = map
        .remove("message_source")
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .or_else(|| Some("memory_engine".to_string()));
    let tool_calls = map.remove("tool_calls");
    let tool_call_id = map
        .remove("tool_call_id")
        .and_then(|value| value.as_str().map(ToOwned::to_owned));
    let reasoning = map
        .remove("reasoning")
        .and_then(|value| value.as_str().map(ToOwned::to_owned));

    let metadata = if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map))
    };

    (
        message_mode,
        message_source,
        tool_calls,
        tool_call_id,
        reasoning,
        metadata,
    )
}

async fn get_session_for_engine_message(
    db: &crate::db::Db,
    session_id: &str,
) -> Result<Session, String> {
    sessions::get_session_by_id(db, session_id)
        .await?
        .ok_or_else(|| format!("session not found: {session_id}"))
}

pub async fn create_message(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
    req: crate::models::CreateMessageRequest,
) -> Result<Message, String> {
    let created_at = chrono::Utc::now().to_rfc3339();
    sync_message_input(
        config,
        db,
        session_id,
        MessageSyncRequest {
            message_id: Some(Uuid::new_v4().to_string()),
            role: req.role,
            content: req.content,
            message_mode: req.message_mode,
            message_source: req.message_source,
            tool_calls: req.tool_calls,
            tool_call_id: req.tool_call_id,
            reasoning: req.reasoning,
            metadata: req.metadata,
            created_at,
        },
    )
    .await
}

#[derive(Debug, Clone)]
pub struct MessageSyncRequest {
    pub message_id: Option<String>,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<serde_json::Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

pub async fn sync_message_input(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
    req: MessageSyncRequest,
) -> Result<Message, String> {
    let session = get_session_for_engine_message(db, session_id).await?;
    sync_session(config, &session).await?;

    let message_id = req
        .message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let tenant_id = session.user_id.trim().to_string();
    if tenant_id.is_empty() {
        return Err(format!("session {} has empty user_id", session.id));
    }

    let message = Message {
        id: message_id.clone(),
        session_id: session.id.clone(),
        role: req.role.clone(),
        content: req.content.clone(),
        message_mode: req.message_mode.clone(),
        message_source: req.message_source.clone(),
        tool_calls: req.tool_calls.clone(),
        tool_call_id: req.tool_call_id.clone(),
        reasoning: req.reasoning.clone(),
        metadata: build_record_metadata(
            req.message_mode,
            req.message_source,
            req.tool_calls,
            req.tool_call_id,
            req.reasoning,
            req.metadata,
        ),
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: req.created_at,
    };

    sync_message(config, db, &message).await?;
    run_thread_summary(config, tenant_id.as_str(), session.id.as_str()).await?;
    Ok(message)
}

pub async fn batch_create_messages(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
    requests: Vec<crate::models::CreateMessageRequest>,
) -> Result<Vec<Message>, String> {
    let session = get_session_for_engine_message(db, session_id).await?;
    sync_session(config, &session).await?;

    let items = requests
        .into_iter()
        .map(|req| Message {
            id: Uuid::new_v4().to_string(),
            session_id: session.id.clone(),
            role: req.role,
            content: req.content,
            message_mode: req.message_mode.clone(),
            message_source: req.message_source.clone(),
            tool_calls: req.tool_calls.clone(),
            tool_call_id: req.tool_call_id.clone(),
            reasoning: req.reasoning.clone(),
            metadata: build_record_metadata(
                req.message_mode,
                req.message_source,
                req.tool_calls,
                req.tool_call_id,
                req.reasoning,
                req.metadata,
            ),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .collect::<Vec<_>>();

    sync_messages_batch(config, db, session.id.as_str(), items.as_slice()).await?;
    run_thread_summary(config, session.user_id.as_str(), session.id.as_str()).await?;
    Ok(items)
}

pub async fn list_messages(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
    limit: i64,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    let session = get_session_for_engine_message(db, session_id).await?;
    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}/records?tenant_id={}&source_id=memory_server&record_type=message&limit={}&offset={}&order={}",
        engine_base_url(config),
        urlencoding::encode(session.id.as_str()),
        urlencoding::encode(session.user_id.as_str()),
        limit.max(1).min(2000),
        offset.max(0),
        if asc { "asc" } else { "desc" }
    );

    let response = build_client(config)?
        .get(endpoint.as_str())
        .send()
        .await
        .map_err(|err| format!("memory engine list messages request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine list messages failed: status={} body={}",
            status, body
        ));
    }

    let payload = response
        .json::<EngineListRecordsResponse>()
        .await
        .map_err(|err| format!("memory engine list messages decode failed: {err}"))?;
    Ok(payload
        .items
        .into_iter()
        .map(engine_record_to_message)
        .collect())
}

pub async fn get_message(
    config: &AppConfig,
    db: &crate::db::Db,
    message_id: &str,
    session_id: Option<&str>,
) -> Result<Option<Message>, String> {
    if let Some(session_id) = session_id {
        return get_message_in_session(config, db, session_id, message_id).await;
    }

    let sessions = sessions::list_sessions(db, None, None, Some("active"), 5000, 0).await?;
    for session in sessions {
        if let Some(message) =
            get_message_in_session(config, db, session.id.as_str(), message_id).await?
        {
            return Ok(Some(message));
        }
    }
    Ok(None)
}

pub async fn delete_message(
    config: &AppConfig,
    db: &crate::db::Db,
    message_id: &str,
    session_id: Option<&str>,
) -> Result<bool, String> {
    if let Some(session_id) = session_id {
        return delete_message_in_session(config, db, session_id, message_id).await;
    }

    let sessions = sessions::list_sessions(db, None, None, Some("active"), 5000, 0).await?;
    for session in sessions {
        if delete_message_in_session(config, db, session.id.as_str(), message_id).await? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub async fn clear_session_messages(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
) -> Result<i64, String> {
    let session = get_session_for_engine_message(db, session_id).await?;
    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}/records?tenant_id={}&source_id=memory_server&record_type=message",
        engine_base_url(config),
        urlencoding::encode(session.id.as_str()),
        urlencoding::encode(session.user_id.as_str())
    );

    let response = build_client(config)?
        .delete(endpoint.as_str())
        .send()
        .await
        .map_err(|err| format!("memory engine clear session messages request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine clear session messages failed: status={} body={}",
            status, body
        ));
    }

    let payload = response
        .json::<EngineDeleteRecordsResponse>()
        .await
        .map_err(|err| format!("memory engine clear session messages decode failed: {err}"))?;
    Ok(payload.deleted)
}

pub async fn get_latest_user_message_by_session(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
) -> Result<Option<Message>, String> {
    let session = get_session_for_engine_message(db, session_id).await?;
    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}/records?tenant_id={}&source_id=memory_server&record_type=message&role=user&limit=1&offset=0&order=desc",
        engine_base_url(config),
        urlencoding::encode(session.id.as_str()),
        urlencoding::encode(session.user_id.as_str())
    );

    let response = build_client(config)?
        .get(endpoint.as_str())
        .send()
        .await
        .map_err(|err| format!("memory engine latest user message request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine latest user message failed: status={} body={}",
            status, body
        ));
    }

    let mut payload = response
        .json::<EngineListRecordsResponse>()
        .await
        .map_err(|err| format!("memory engine latest user message decode failed: {err}"))?;
    Ok(payload.items.pop().map(engine_record_to_message))
}

pub async fn list_summaries(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
    level: Option<i64>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<SessionSummary>, String> {
    let session = get_session_for_engine_message(db, session_id).await?;
    let normalized_status = status.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else if trimmed.eq_ignore_ascii_case("summarized") {
            Some("done")
        } else {
            Some(trimmed)
        }
    });
    let mut endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}/summaries?limit={}&offset={}",
        engine_base_url(config),
        urlencoding::encode(session.id.as_str()),
        limit.max(1).min(500),
        offset.max(0)
    );
    if let Some(level) = level {
        endpoint.push_str("&level=");
        endpoint.push_str(level.to_string().as_str());
    }
    if let Some(status) = normalized_status {
        endpoint.push_str("&status=");
        endpoint.push_str(urlencoding::encode(status).as_ref());
    }

    let response = build_client(config)?
        .get(endpoint.as_str())
        .send()
        .await
        .map_err(|err| format!("memory engine list summaries request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine list summaries failed: status={} body={}",
            status, body
        ));
    }

    let payload = response
        .json::<EngineListSummariesResponse>()
        .await
        .map_err(|err| format!("memory engine list summaries decode failed: {err}"))?;
    Ok(payload
        .items
        .into_iter()
        .map(engine_summary_to_session_summary)
        .collect())
}

async fn list_engine_summaries_by_thread_label(
    config: &AppConfig,
    tenant_id: &str,
    thread_label: &str,
    summary_type: Option<&str>,
    status: Option<&str>,
    level: Option<i64>,
    subject_memory_summarized: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSummary>, String> {
    let request = EngineListSummariesByThreadLabelRequest {
        tenant_id: tenant_id.to_string(),
        source_id: "memory_server".to_string(),
        thread_label: thread_label.to_string(),
        summary_type: summary_type.map(ToOwned::to_owned),
        status: status.map(ToOwned::to_owned),
        level,
        subject_memory_summarized,
        limit: Some(limit.max(1).min(5_000)),
        offset: Some(offset.max(0)),
    };

    let endpoint = format!(
        "{}/api/memory-engine/v1/summaries/query-by-thread-label",
        engine_base_url(config),
    );

    let response = build_client(config)?
        .post(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine label summary query failed: {err}"))?;
    if !response.status().is_success() {
        let status_code = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine label summary query failed: status={} body={}",
            status_code, body
        ));
    }

    response
        .json::<EngineListSummariesResponse>()
        .await
        .map(|payload| payload.items)
        .map_err(|err| format!("memory engine label summary query decode failed: {err}"))
}

pub async fn list_pending_agent_memory_summaries_by_agent(
    config: &AppConfig,
    tenant_id: &str,
    agent_id: &str,
) -> Result<Vec<AgentMemorySummarySource>, String> {
    let thread_label = format!("agent:{agent_id}");
    let summaries = list_engine_summaries_by_thread_label(
        config,
        tenant_id,
        thread_label.as_str(),
        Some("thread_incremental"),
        Some("done"),
        None,
        Some(0),
        5_000,
        0,
    )
    .await?;

    let mut out = summaries
        .into_iter()
        .map(|item| {
            let project_id = project_id_from_summary_metadata(item.metadata.as_ref());
            engine_summary_to_agent_memory_summary_source(item, project_id)
        })
        .collect::<Vec<_>>();
    out.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(out)
}

pub async fn list_summaries_by_thread_label(
    config: &AppConfig,
    tenant_id: &str,
    thread_label: &str,
    summary_type: Option<&str>,
    status: Option<&str>,
    level: Option<i64>,
    subject_memory_summarized: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<SessionSummary>, String> {
    let items = list_engine_summaries_by_thread_label(
        config,
        tenant_id,
        thread_label,
        summary_type,
        status,
        level,
        subject_memory_summarized,
        limit,
        offset,
    )
    .await?;

    Ok(items
        .into_iter()
        .map(engine_summary_to_session_summary)
        .collect())
}

pub async fn list_threads_by_label(
    config: &AppConfig,
    tenant_id: &str,
    thread_label: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineThread>, String> {
    let request = EngineListThreadsByLabelRequest {
        tenant_id: tenant_id.to_string(),
        source_id: "memory_server".to_string(),
        thread_label: thread_label.to_string(),
        status: status.map(ToOwned::to_owned),
        limit: Some(limit.max(1).min(5_000)),
        offset: Some(offset.max(0)),
    };

    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/query-by-label",
        engine_base_url(config),
    );

    let response = build_client(config)?
        .post(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine label thread query failed: {err}"))?;
    if !response.status().is_success() {
        let status_code = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine label thread query failed: status={} body={}",
            status_code, body
        ));
    }

    response
        .json::<EngineListThreadsResponse>()
        .await
        .map(|payload| payload.items)
        .map_err(|err| format!("memory engine label thread query decode failed: {err}"))
}

pub async fn list_agent_ids_with_pending_agent_memory_by_user(
    config: &AppConfig,
    db: &crate::db::Db,
    tenant_id: &str,
    limit: i64,
) -> Result<Vec<String>, String> {
    let contacts = crate::repositories::contacts::list_contacts(db, tenant_id, Some("active"), 5_000, 0)
        .await?;
    let mut out = Vec::new();
    for contact in contacts {
        let items = list_pending_agent_memory_summaries_by_agent(
            config,
            tenant_id,
            contact.agent_id.as_str(),
        )
        .await?;
        if !items.is_empty() {
            out.push(contact.agent_id);
            if out.len() >= limit.max(1).min(5_000) as usize {
                break;
            }
        }
    }
    Ok(out)
}

pub async fn list_all_summaries_by_session(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
) -> Result<Vec<SessionSummary>, String> {
    list_summaries(config, db, session_id, None, None, 500, 0).await
}

pub async fn delete_summary(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
    summary_id: &str,
) -> Result<usize, String> {
    let session = get_session_for_engine_message(db, session_id).await?;
    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}/summaries/{}",
        engine_base_url(config),
        urlencoding::encode(session.id.as_str()),
        urlencoding::encode(summary_id)
    );

    let response = build_client(config)?
        .delete(endpoint.as_str())
        .send()
        .await
        .map_err(|err| format!("memory engine delete summary request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine delete summary failed: status={} body={}",
            status, body
        ));
    }

    let payload = response
        .json::<EngineDeleteSummaryResponse>()
        .await
        .map_err(|err| format!("memory engine delete summary decode failed: {err}"))?;
    Ok(payload.reset_records.max(1))
}

async fn get_message_in_session(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
    message_id: &str,
) -> Result<Option<Message>, String> {
    let session = get_session_for_engine_message(db, session_id).await?;
    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}/records?tenant_id={}&source_id=memory_server&record_type=message&limit=2000&offset=0&order=asc",
        engine_base_url(config),
        urlencoding::encode(session.id.as_str()),
        urlencoding::encode(session.user_id.as_str())
    );

    let response = build_client(config)?
        .get(endpoint.as_str())
        .send()
        .await
        .map_err(|err| format!("memory engine session message scan failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine session message scan failed: status={} body={}",
            status, body
        ));
    }

    let payload = response
        .json::<EngineListRecordsResponse>()
        .await
        .map_err(|err| format!("memory engine session message scan decode failed: {err}"))?;
    Ok(payload
        .items
        .into_iter()
        .find(|record| record.id == message_id)
        .map(engine_record_to_message))
}

async fn delete_message_in_session(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
    message_id: &str,
) -> Result<bool, String> {
    let session = get_session_for_engine_message(db, session_id).await?;
    let endpoint = format!(
        "{}/api/memory-engine/v1/records/{}?tenant_id={}&source_id=memory_server",
        engine_base_url(config),
        urlencoding::encode(message_id),
        urlencoding::encode(session.user_id.as_str())
    );

    let response = build_client(config)?
        .delete(endpoint.as_str())
        .send()
        .await
        .map_err(|err| format!("memory engine delete message request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine delete message failed: status={} body={}",
            status, body
        ));
    }

    let payload = response
        .json::<EngineDeleteRecordResponse>()
        .await
        .map_err(|err| format!("memory engine delete message decode failed: {err}"))?;
    Ok(payload.deleted)
}

pub async fn sync_messages_batch(
    config: &AppConfig,
    db: &crate::db::Db,
    session_id: &str,
    created_messages: &[Message],
) -> Result<(), String> {
    let base_url = engine_base_url(config);
    if created_messages.is_empty() {
        return Ok(());
    }

    let session = sessions::get_session_by_id(db, session_id)
        .await?
        .ok_or_else(|| format!("session not found for message batch sync: {session_id}"))?;
    let tenant_id = session.user_id.trim();
    if tenant_id.is_empty() {
        return Ok(());
    }

    let request = EngineBatchSyncRecordsRequest {
        tenant_id: tenant_id.to_string(),
        source_id: "memory_server".to_string(),
        records: created_messages
            .iter()
            .map(|message| EngineUpsertRecordInput {
                id: message.id.clone(),
                external_record_id: Some(message.id.clone()),
                role: message.role.clone(),
                record_type: "message".to_string(),
                content: message.content.clone(),
                structured_payload: None,
                metadata: message.metadata.clone(),
                created_at: message.created_at.clone(),
            })
            .collect(),
    };

    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}/records/batch-sync",
        base_url,
        urlencoding::encode(session_id)
    );

    let response = build_client(config)?
        .put(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine sync batch request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine sync batch failed: status={} body={}",
            status, body
        ));
    }

    Ok(())
}

fn build_client(config: &AppConfig) -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(config.memory_engine_timeout_secs))
        .build()
        .map_err(|err| err.to_string())
}

pub async fn run_thread_summary(
    config: &AppConfig,
    tenant_id: &str,
    thread_id: &str,
) -> Result<EngineRunThreadSummaryResponse, String> {
    let base_url = engine_base_url(config);
    if tenant_id.trim().is_empty() {
        return Err("empty tenant_id".to_string());
    }

    let request = EngineRunThreadSummaryRequest {
        tenant_id: tenant_id.trim().to_string(),
        source_id: "memory_server".to_string(),
        max_records: Some(20),
    };

    let endpoint = format!(
        "{}/api/memory-engine/v1/threads/{}/summaries/run",
        base_url,
        urlencoding::encode(thread_id)
    );

    let response = build_client(config)?
        .post(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine run summary request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine run summary failed: status={} body={}",
            status, body
        ));
    }

    response
        .json::<EngineRunThreadSummaryResponse>()
        .await
        .map_err(|err| format!("memory engine run summary decode failed: {err}"))
}

pub async fn run_pending_summaries_once(
    config: &AppConfig,
    tenant_id: Option<&str>,
    max_threads: Option<i64>,
) -> Result<EngineRunPendingSummariesResponse, String> {
    let base_url = engine_base_url(config);

    let request = EngineRunPendingSummariesRequest {
        tenant_id: tenant_id.map(ToOwned::to_owned),
        source_id: Some("memory_server".to_string()),
        max_threads,
    };
    let endpoint = format!(
        "{}/api/memory-engine/v1/jobs/summaries/run-once",
        base_url
    );

    let response = build_client(config)?
        .post(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine run pending summaries request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine run pending summaries failed: status={} body={}",
            status, body
        ));
    }

    response
        .json::<EngineRunPendingSummariesResponse>()
        .await
        .map_err(|err| format!("memory engine run pending summaries decode failed: {err}"))
}

pub async fn run_pending_rollups_once(
    config: &AppConfig,
    tenant_id: &str,
    rollup_config: &SummaryRollupJobConfig,
) -> Result<EngineRunPendingRollupsResponse, String> {
    let base_url = engine_base_url(config);

    let request = EngineRunPendingRollupsRequest {
        tenant_id: Some(tenant_id.trim().to_string()),
        source_id: Some("memory_server".to_string()),
        summary_prompt: rollup_config.summary_prompt.clone(),
        max_threads: Some(rollup_config.max_sessions_per_tick.max(1)),
        round_limit: Some(rollup_config.round_limit.max(1)),
        token_limit: Some(rollup_config.token_limit.max(500)),
        target_summary_tokens: Some(rollup_config.target_summary_tokens.max(128)),
        keep_level0_count: Some(rollup_config.keep_raw_level0_count.max(0)),
        max_level: Some(rollup_config.max_level.max(1)),
    };
    let endpoint = format!(
        "{}/api/memory-engine/v1/jobs/rollups/run-once",
        base_url
    );

    let response = build_client(config)?
        .post(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine run pending rollups request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine run pending rollups failed: status={} body={}",
            status, body
        ));
    }

    response
        .json::<EngineRunPendingRollupsResponse>()
        .await
        .map_err(|err| format!("memory engine run pending rollups decode failed: {err}"))
}

pub async fn run_review_repair_scope(
    config: &AppConfig,
    tenant_id: &str,
    thread_label: &str,
    max_threads: i64,
    max_records_per_thread: usize,
) -> Result<EngineRunThreadRepairScopeResponse, String> {
    let normalized_tenant = tenant_id.trim();
    let normalized_label = thread_label.trim();
    if normalized_tenant.is_empty() {
        return Err("empty tenant_id".to_string());
    }
    if normalized_label.is_empty() {
        return Err("empty thread_label".to_string());
    }

    let request = EngineRunThreadRepairScopeRequest {
        tenant_id: normalized_tenant.to_string(),
        source_id: "memory_server".to_string(),
        thread_label: normalized_label.to_string(),
        thread_status: Some("active".to_string()),
        pending_record_type: Some("message".to_string()),
        max_threads: Some(max_threads.max(1).min(5_000)),
        max_records_per_thread: Some(max_records_per_thread.max(1)),
    };

    let endpoint = format!(
        "{}/api/memory-engine/v1/jobs/thread-repair-scope/run-once",
        engine_base_url(config)
    );

    let response = build_client(config)?
        .post(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine run review repair scope request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine run review repair scope failed: status={} body={}",
            status, body
        ));
    }

    response
        .json::<EngineRunThreadRepairScopeResponse>()
        .await
        .map_err(|err| format!("memory engine run review repair scope decode failed: {err}"))
}

pub async fn get_review_repair_scope_status(
    config: &AppConfig,
    tenant_id: &str,
    thread_label: &str,
    max_threads: i64,
) -> Result<EngineGetThreadRepairScopeStatusResponse, String> {
    let normalized_tenant = tenant_id.trim();
    let normalized_label = thread_label.trim();
    if normalized_tenant.is_empty() {
        return Err("empty tenant_id".to_string());
    }
    if normalized_label.is_empty() {
        return Err("empty thread_label".to_string());
    }

    let request = EngineGetThreadRepairScopeStatusRequest {
        tenant_id: normalized_tenant.to_string(),
        source_id: "memory_server".to_string(),
        thread_label: normalized_label.to_string(),
        thread_status: Some("active".to_string()),
        pending_record_type: Some("message".to_string()),
        max_threads: Some(max_threads.max(1).min(5_000)),
    };

    let endpoint = format!(
        "{}/api/memory-engine/v1/jobs/thread-repair-scope/status",
        engine_base_url(config)
    );

    let response = build_client(config)?
        .post(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine review repair status request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine review repair status failed: status={} body={}",
            status, body
        ));
    }

    response
        .json::<EngineGetThreadRepairScopeStatusResponse>()
        .await
        .map_err(|err| format!("memory engine review repair status decode failed: {err}"))
}

#[allow(clippy::too_many_arguments)]
pub async fn run_agent_recall_job(
    config: &AppConfig,
    tenant_id: &str,
    agent_id: &str,
    summary_prompt: Option<&str>,
    round_limit: i64,
    token_limit: i64,
    target_summary_tokens: i64,
    keep_level0_count: i64,
    max_level: i64,
) -> Result<EngineRunSubjectMemoryJobResponse, String> {
    let base_url = engine_base_url(config);
    let normalized_tenant = tenant_id.trim();
    let normalized_agent = agent_id.trim();
    if normalized_tenant.is_empty() {
        return Err("empty tenant_id".to_string());
    }
    if normalized_agent.is_empty() {
        return Err("empty agent_id".to_string());
    }

    let agent_subject_id = format!("agent:{normalized_agent}");
    let request = EngineRunSubjectMemoryJobRequest {
        tenant_id: normalized_tenant.to_string(),
        source_id: "memory_server".to_string(),
        subject_id: agent_subject_id.clone(),
        memory_type: "agent_recall".to_string(),
        source_thread_label: agent_subject_id.clone(),
        relation_subject_id: Some(agent_subject_id.clone()),
        source_summary_type: Some("thread_incremental".to_string()),
        summary_prompt: summary_prompt.map(ToOwned::to_owned),
        prompt_title: Some(format!("Agent recall {}", normalized_agent)),
        round_limit: Some(round_limit.max(1)),
        token_limit: Some(token_limit.max(500)),
        target_summary_tokens: Some(target_summary_tokens.max(128)),
        keep_level0_count: Some(keep_level0_count.max(0)),
        max_level: Some(max_level.max(1)),
        memory_metadata: Some(serde_json::json!({
            "legacy_owner": "memory_server",
            "agent_id": normalized_agent,
        })),
    };

    let endpoint = format!(
        "{}/api/memory-engine/v1/jobs/subject-memories/run-once",
        base_url
    );

    let response = build_client(config)?
        .post(endpoint.as_str())
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("memory engine run subject memory job request failed: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "memory engine run subject memory job failed: status={} body={}",
            status, body
        ));
    }

    response
        .json::<EngineRunSubjectMemoryJobResponse>()
        .await
        .map_err(|err| format!("memory engine run subject memory job decode failed: {err}"))
}
