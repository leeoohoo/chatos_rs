use mongodb::bson::{doc, Bson, Document};

use super::normalizer::{
    normalize_priority, normalize_status, normalize_tags, parse_tags_json, trimmed_non_empty,
};
use super::types::TaskRecord;

pub(super) fn task_record_to_doc(task: &TaskRecord) -> Document {
    let tags = task
        .tags
        .iter()
        .cloned()
        .map(Bson::String)
        .collect::<Vec<Bson>>();

    let mut doc = doc! {
        "id": task.id.clone(),
        "session_id": task.session_id.clone(),
        "conversation_turn_id": task.conversation_turn_id.clone(),
        "title": task.title.clone(),
        "details": task.details.clone(),
        "priority": task.priority.clone(),
        "status": task.status.clone(),
        "tags": Bson::Array(tags),
        "created_at": task.created_at.clone(),
        "updated_at": task.updated_at.clone(),
    };
    if let Some(due_at) = task.due_at.clone() {
        doc.insert("due_at", Bson::String(due_at));
    }
    doc
}

pub(super) fn task_record_from_doc(doc: &Document) -> Option<TaskRecord> {
    let id = doc.get_str("id").ok()?.to_string();
    let session_id = doc.get_str("session_id").ok()?.to_string();
    let conversation_turn_id = doc.get_str("conversation_turn_id").ok()?.to_string();
    let title = doc.get_str("title").ok()?.to_string();
    let details = doc.get_str("details").ok().unwrap_or_default().to_string();
    let priority = doc.get_str("priority").ok().unwrap_or("medium").to_string();
    let status = doc.get_str("status").ok().unwrap_or("todo").to_string();
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

    Some(TaskRecord {
        id,
        session_id,
        conversation_turn_id,
        title,
        details,
        priority: normalize_priority(priority.as_str()),
        status: normalize_status(status.as_str()),
        tags: normalize_tags(tags),
        due_at,
        created_at,
        updated_at,
    })
}
