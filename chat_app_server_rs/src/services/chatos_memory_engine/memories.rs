use memory_engine_sdk::{
    EngineSubjectMemory, QuerySubjectMemoriesRequest, UpsertSubjectMemoryScopeRequest,
};
use serde_json::json;

use crate::core::chat_runtime::{contact_agent_id_from_metadata, contact_id_from_metadata};
use crate::models::memory_mapping_types::{MemoryAgentRecallDto, MemoryProjectMemoryDto};
use crate::models::session::Session;

use super::client::build_client;
use super::mappers::{
    engine_subject_memory_to_agent_recall, engine_subject_memory_to_project_memory,
};
use super::mapping::{CHATOS_COMPAT_SOURCE_ID, resolve_session_project_scope};
use super::normalize_non_empty;

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
        Some(format!(
            "project_memory:contact:{contact_id}:{normalized_project_id}"
        )),
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
        let mut rows =
            list_contact_project_memories(user_id, contact_id, project_id.as_str(), limit, 0)
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

pub(super) async fn register_subject_memory_scopes(
    client: &memory_engine_sdk::MemoryEngineClient,
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
