mod mapping;

use std::time::Duration;

use memory_engine_sdk::{
    ComposeContextPolicy, EngineSubjectMemory, EngineThreadSnapshot, MemoryEngineClient,
    ListJobRunsRequest,
    QuerySubjectMemoriesRequest, SdkBatchSyncRecordsRequest, SdkComposeContextRequest,
    SdkCountThreadRecordsRequest,
    SdkGetThreadRepairScopeStatusRequest, SdkListThreadRecordsRequest,
    SdkListThreadSummariesRequest, SdkUpsertThreadRequest,
    SdkUpsertThreadSnapshotRequest, ThreadSnapshotLookupResponse, UpsertRecordInput,
    UpsertSubjectMemoryScopeRequest,
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::chat_runtime::{contact_agent_id_from_metadata, contact_id_from_metadata};
use crate::models::memory_mapping_types::{MemoryAgentRecallDto, MemoryProjectMemoryDto};
use crate::models::memory_runtime_types::{
    SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotDto,
    TurnRuntimeSnapshotLookupResponseDto, TurnRuntimeSnapshotRuntimeDto,
    TurnRuntimeSnapshotSystemMessageDto, TurnRuntimeSnapshotToolDto,
};
use crate::core::time::now_rfc3339;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;

use self::mapping::{
    build_review_repair_scope, build_thread_mapping, pack_message_metadata, resolve_session_project_scope,
    unpack_message_metadata,
};

pub use self::mapping::CHATOS_COMPAT_SOURCE_ID;

const CHATOS_TURN_RUNTIME_SNAPSHOT_TYPE: &str = "turn_runtime";

#[derive(Debug, Clone, Default)]
pub struct ComposedChatHistoryContext {
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone)]
pub struct ChatosReviewRepairRequest {
    pub session: Session,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewRepairSummaryRunResult {
    pub processed_sessions: usize,
    pub summarized_sessions: usize,
    pub generated_summaries: usize,
    pub marked_messages: usize,
    pub failed_sessions: usize,
    pub pending_message_count: i64,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewRepairStatusResult {
    pub running: bool,
    pub running_job_count: i64,
    pub pending_message_count: i64,
    pub scope_session_count: usize,
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
    pub job_type: String,
}

pub async fn compose_chatos_context(
    session: &Session,
    summary_limit: usize,
    include_raw_messages: bool,
) -> Result<ComposedChatHistoryContext, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let resp = client
        .compose_context(&SdkComposeContextRequest {
            tenant_id: mapping.tenant_id,
            subject_id: Some(mapping.subject_id),
            related_subject_ids: if mapping.related_subject_ids.is_empty() {
                None
            } else {
                Some(mapping.related_subject_ids)
            },
            thread_id: mapping.thread_id,
            policy: Some(ComposeContextPolicy {
                include_recent_records: Some(include_raw_messages),
                include_thread_summary: Some(true),
                include_subject_memory: Some(true),
                recent_record_limit: Some(summary_limit.max(1)),
                summary_limit: Some(summary_limit.max(1)),
            }),
        })
        .await?;

    let merged_summary = if resp.blocks.is_empty() {
        None
    } else {
        Some(
            resp.blocks
                .iter()
                .map(|block| format!("[{}]\n{}", block.block_type, block.text))
                .collect::<Vec<_>>()
                .join("\n\n===\n\n"),
        )
    };

    Ok(ComposedChatHistoryContext {
        merged_summary,
        summary_count: resp.blocks.len(),
        messages: resp.recent_records.into_iter().map(engine_record_to_message).collect(),
    })
}

pub async fn sync_chatos_session(session: &Session) -> Result<(), String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    client
        .upsert_thread(
            mapping.thread_id.as_str(),
            &SdkUpsertThreadRequest {
                tenant_id: mapping.tenant_id,
                subject_id: mapping.subject_id,
                thread_type: "chat".to_string(),
                external_thread_id: Some(session.id.clone()),
                title: Some(session.title.clone()),
                labels: if mapping.labels.is_empty() {
                    None
                } else {
                    Some(mapping.labels)
                },
                metadata: Some(mapping.metadata),
                status: Some(session.status.clone()),
                created_at: Some(session.created_at.clone()),
                updated_at: Some(session.updated_at.clone()),
                archived_at: session.archived_at.clone(),
            },
        )
        .await
        .map(|_| ())?;
    register_subject_memory_scopes(&client, session).await
}

