// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{Bson, Document};

use super::normalizer::{
    normalize_priority, normalize_status, normalize_tags, parse_tags_json, trimmed_non_empty,
};
use super::types::{TaskOutcomeItem, TaskRecord};

pub(super) fn task_record_from_doc(doc: &Document) -> Option<TaskRecord> {
    let id = doc.get_str("id").ok()?.to_string();
    let conversation_id = doc.get_str("conversation_id").ok()?.to_string();
    let conversation_turn_id = doc.get_str("conversation_turn_id").ok()?.to_string();
    let title = doc.get_str("title").ok()?.to_string();
    let details = doc.get_str("details").ok().unwrap_or_default().to_string();
    let priority = doc.get_str("priority").ok().unwrap_or("medium").to_string();
    let status = doc.get_str("status").ok().unwrap_or("todo").to_string();
    let outcome_summary = doc
        .get_str("outcome_summary")
        .ok()
        .unwrap_or_default()
        .to_string();
    let outcome_items = parse_outcome_items(doc.get("outcome_items"));
    let resume_hint = doc
        .get_str("resume_hint")
        .ok()
        .unwrap_or_default()
        .to_string();
    let blocker_reason = doc
        .get_str("blocker_reason")
        .ok()
        .unwrap_or_default()
        .to_string();
    let blocker_needs = parse_string_list(doc.get("blocker_needs"));
    let blocker_kind = doc
        .get_str("blocker_kind")
        .ok()
        .unwrap_or_default()
        .to_string();
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

    let tags = match doc.get("tags") {
        Some(Bson::Array(arr)) => arr
            .iter()
            .filter_map(|value| value.as_str().map(|item| item.to_string()))
            .collect::<Vec<String>>(),
        Some(Bson::String(raw)) => parse_tags_json(raw),
        _ => Vec::new(),
    };

    let due_at = doc
        .get_str("due_at")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let completed_at = doc
        .get_str("completed_at")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let last_outcome_at = doc
        .get_str("last_outcome_at")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());

    Some(TaskRecord {
        id,
        conversation_id,
        conversation_turn_id,
        title,
        details,
        priority: normalize_priority(priority.as_str()),
        status: normalize_status(status.as_str()),
        tags: normalize_tags(tags),
        due_at,
        outcome_summary,
        outcome_items,
        resume_hint,
        blocker_reason,
        blocker_needs,
        blocker_kind,
        completed_at,
        last_outcome_at,
        created_at,
        updated_at,
    })
}

fn parse_outcome_items(value: Option<&Bson>) -> Vec<TaskOutcomeItem> {
    match value {
        Some(Bson::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Bson::Document(doc) => Some(TaskOutcomeItem {
                    kind: doc.get_str("kind").ok().unwrap_or("finding").to_string(),
                    text: doc.get_str("text").ok().unwrap_or_default().to_string(),
                    importance: doc
                        .get_str("importance")
                        .ok()
                        .map(|value| value.to_string()),
                    refs: parse_string_list(doc.get("refs")),
                }),
                _ => None,
            })
            .filter(|item| !item.text.trim().is_empty())
            .collect(),
        Some(Bson::String(raw)) => serde_json::from_str::<Vec<TaskOutcomeItem>>(raw)
            .unwrap_or_default()
            .into_iter()
            .filter(|item| !item.text.trim().is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_string_list(value: Option<&Bson>) -> Vec<String> {
    match value {
        Some(Bson::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(|value| value.to_string()))
            .collect(),
        Some(Bson::String(raw)) => serde_json::from_str::<Vec<String>>(raw).unwrap_or_default(),
        _ => Vec::new(),
    }
}
