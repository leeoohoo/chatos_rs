// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::core::chat_runtime::{
    contact_agent_id_from_metadata, contact_id_from_metadata, project_id_from_metadata,
};
use crate::models::message::Message;
use crate::models::project::PUBLIC_PROJECT_ID;
use crate::models::session::Session;
use crate::services::text_normalization::normalize_optional_text_ref;

pub const CHATOS_COMPAT_SOURCE_ID: &str = "chatos";

#[derive(Debug, Clone)]
pub struct ChatosThreadMapping {
    pub tenant_id: String,
    pub thread_id: String,
    pub subject_id: String,
    pub related_subject_ids: Vec<String>,
    pub labels: Vec<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct ChatosReviewRepairScope {
    pub project_id: String,
    pub contact_id: Option<String>,
    pub agent_id: Option<String>,
}

pub(crate) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    normalize_optional_text_ref(value)
}

pub(crate) fn resolve_session_project_scope(
    project_id: Option<&str>,
    metadata: Option<&Value>,
) -> String {
    normalize_optional_text(project_id)
        .or_else(|| project_id_from_metadata(metadata))
        .map(|value| {
            if value == "0" {
                PUBLIC_PROJECT_ID.to_string()
            } else {
                value
            }
        })
        .unwrap_or_else(|| PUBLIC_PROJECT_ID.to_string())
}

pub(crate) fn build_thread_mapping(session: &Session) -> Result<ChatosThreadMapping, String> {
    let tenant_id = session
        .user_id
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("session {} has empty user_id", session.id))?;
    let thread_id = session.id.trim().to_string();
    if thread_id.is_empty() {
        return Err("session id is required".to_string());
    }

    let metadata_ref = session.metadata.as_ref();
    let project_id = resolve_session_project_scope(session.project_id.as_deref(), metadata_ref);
    let contact_id = contact_id_from_metadata(metadata_ref);
    let agent_id = contact_agent_id_from_metadata(metadata_ref)
        .or_else(|| normalize_optional_text(session.selected_agent_id.as_deref()));

    let mut related_subject_ids = Vec::new();
    if let Some(contact_id) = contact_id.clone() {
        related_subject_ids.push(format!("contact:{contact_id}"));
    }
    if let Some(agent_id) = agent_id.clone() {
        related_subject_ids.push(format!("agent:{agent_id}"));
    }
    if !project_id.is_empty() {
        related_subject_ids.push(format!("project:{project_id}"));
        if let Some(contact_id) = contact_id.clone() {
            related_subject_ids.push(format!("contact_project:{contact_id}:{project_id}"));
        }
        if let Some(agent_id) = agent_id.clone() {
            related_subject_ids.push(format!("agent_project:{agent_id}:{project_id}"));
        }
    }

    let labels = related_subject_ids.clone();
    let metadata = json!({
        "mapping_version": "chatos_sdk.v1",
        "mapping_source": "chatos_sdk",
        "legacy_session_mapping": {
            "session_id": session.id,
            "project_id": project_id,
            "contact_id": contact_id,
            "agent_id": agent_id,
        },
        // Persist only the original Chatos-side session metadata. If we write
        // back the full engine thread metadata here, source_metadata will nest
        // recursively on every session sync and eventually become unreadable.
        "source_metadata": extract_source_metadata_for_engine(session.metadata.as_ref()),
    });

    Ok(ChatosThreadMapping {
        tenant_id,
        thread_id: thread_id.clone(),
        subject_id: format!("session:{thread_id}"),
        related_subject_ids,
        labels,
        metadata,
    })
}

fn extract_source_metadata_for_engine(metadata: Option<&Value>) -> Value {
    let Some(metadata) = metadata else {
        return Value::Null;
    };

    if let Some(source_metadata) = metadata.get("source_metadata") {
        return source_metadata.clone();
    }

    metadata.clone()
}

pub(crate) fn build_review_repair_scope(
    session: &Session,
) -> Result<ChatosReviewRepairScope, String> {
    build_thread_mapping(session)?;
    let metadata = session.metadata.as_ref();
    let project_id = resolve_session_project_scope(session.project_id.as_deref(), metadata);
    let contact_id = contact_id_from_metadata(metadata);
    let agent_id = contact_agent_id_from_metadata(metadata)
        .or_else(|| normalize_optional_text(session.selected_agent_id.as_deref()));

    Ok(ChatosReviewRepairScope {
        project_id,
        contact_id,
        agent_id,
    })
}

pub(crate) fn pack_message_metadata(message: &Message) -> Option<Value> {
    let mut map = match message.metadata.clone() {
        Some(Value::Object(obj)) => obj,
        Some(_) | None => serde_json::Map::new(),
    };

    if let Some(value) = message
        .message_mode
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        map.insert("message_mode".to_string(), Value::String(value.to_string()));
    }
    if let Some(value) = message
        .message_source
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        map.insert(
            "message_source".to_string(),
            Value::String(value.to_string()),
        );
    }
    if let Some(value) = message.tool_calls.clone() {
        map.insert("tool_calls".to_string(), value);
    }
    if let Some(value) = message
        .tool_call_id
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        map.insert("tool_call_id".to_string(), Value::String(value.to_string()));
    }
    if let Some(value) = message
        .reasoning
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        map.insert("reasoning".to_string(), Value::String(value.to_string()));
    }

    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

type MessageMetadataParts = (
    Option<String>,
    Option<String>,
    Option<Value>,
    Option<String>,
    Option<String>,
    Option<Value>,
);

pub(crate) fn unpack_message_metadata(metadata: Option<Value>) -> MessageMetadataParts {
    let Some(Value::Object(mut map)) = metadata else {
        return (
            None,
            Some("memory_engine".to_string()),
            None,
            None,
            None,
            None,
        );
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
        Some(Value::Object(map))
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