pub async fn create_chatos_session(
    user_id: String,
    title: String,
    project_id: Option<String>,
    metadata: Option<Value>,
) -> Result<Session, String> {
    let effective_project_id = resolve_session_project_scope(project_id.as_deref(), metadata.as_ref());
    if let Some(existing) = find_existing_active_chatos_session(
        user_id.as_str(),
        effective_project_id.as_str(),
        metadata.as_ref(),
    )
    .await? {
        return Ok(existing);
    }

    let normalized_project_id = if effective_project_id == "0" {
        None
    } else {
        Some(effective_project_id)
    };
    let session = Session::new(title, None, metadata, Some(user_id), normalized_project_id);
    sync_chatos_session(&session).await?;
    Ok(session)
}

pub async fn update_chatos_session(
    session_id: &str,
    title: Option<String>,
    status: Option<String>,
    metadata: Option<Value>,
) -> Result<Option<Session>, String> {
    let Some(current) = get_chatos_session(session_id, None).await? else {
        return Ok(None);
    };

    let merged_title = title.unwrap_or(current.title.clone());
    let merged_metadata = metadata.or(current.metadata.clone());
    let mut updated = Session::new(
        merged_title,
        None,
        merged_metadata,
        current.user_id.clone(),
        current.project_id.clone(),
    );
    updated.id = current.id.clone();
    updated.created_at = current.created_at.clone();
    updated.updated_at = now_rfc3339();
    updated.status = status.unwrap_or(current.status.clone());
    updated.archived_at = if updated.status == "archived" {
        Some(updated.updated_at.clone())
    } else {
        current.archived_at.clone()
    };

    sync_chatos_session(&updated).await?;
    Ok(Some(updated))
}

pub async fn archive_chatos_session(session_id: &str) -> Result<bool, String> {
    Ok(update_chatos_session(
        session_id,
        None,
        Some("archived".to_string()),
        None,
    )
    .await?
    .is_some())
}

pub async fn get_chatos_session(session_id: &str, tenant_id: Option<&str>) -> Result<Option<Session>, String> {
    let client = build_client()?;
    let item = client.get_thread(session_id, tenant_id).await?;
    Ok(item.map(engine_thread_to_session))
}

pub async fn list_chatos_sessions(
    tenant_id: &str,
    project_id: Option<&str>,
    limit: Option<i64>,
    offset: i64,
    include_archived: bool,
    include_archiving: bool,
) -> Result<Vec<Session>, String> {
    let client = build_client()?;
    let status = if include_archived {
        None
    } else if include_archiving {
        None
    } else {
        Some("active".to_string())
    };
    let mut items = client
        .list_threads(&memory_engine_sdk::SdkListThreadsRequest {
            tenant_id: tenant_id.to_string(),
            subject_id: None,
            external_thread_id: None,
            session_id: None,
            contact_id: None,
            project_id: project_id.map(ToOwned::to_owned),
            agent_id: None,
            mapping_source: Some("chatos_sdk".to_string()),
            mapping_version: None,
            thread_label: None,
            status,
            limit,
            offset: Some(offset),
        })
        .await?;
    if include_archiving && !include_archived {
        items.retain(|thread| thread.status != "archived");
    }
    Ok(items.into_iter().map(engine_thread_to_session).collect())
}

