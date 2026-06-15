use mongodb::{
    bson::{doc, Bson, Document},
    options::FindOptions,
};

use crate::models::{RunListFilters, TaskListFilters, UiPromptStatus};

use super::codec::{task_run_status_to_str, task_status_to_str, ui_prompt_status_to_str};

pub(super) fn is_mongo_active_run_index_conflict(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("e11000")
        || normalized.contains("duplicate key")
        || normalized.contains(super::ACTIVE_TASK_RUN_UNIQUE_INDEX_NAME)
}

pub(super) fn is_mongo_active_run_conflict(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    (normalized.contains("e11000") || normalized.contains("duplicate key"))
        && normalized.contains("task_id")
}

pub(super) fn build_mongo_task_filter(filters: &TaskListFilters) -> Document {
    let mut filter = Document::new();
    if let Some(status) = filters.status {
        filter.insert("status", task_status_to_str(status));
    }
    if let Some(keyword) = filters.keyword.as_deref() {
        let regex = doc! {
            "$regex": escape_regex_pattern(keyword),
            "$options": "i",
        };
        filter.insert(
            "$or",
            vec![
                doc! { "id": regex.clone() },
                doc! { "title": regex.clone() },
                doc! { "objective": regex.clone() },
                doc! { "description": regex.clone() },
                doc! { "result_summary": regex.clone() },
                doc! { "tags": regex },
            ],
        );
    }
    if let Some(tag) = filters.tag.as_deref() {
        filter.insert("tags", tag);
    }
    if let Some(model_config_id) = filters.model_config_id.as_deref() {
        filter.insert("default_model_config_id", model_config_id);
    }
    if let Some(creator_user_id) = filters.creator_user_id.as_deref() {
        filter.insert("creator_user_id", creator_user_id);
    }
    if filters.scheduled_only.unwrap_or(false) {
        filter.insert("schedule.mode", doc! { "$ne": "manual" });
    }
    if let Some(parent_task_id) = filters.parent_task_id.as_deref() {
        filter.insert("parent_task_id", parent_task_id);
    }
    if let Some(source_run_id) = filters.source_run_id.as_deref() {
        filter.insert("source_run_id", source_run_id);
    }
    filter
}

pub(super) fn build_mongo_run_filter(filters: &RunListFilters) -> Document {
    let mut filter = Document::new();
    if let Some(task_id) = filters.task_id.as_deref() {
        filter.insert("task_id", task_id);
    }
    if let Some(status) = filters.status {
        filter.insert("status", task_run_status_to_str(status));
    }
    if let Some(model_config_id) = filters.model_config_id.as_deref() {
        filter.insert("model_config_id", model_config_id);
    }
    if let Some(keyword) = filters.keyword.as_deref() {
        let regex = doc! {
            "$regex": escape_regex_pattern(keyword),
            "$options": "i",
        };
        filter.insert(
            "$or",
            vec![
                doc! { "id": regex.clone() },
                doc! { "task_id": regex.clone() },
                doc! { "model_config_id": regex.clone() },
                doc! { "result_summary": regex.clone() },
                doc! { "error_message": regex },
            ],
        );
    }
    filter
}

pub(super) fn build_mongo_prompt_filter(
    task_id: Option<&str>,
    run_id: Option<&str>,
    status: Option<UiPromptStatus>,
) -> Document {
    let mut filter = Document::new();
    if let Some(task_id) = task_id {
        filter.insert("task_id", task_id);
    }
    if let Some(run_id) = run_id {
        filter.insert("run_id", run_id);
    }
    if let Some(status) = status {
        filter.insert("status", ui_prompt_status_to_str(status));
    }
    filter
}

pub(super) fn mongo_find_options(
    sort: Document,
    offset: Option<usize>,
    limit: Option<usize>,
) -> FindOptions {
    let mut options = FindOptions::default();
    options.sort = Some(sort);
    options.skip = offset.filter(|value| *value > 0).map(|value| value as u64);
    options.limit = limit.map(|value| value as i64);
    options
}

pub(super) fn build_skip_stage(offset: Option<usize>) -> Document {
    match offset.filter(|value| *value > 0) {
        Some(offset) => doc! { "$skip": offset as i64 },
        None => Document::new(),
    }
}

pub(super) fn build_limit_stage(limit: Option<usize>) -> Document {
    match limit {
        Some(limit) => doc! { "$limit": limit as i64 },
        None => Document::new(),
    }
}

fn escape_regex_pattern(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

pub(super) fn bson_string_field(doc: &Document, field: &str) -> Option<String> {
    match doc.get(field) {
        Some(Bson::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(super) fn bson_usize_field(doc: &Document, field: &str) -> Option<usize> {
    match doc.get(field) {
        Some(Bson::Int32(value)) if *value >= 0 => Some(*value as usize),
        Some(Bson::Int64(value)) if *value >= 0 => Some(*value as usize),
        Some(Bson::Double(value)) if *value >= 0.0 => Some(*value as usize),
        _ => None,
    }
}
