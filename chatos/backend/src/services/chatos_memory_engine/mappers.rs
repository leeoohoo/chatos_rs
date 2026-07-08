// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use memory_engine_sdk::{EngineSubjectMemory, EngineThreadSnapshot, ThreadSnapshotLookupResponse};
use serde_json::Value;

use crate::models::memory_mapping_types::{MemoryAgentRecallDto, MemoryProjectMemoryDto};
use crate::models::memory_runtime_types::{
    SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotDto,
    TurnRuntimeSnapshotLookupResponseDto, TurnRuntimeSnapshotRuntimeDto,
    TurnRuntimeSnapshotSystemMessageDto, TurnRuntimeSnapshotToolDto,
};
use crate::models::message::Message;
use crate::models::project::PUBLIC_PROJECT_ID;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;

use super::mapping::unpack_message_metadata;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct ChatosTurnRuntimeSnapshotPayload {
    pub system_messages: Option<Vec<TurnRuntimeSnapshotSystemMessageDto>>,
    pub tools: Option<Vec<TurnRuntimeSnapshotToolDto>>,
    pub runtime: Option<TurnRuntimeSnapshotRuntimeDto>,
}

pub fn engine_record_to_message(record: memory_engine_sdk::EngineRecord) -> Message {
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

pub(super) fn engine_summary_to_session_summary(
    item: memory_engine_sdk::EngineSummary,
) -> SessionSummaryV2 {
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

pub(super) fn engine_thread_to_session(item: memory_engine_sdk::EngineThread) -> Session {
    let title = item.title.clone().unwrap_or_else(|| "Untitled".to_string());
    let metadata = item.metadata.clone();
    let (selected_model_id, selected_agent_id) =
        extract_selection_from_engine_metadata(metadata.as_ref());
    let project_id = item
        .metadata
        .as_ref()
        .and_then(|value| value.get("legacy_session_mapping"))
        .and_then(|value| value.get("project_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value == "0" {
                PUBLIC_PROJECT_ID.to_string()
            } else {
                value.to_string()
            }
        });

    Session {
        id: item.id.clone(),
        title,
        description: None,
        metadata,
        selected_model_id,
        selected_agent_id,
        user_id: Some(item.tenant_id),
        project_id,
        message_count: 0,
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

pub(super) fn engine_subject_memory_to_project_memory(
    item: EngineSubjectMemory,
) -> MemoryProjectMemoryDto {
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
        .map(|value| {
            if value == "0" {
                PUBLIC_PROJECT_ID.to_string()
            } else {
                value
            }
        })
        .unwrap_or_else(|| PUBLIC_PROJECT_ID.to_string());

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

pub(super) fn engine_subject_memory_to_agent_recall(
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

pub(super) fn build_chatos_turn_snapshot_payload_value(
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

pub(super) fn engine_lookup_to_turn_snapshot_lookup(
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

pub(super) fn engine_snapshot_to_turn_snapshot(
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