pub async fn list_chatos_sessions_by_agent(
    tenant_id: &str,
    agent_id: &str,
    project_id: Option<&str>,
    status: Option<&str>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<Session>, String> {
    let client = build_client()?;
    let items = client
        .list_threads(&memory_engine_sdk::SdkListThreadsRequest {
            tenant_id: tenant_id.to_string(),
            subject_id: None,
            external_thread_id: None,
            session_id: None,
            contact_id: None,
            project_id: project_id.map(ToOwned::to_owned),
            agent_id: Some(agent_id.to_string()),
            mapping_source: Some("chatos_sdk".to_string()),
            mapping_version: None,
            thread_label: None,
            status: status.map(ToOwned::to_owned),
            limit,
            offset: Some(offset),
        })
        .await?;
    Ok(items.into_iter().map(engine_thread_to_session).collect())
}

pub async fn list_chatos_messages(
    session: &Session,
    limit: Option<i64>,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let items = client
        .list_thread_records(
            mapping.thread_id.as_str(),
            &SdkListThreadRecordsRequest {
                tenant_id: mapping.tenant_id,
                role: None,
                record_type: Some("message".to_string()),
                summary_status: None,
                limit,
                offset: Some(offset),
                order: Some(if asc { "asc".to_string() } else { "desc".to_string() }),
            },
        )
        .await?;
    Ok(items.into_iter().map(engine_record_to_message).collect())
}

pub async fn get_chatos_message_by_id(message_id: &str) -> Result<Option<Message>, String> {
    let client = build_client()?;
    Ok(client
        .get_record(message_id, None)
        .await?
        .map(engine_record_to_message))
}

pub async fn upsert_chatos_message(session: &Session, message: &Message) -> Result<Message, String> {
    sync_chatos_session(session).await?;
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    client
        .batch_sync_records(
            mapping.thread_id.as_str(),
            &SdkBatchSyncRecordsRequest {
                tenant_id: mapping.tenant_id,
                records: vec![UpsertRecordInput {
                    id: message.id.clone(),
                    external_record_id: None,
                    role: message.role.clone(),
                    record_type: "message".to_string(),
                    content: message.content.clone(),
                    structured_payload: None,
                    metadata: pack_message_metadata(message),
                    summary_status: Some(message.summary_status.clone()),
                    summary_id: message.summary_id.clone(),
                    summarized_at: message.summarized_at.clone(),
                    created_at: message.created_at.clone(),
                }],
            },
        )
        .await?;
    Ok(message.clone())
}

pub async fn delete_chatos_message_by_id(message_id: &str) -> Result<bool, String> {
    let client = build_client()?;
    client.delete_record(message_id, None).await
}

pub async fn delete_all_chatos_messages(session: &Session) -> Result<i64, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    client
        .delete_thread_records(
            mapping.thread_id.as_str(),
            mapping.tenant_id.as_str(),
            Some("message"),
        )
        .await
}

pub async fn list_chatos_summaries(
    session: &Session,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<SessionSummaryV2>, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let items = client
        .list_thread_summaries(
            mapping.thread_id.as_str(),
            &SdkListThreadSummariesRequest {
                tenant_id: mapping.tenant_id,
                summary_type: None,
                status: None,
                level: None,
                limit,
                offset: Some(offset),
            },
        )
        .await?;
    Ok(items.into_iter().map(engine_summary_to_session_summary).collect())
}

pub async fn delete_chatos_summary(
    session: &Session,
    summary_id: &str,
) -> Result<Option<usize>, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let summaries = client
        .list_thread_summaries(
            mapping.thread_id.as_str(),
            &SdkListThreadSummariesRequest {
                tenant_id: mapping.tenant_id.clone(),
                summary_type: None,
                status: None,
                level: None,
                limit: Some(5_000),
                offset: Some(0),
            },
        )
        .await?;
    if !summaries.iter().any(|item| item.id == summary_id) {
        return Ok(None);
    }
    client
        .delete_thread_summary(mapping.thread_id.as_str(), summary_id, mapping.tenant_id.as_str())
        .await
        .map(Some)
}

