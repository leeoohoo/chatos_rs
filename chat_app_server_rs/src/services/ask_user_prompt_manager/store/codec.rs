use mongodb::bson::{doc, Bson, Document};
use serde_json::json;

use crate::services::ask_user_prompt_manager::normalizer::trimmed_non_empty;
use crate::services::ask_user_prompt_manager::types::{AskUserPromptRecord, AskUserPromptStatus};

pub(super) fn ask_user_prompt_record_to_doc(record: &AskUserPromptRecord) -> Document {
    let prompt_json = serde_json::to_string(&record.prompt).unwrap_or_else(|_| "{}".to_string());
    let response_json = record
        .response
        .as_ref()
        .and_then(|value| serde_json::to_string(value).ok());

    let mut out = doc! {
        "id": record.id.clone(),
        "conversation_id": record.conversation_id.clone(),
        "conversation_turn_id": record.conversation_turn_id.clone(),
        "kind": record.kind.clone(),
        "status": record.status.as_str(),
        "prompt_json": prompt_json,
        "source": record.source.clone(),
        "created_at": record.created_at.clone(),
        "updated_at": record.updated_at.clone(),
    };
    if let Some(value) = record.tool_call_id.clone() {
        out.insert("tool_call_id", Bson::String(value));
    }
    if let Some(value) = response_json {
        out.insert("response_json", Bson::String(value));
    }
    if let Some(value) = record.expires_at.clone() {
        out.insert("expires_at", Bson::String(value));
    }
    if let Some(value) = record.external_prompt_id.clone() {
        out.insert("external_prompt_id", Bson::String(value));
    }
    if let Some(value) = record.external_task_id.clone() {
        out.insert("external_task_id", Bson::String(value));
    }
    if let Some(value) = record.external_run_id.clone() {
        out.insert("external_run_id", Bson::String(value));
    }
    if let Some(value) = record.external_project_id.clone() {
        out.insert("external_project_id", Bson::String(value));
    }
    out
}

pub(super) fn ask_user_prompt_record_from_doc(doc: &Document) -> Option<AskUserPromptRecord> {
    let id = doc.get_str("id").ok()?.to_string();
    let conversation_id = doc.get_str("conversation_id").ok()?.to_string();
    let conversation_turn_id = doc.get_str("conversation_turn_id").ok()?.to_string();
    let kind = doc.get_str("kind").ok().unwrap_or_default().to_string();
    let status = parse_status(doc.get_str("status").ok().unwrap_or("pending"));
    let prompt = doc
        .get_str("prompt_json")
        .ok()
        .map(parse_json_or_default)
        .unwrap_or_else(|| json!({}));
    let response = doc.get_str("response_json").ok().map(parse_json_or_default);
    let tool_call_id = doc
        .get_str("tool_call_id")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let expires_at = doc
        .get_str("expires_at")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let source = doc
        .get_str("source")
        .ok()
        .and_then(trimmed_non_empty)
        .unwrap_or("chatos")
        .to_string();
    let external_prompt_id = doc
        .get_str("external_prompt_id")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let external_task_id = doc
        .get_str("external_task_id")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let external_run_id = doc
        .get_str("external_run_id")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let external_project_id = doc
        .get_str("external_project_id")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let created_at = doc
        .get_str("created_at")
        .ok()
        .unwrap_or_default()
        .to_string();
    let updated_at = doc
        .get_str("updated_at")
        .ok()
        .unwrap_or_default()
        .to_string();

    Some(AskUserPromptRecord {
        id,
        conversation_id,
        conversation_turn_id,
        tool_call_id,
        kind,
        status,
        prompt,
        response,
        expires_at,
        source,
        external_prompt_id,
        external_task_id,
        external_run_id,
        external_project_id,
        created_at,
        updated_at,
    })
}

pub(super) fn parse_json_or_default(raw: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(raw).unwrap_or_else(|_| json!({}))
}

pub(super) fn parse_status(raw: &str) -> AskUserPromptStatus {
    AskUserPromptStatus::from_str(raw).unwrap_or(AskUserPromptStatus::Pending)
}
