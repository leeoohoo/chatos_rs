use memory_engine_sdk::{
    ComposeContextPolicy, CompactTurnsResponse, SdkComposeContextRequest,
    SdkGetTurnProcessRecordsRequest, SdkListCompactTurnsRequest, SdkListThreadRecordsRequest,
    SdkListThreadSummariesRequest, SdkListThreadsRequest, SdkUpsertThreadRequest,
    TurnProcessRecordsResponse,
};
use serde_json::Value;

use crate::core::chat_runtime::{contact_agent_id_from_metadata, contact_id_from_metadata};
use crate::core::time::now_rfc3339;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;

use super::client::build_client;
use super::mappers::{
    engine_record_to_message, engine_summary_to_session_summary, engine_thread_to_session,
};
use super::mapping::{build_thread_mapping, pack_message_metadata, resolve_session_project_scope};
use super::register_subject_memory_scopes;
use super::types::ComposedChatHistoryContext;
use memory_engine_sdk::{SdkBatchSyncRecordsRequest, UpsertRecordInput};

pub async fn compose_chatos_context(
    session: &Session,
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
                recent_record_limit: None,
                summary_limit: None,
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
        summary_count: resp.meta.summary_count,
        messages: resp
            .recent_records
            .into_iter()
            .map(engine_record_to_message)
            .collect(),
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
    let effective_project_id =
        resolve_session_project_scope(project_id.as_deref(), metadata.as_ref());
    if let Some(existing) = find_existing_active_chatos_session(
        user_id.as_str(),
        effective_project_id.as_str(),
        metadata.as_ref(),
    )
    .await?
    {
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
    Ok(
        update_chatos_session(session_id, None, Some("archived".to_string()), None)
            .await?
            .is_some(),
    )
}

pub async fn get_chatos_session(
    session_id: &str,
    tenant_id: Option<&str>,
) -> Result<Option<Session>, String> {
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
    let status = if include_archived || include_archiving {
        None
    } else {
        Some("active".to_string())
    };
    let mut items = client
        .list_threads(&SdkListThreadsRequest {
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
        .list_threads(&SdkListThreadsRequest {
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
                order: Some(if asc {
                    "asc".to_string()
                } else {
                    "desc".to_string()
                }),
            },
    )
    .await?;
    Ok(items.into_iter().map(engine_record_to_message).collect())
}

pub async fn list_chatos_compact_turns(
    session: &Session,
    limit: Option<i64>,
    before_turn_id: Option<&str>,
) -> Result<CompactTurnsResponse, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    client
        .list_compact_turns(
            mapping.thread_id.as_str(),
            &SdkListCompactTurnsRequest {
                tenant_id: mapping.tenant_id,
                record_type: Some("message".to_string()),
                limit,
                before_turn_id: before_turn_id.map(ToOwned::to_owned),
            },
        )
        .await
}

pub async fn get_chatos_turn_process_records(
    session: &Session,
    turn_id: &str,
) -> Result<TurnProcessRecordsResponse, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    client
        .get_turn_process_records(
            mapping.thread_id.as_str(),
            turn_id,
            &SdkGetTurnProcessRecordsRequest {
                tenant_id: mapping.tenant_id,
                record_type: Some("message".to_string()),
            },
        )
        .await
}

pub async fn get_chatos_message_by_id(message_id: &str) -> Result<Option<Message>, String> {
    let client = build_client()?;
    Ok(client
        .get_record(message_id, None)
        .await?
        .map(engine_record_to_message))
}

pub async fn upsert_chatos_message(
    session: &Session,
    message: &Message,
) -> Result<Message, String> {
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
    Ok(items
        .into_iter()
        .map(engine_summary_to_session_summary)
        .collect())
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
        .delete_thread_summary(
            mapping.thread_id.as_str(),
            summary_id,
            mapping.tenant_id.as_str(),
        )
        .await
        .map(Some)
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
            .list_threads(&SdkListThreadsRequest {
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
            .list_threads(&SdkListThreadsRequest {
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