pub async fn run_chatos_review_repair(
    req: &ChatosReviewRepairRequest,
) -> Result<ReviewRepairSummaryRunResult, String> {
    let scope = build_review_repair_scope(&req.session)?;
    let mapping = build_thread_mapping(&req.session)?;
    let client = build_client()?;
    let pending_message_count = client
        .count_thread_records(
            mapping.thread_id.as_str(),
            &SdkCountThreadRecordsRequest {
                tenant_id: mapping.tenant_id.clone(),
                role: None,
                record_type: Some("message".to_string()),
                summary_status: Some("pending".to_string()),
            },
        )
        .await?;
    if pending_message_count <= 0 {
        return Ok(ReviewRepairSummaryRunResult {
            processed_sessions: 0,
            summarized_sessions: 0,
            generated_summaries: 0,
            marked_messages: 0,
            failed_sessions: 0,
            pending_message_count: 0,
            project_id: scope.project_id,
            contact_id: scope.contact_id,
            agent_id: scope.agent_id,
            mode: "review_repair".to_string(),
        });
    }

    let resp = client
        .run_thread_repair_summary(
            mapping.thread_id.as_str(),
            mapping.tenant_id.as_str(),
        )
        .await?;

    Ok(ReviewRepairSummaryRunResult {
        processed_sessions: 1,
        summarized_sessions: usize::from(resp.generated),
        generated_summaries: usize::from(resp.generated),
        marked_messages: if resp.generated {
            resp.source_record_count
        } else {
            0
        },
        failed_sessions: 0,
        pending_message_count,
        project_id: scope.project_id,
        contact_id: scope.contact_id,
        agent_id: scope.agent_id,
        mode: "review_repair".to_string(),
    })
}

pub async fn get_chatos_review_repair_status(
    req: &ChatosReviewRepairRequest,
) -> Result<ReviewRepairStatusResult, String> {
    let scope = build_review_repair_scope(&req.session)?;
    let mapping = build_thread_mapping(&req.session)?;
    let client = build_client()?;
    let pending_message_count = client
        .count_thread_records(
            mapping.thread_id.as_str(),
            &SdkCountThreadRecordsRequest {
                tenant_id: mapping.tenant_id.clone(),
                role: None,
                record_type: Some("message".to_string()),
                summary_status: Some("pending".to_string()),
            },
        )
        .await?;

    let running_job_count = match client
        .list_job_runs(&ListJobRunsRequest {
            job_type: Some("thread_repair".to_string()),
            thread_id: Some(mapping.thread_id.clone()),
            status: Some("running".to_string()),
            tenant_id: Some(mapping.tenant_id.clone()),
            source_id: Some(CHATOS_COMPAT_SOURCE_ID.to_string()),
            limit: Some(10),
        })
        .await
    {
        Ok(items) => items.len() as i64,
        Err(_) => {
            if let Some(thread_label) = scope.thread_label.clone() {
                client
                    .get_thread_repair_scope_status(&SdkGetThreadRepairScopeStatusRequest {
                        tenant_id: scope.tenant_id.clone(),
                        thread_label,
                        thread_status: Some("active".to_string()),
                        pending_record_type: Some("message".to_string()),
                        max_threads: Some(5_000),
                    })
                    .await
                    .map(|value| value.running_job_count)
                    .unwrap_or(0)
            } else {
                0
            }
        }
    };

    Ok(ReviewRepairStatusResult {
        running: running_job_count > 0,
        running_job_count,
        pending_message_count,
        scope_session_count: usize::from(pending_message_count > 0),
        project_id: scope.project_id,
        contact_id: scope.contact_id,
        agent_id: scope.agent_id,
        job_type: "memory_engine_thread_repair".to_string(),
    })
}

pub async fn sync_chatos_turn_runtime_snapshot(
    session: &Session,
    turn_id: &str,
    payload: &SyncTurnRuntimeSnapshotRequestDto,
) -> Result<TurnRuntimeSnapshotDto, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let payload_value = build_chatos_turn_snapshot_payload_value(payload)?;
    let metadata = Some(json!({
        "subsystem": "chatos",
        "resource_type": "turn_runtime_snapshot",
        "schema_version": "chatos.turn_runtime_snapshot.v1",
    }));

    let resp = client
        .upsert_thread_snapshot(
            mapping.thread_id.as_str(),
            CHATOS_TURN_RUNTIME_SNAPSHOT_TYPE,
            turn_id,
            &SdkUpsertThreadSnapshotRequest {
                tenant_id: mapping.tenant_id,
                user_message_id: payload.user_message_id.clone(),
                status: payload.status.clone(),
                snapshot_source: payload.snapshot_source.clone(),
                snapshot_version: payload.snapshot_version,
                payload: payload_value,
                metadata,
                captured_at: payload.captured_at.clone(),
            },
        )
        .await?;

    engine_snapshot_to_turn_snapshot(resp)
}

pub async fn get_latest_chatos_turn_runtime_snapshot(
    session: &Session,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let resp = client
        .get_latest_thread_snapshot(
            mapping.thread_id.as_str(),
            CHATOS_TURN_RUNTIME_SNAPSHOT_TYPE,
            mapping.tenant_id.as_str(),
        )
        .await?;
    engine_lookup_to_turn_snapshot_lookup(resp)
}

pub async fn get_chatos_turn_runtime_snapshot_by_turn(
    session: &Session,
    turn_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let resp = client
        .get_thread_snapshot_by_turn(
            mapping.thread_id.as_str(),
            CHATOS_TURN_RUNTIME_SNAPSHOT_TYPE,
            turn_id,
            mapping.tenant_id.as_str(),
        )
        .await?;
    engine_lookup_to_turn_snapshot_lookup(resp)
}

pub async fn list_contact_project_memories(
    user_id: &str,
    contact_id: &str,
    project_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectMemoryDto>, String> {
    let normalized_project_id =
        normalize_non_empty(Some(project_id)).unwrap_or_else(|| "0".to_string());
    let subject_id = format!("contact_project:{contact_id}:{normalized_project_id}");
    let items = query_project_memories(
        user_id,
        subject_id.as_str(),
        Some(format!("project_memory:contact:{contact_id}:{normalized_project_id}")),
        limit,
        offset,
    )
    .await?;
    Ok(items
        .into_iter()
        .map(engine_subject_memory_to_project_memory)
        .collect())
}

pub async fn list_contact_project_memories_by_contact(
    user_id: &str,
    contact_id: &str,
    project_ids: &[String],
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryProjectMemoryDto>, String> {
    let mut items = Vec::new();
    for project_id in project_ids {
        let mut rows = list_contact_project_memories(
            user_id,
            contact_id,
            project_id.as_str(),
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
        .take(limit.unwrap_or(100).max(1).min(1000) as usize)
        .collect())
}

pub async fn list_contact_agent_recalls(
    user_id: &str,
    agent_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryAgentRecallDto>, String> {
    let client = build_client()?;
    let items = client
        .query_subject_memories(&QuerySubjectMemoriesRequest {
            tenant_id: user_id.to_string(),
            source_id: CHATOS_COMPAT_SOURCE_ID.to_string(),
            subject_id: format!("agent:{agent_id}"),
            memory_type: Some("agent_recall".to_string()),
            level: None,
            max_level_exclusive: None,
            rollup_status: None,
            relation_subject_id: None,
            source_digest: None,
            limit,
            offset: Some(offset),
        })
        .await?;
    Ok(items
        .into_iter()
        .map(|item| engine_subject_memory_to_agent_recall(item, agent_id))
        .collect())
}

fn build_client() -> Result<MemoryEngineClient, String> {
    let cfg = Config::try_get()?;
    let timeout = Duration::from_millis(cfg.memory_engine_request_timeout_ms.max(300) as u64);
    MemoryEngineClient::new_direct(
        cfg.memory_engine_base_url.clone(),
        timeout,
        CHATOS_COMPAT_SOURCE_ID.to_string(),
    )
}

async fn register_subject_memory_scopes(
    client: &MemoryEngineClient,
    session: &Session,
) -> Result<(), String> {
    let tenant_id = session
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("session {} has empty user_id", session.id))?;
    let metadata_ref = session.metadata.as_ref();
    let agent_id = contact_agent_id_from_metadata(metadata_ref)
        .or_else(|| normalize_non_empty(session.selected_agent_id.as_deref()));
    let contact_id = contact_id_from_metadata(metadata_ref);
    let project_id = resolve_session_project_scope(session.project_id.as_deref(), metadata_ref);

    if let Some(agent_id_value) = agent_id.clone() {
        let agent_subject_id = format!("agent:{agent_id_value}");
        client
            .upsert_subject_memory_scope(
                format!("agent_recall:{agent_id_value}").as_str(),
                &UpsertSubjectMemoryScopeRequest {
                    tenant_id: tenant_id.to_string(),
                    source_id: CHATOS_COMPAT_SOURCE_ID.to_string(),
                    subject_id: agent_subject_id.clone(),
                    memory_type: "agent_recall".to_string(),
                    source_thread_label: agent_subject_id.clone(),
                    relation_subject_id: Some(agent_subject_id),
                    source_summary_type: Some("thread_incremental".to_string()),
                    prompt_title: Some(format!("Agent recall {agent_id_value}")),
                    memory_metadata: Some(json!({
                        "legacy_owner": "chatos",
                        "scope_type": "agent_recall",
                        "agent_id": agent_id_value,
                        "project_id": project_id,
                    })),
                    status: Some("active".to_string()),
                },
            )
            .await?;
    }

    for (subject_id, relation_subject_id) in build_project_memory_scope_targets(session) {
        client
            .upsert_subject_memory_scope(
                format!("project_memory:{subject_id}").as_str(),
                &UpsertSubjectMemoryScopeRequest {
                    tenant_id: tenant_id.to_string(),
                    source_id: CHATOS_COMPAT_SOURCE_ID.to_string(),
                    subject_id: subject_id.clone(),
                    memory_type: "project_memory".to_string(),
                    source_thread_label: subject_id.clone(),
                    relation_subject_id: Some(relation_subject_id.clone()),
                    source_summary_type: Some("thread_incremental".to_string()),
                    prompt_title: Some(format!("Project memory {subject_id}")),
                    memory_metadata: Some(json!({
                        "legacy_owner": "chatos",
                        "scope_type": "project_memory",
                        "legacy_session_mapping": {
                            "session_id": session.id,
                            "project_id": project_id.clone(),
                            "contact_id": contact_id.clone(),
                            "agent_id": agent_id.clone(),
                        },
                        "project_id": project_id.clone(),
                        "contact_id": contact_id.clone(),
                        "agent_id": agent_id.clone(),
                        "relation_subject_id": relation_subject_id,
                    })),
                    status: Some("active".to_string()),
                },
            )
            .await?;
    }

    Ok(())
}

fn build_project_memory_scope_targets(session: &Session) -> Vec<(String, String)> {
    let metadata_ref = session.metadata.as_ref();
    let project_id = resolve_session_project_scope(session.project_id.as_deref(), metadata_ref);
    let contact_id = contact_id_from_metadata(metadata_ref);
    let agent_id = contact_agent_id_from_metadata(metadata_ref)
        .or_else(|| normalize_non_empty(session.selected_agent_id.as_deref()));

    let mut out = Vec::new();
    if let Some(contact_id) = contact_id.as_deref() {
        out.push((
            format!("contact_project:{contact_id}:{project_id}"),
            format!("project_memory:contact:{contact_id}:{project_id}"),
        ));
    }
    if let Some(agent_id) = agent_id.as_deref() {
        out.push((
            format!("agent_project:{agent_id}:{project_id}"),
            format!("project_memory:agent:{agent_id}:{project_id}"),
        ));
    }
    out.push((
        format!("project:{project_id}"),
        format!("project_memory:project:{project_id}"),
    ));
    out
}

async fn query_project_memories(
    user_id: &str,
    subject_id: &str,
    relation_subject_id: Option<String>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<EngineSubjectMemory>, String> {
    let client = build_client()?;
    client
        .query_subject_memories(&QuerySubjectMemoriesRequest {
            tenant_id: user_id.to_string(),
            source_id: CHATOS_COMPAT_SOURCE_ID.to_string(),
            subject_id: subject_id.to_string(),
            memory_type: Some("project_memory".to_string()),
            level: None,
            max_level_exclusive: None,
            rollup_status: None,
            relation_subject_id,
            source_digest: None,
            limit,
            offset: Some(offset),
        })
        .await
}

fn engine_subject_memory_to_project_memory(item: EngineSubjectMemory) -> MemoryProjectMemoryDto {
    let mapping = item
        .metadata
        .as_ref()
        .and_then(|value| value.get("legacy_session_mapping"));
    let contact_id = mapping
        .and_then(|value| value.get("contact_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let agent_id = mapping
        .and_then(|value| value.get("agent_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let project_id = mapping
        .and_then(|value| value.get("project_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| project_id_from_subject_id(item.subject_id.as_str()))
        .unwrap_or_else(|| "0".to_string());

    MemoryProjectMemoryDto {
        id: item.id,
        user_id: item.tenant_id,
        contact_id,
        agent_id,
        project_id,
        memory_text: item.text,
        memory_version: 1,
        last_source_at: item.last_seen_at,
        updated_at: item.updated_at,
    }
}

fn engine_subject_memory_to_agent_recall(
    item: EngineSubjectMemory,
    agent_id: &str,
) -> MemoryAgentRecallDto {
    MemoryAgentRecallDto {
        id: item.id,
        user_id: item.tenant_id,
        agent_id: agent_id.to_string(),
        recall_key: item.memory_key,
        recall_text: item.text,
        level: item.level,
        confidence: item.confidence,
        last_seen_at: item.last_seen_at,
        updated_at: item.updated_at,
    }
}

fn project_id_from_subject_id(subject_id: &str) -> Option<String> {
    subject_id
        .split("contact_project:")
        .nth(1)
        .or_else(|| subject_id.split("agent_project:").nth(1))
        .map(|tail| tail.rsplit(':').next().unwrap_or_default())
        .or_else(|| {
            subject_id
                .strip_prefix("project:")
                .map(|value| value.trim())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn find_existing_active_chatos_session(
    user_id: &str,
    project_scope: &str,
    metadata: Option<&Value>,
) -> Result<Option<Session>, String> {
    let contact_id = contact_id_from_metadata(metadata);
    let agent_id = contact_agent_id_from_metadata(metadata);
    if contact_id.is_none() && agent_id.is_none() {
        return Ok(None);
    }

    let client = build_client()?;
    if let Some(contact_id) = contact_id.as_deref() {
        let items = client
            .list_threads(&memory_engine_sdk::SdkListThreadsRequest {
                tenant_id: user_id.to_string(),
                subject_id: None,
                external_thread_id: None,
                session_id: None,
                contact_id: Some(contact_id.to_string()),
                project_id: Some(project_scope.to_string()),
                agent_id: None,
                mapping_source: Some("chatos_sdk".to_string()),
                mapping_version: None,
                thread_label: None,
                status: Some("active".to_string()),
                limit: Some(1),
                offset: Some(0),
            })
            .await?;
        if let Some(item) = items.into_iter().next() {
            return Ok(Some(engine_thread_to_session(item)));
        }
    }

    if let Some(agent_id) = agent_id.as_deref() {
        let items = client
            .list_threads(&memory_engine_sdk::SdkListThreadsRequest {
                tenant_id: user_id.to_string(),
                subject_id: None,
                external_thread_id: None,
                session_id: None,
                contact_id: None,
                project_id: Some(project_scope.to_string()),
                agent_id: Some(agent_id.to_string()),
                mapping_source: Some("chatos_sdk".to_string()),
                mapping_version: None,
                thread_label: None,
                status: Some("active".to_string()),
                limit: Some(1),
                offset: Some(0),
            })
            .await?;
        if let Some(item) = items.into_iter().next() {
            return Ok(Some(engine_thread_to_session(item)));
        }
    }

    Ok(None)
}

fn engine_record_to_message(record: memory_engine_sdk::EngineRecord) -> Message {
    let (message_mode, message_source, tool_calls, tool_call_id, reasoning, metadata) =
        unpack_message_metadata(record.metadata);

    Message {
        id: record.id,
        session_id: record.thread_id,
        role: record.role,
        content: record.content,
        message_mode,
        message_source,
        summary: None,
        tool_calls,
        tool_call_id,
        reasoning,
        metadata,
        summary_status: record.summary_status,
        summary_id: record.summary_id,
        summarized_at: record.summarized_at,
        created_at: record.created_at,
    }
}

fn engine_summary_to_session_summary(item: memory_engine_sdk::EngineSummary) -> SessionSummaryV2 {
    SessionSummaryV2 {
        id: item.id,
        session_id: item.thread_id,
        summary_text: item.summary_text,
        summary_model: item
            .metadata
            .as_ref()
            .and_then(|value| value.get("model"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("memory_engine")
            .to_string(),
        trigger_type: item.summary_type,
        source_start_message_id: item.source_record_start_id,
        source_end_message_id: item.source_record_end_id,
        source_message_count: item.source_record_count,
        source_estimated_tokens: item.source_record_count.max(0),
        status: item.status,
        error_message: None,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

fn engine_thread_to_session(item: memory_engine_sdk::EngineThread) -> Session {
    let title = item
        .title
        .clone()
        .unwrap_or_else(|| "Untitled".to_string());
    let metadata = item.metadata.clone();
    let (selected_model_id, selected_agent_id) = extract_selection_from_engine_metadata(metadata.as_ref());
    let project_id = item
        .metadata
        .as_ref()
        .and_then(|value| value.get("legacy_session_mapping"))
        .and_then(|value| value.get("project_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .filter(|value| value != "0");

    Session {
        id: item.id.clone(),
        title,
        description: None,
        metadata,
        selected_model_id,
        selected_agent_id,
        user_id: Some(item.tenant_id),
        project_id,
        status: item.status.clone(),
        archived_at: item.archived_at.or_else(|| {
            if item.status == "archived" {
                Some(item.updated_at.clone())
            } else {
                None
            }
        }),
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

fn extract_selection_from_engine_metadata(
    metadata: Option<&Value>,
) -> (Option<String>, Option<String>) {
    let Some(Value::Object(metadata_map)) = metadata else {
        return (None, None);
    };

    let selected_model_id = metadata_map
        .get("source_metadata")
        .and_then(|value| value.get("chat_runtime"))
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
                .get("source_metadata")
                .and_then(|value| value.get("ui_chat_selection"))
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
        .get("legacy_session_mapping")
        .and_then(|value| value.get("agent_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            metadata_map
                .get("source_metadata")
                .and_then(|value| value.get("contact"))
                .and_then(Value::as_object)
                .and_then(|contact| contact.get("agent_id").or_else(|| contact.get("agentId")))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            metadata_map
                .get("source_metadata")
                .and_then(|value| value.get("ui_chat_selection"))
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

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct ChatosTurnRuntimeSnapshotPayload {
    pub system_messages: Option<Vec<TurnRuntimeSnapshotSystemMessageDto>>,
    pub tools: Option<Vec<TurnRuntimeSnapshotToolDto>>,
    pub runtime: Option<TurnRuntimeSnapshotRuntimeDto>,
}

fn build_chatos_turn_snapshot_payload_value(
    payload: &SyncTurnRuntimeSnapshotRequestDto,
) -> Result<Option<Value>, String> {
    if payload.system_messages.is_none() && payload.tools.is_none() && payload.runtime.is_none() {
        return Ok(None);
    }
    serde_json::to_value(ChatosTurnRuntimeSnapshotPayload {
        system_messages: payload.system_messages.clone(),
        tools: payload.tools.clone(),
        runtime: payload.runtime.clone(),
    })
    .map(Some)
    .map_err(|err| err.to_string())
}

fn engine_lookup_to_turn_snapshot_lookup(
    lookup: ThreadSnapshotLookupResponse,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, String> {
    Ok(TurnRuntimeSnapshotLookupResponseDto {
        session_id: lookup.thread_id,
        turn_id: lookup.turn_id,
        status: lookup.status,
        snapshot_source: lookup.snapshot_source,
        snapshot: match lookup.snapshot {
            Some(snapshot) => Some(engine_snapshot_to_turn_snapshot(snapshot)?),
            None => None,
        },
    })
}

fn engine_snapshot_to_turn_snapshot(
    snapshot: EngineThreadSnapshot,
) -> Result<TurnRuntimeSnapshotDto, String> {
    let payload = match snapshot.payload {
        Some(value) => serde_json::from_value::<ChatosTurnRuntimeSnapshotPayload>(value)
            .map_err(|err| err.to_string())?,
        None => ChatosTurnRuntimeSnapshotPayload::default(),
    };

    Ok(TurnRuntimeSnapshotDto {
        id: snapshot.id,
        session_id: snapshot.thread_id,
        user_id: snapshot.tenant_id,
        turn_id: snapshot.turn_id,
        user_message_id: snapshot.user_message_id,
        status: snapshot.status,
        snapshot_source: snapshot.snapshot_source,
        snapshot_version: snapshot.snapshot_version,
        captured_at: snapshot.captured_at,
        updated_at: snapshot.updated_at,
        system_messages: payload.system_messages.unwrap_or_default(),
        tools: payload.tools.unwrap_or_default(),
        runtime: payload.runtime,
    })
}
