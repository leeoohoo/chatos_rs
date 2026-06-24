use mongodb::{
    bson::{doc, Bson, Document},
    options::FindOptions,
};

use crate::models::{RunListFilters, TaskListFilters, UiPromptStatus, PUBLIC_PROJECT_ID};

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
    let mut and_clauses = Vec::<Document>::new();
    if let Some(status) = filters.status {
        filter.insert("status", task_status_to_str(status));
    }
    if let Some(keyword) = filters.keyword.as_deref() {
        let regex = doc! {
            "$regex": escape_regex_pattern(keyword),
            "$options": "i",
        };
        and_clauses.push(doc! {
            "$or": vec![
                doc! { "id": regex.clone() },
                doc! { "title": regex.clone() },
                doc! { "objective": regex.clone() },
                doc! { "description": regex.clone() },
                doc! { "result_summary": regex.clone() },
                doc! { "tags": regex },
            ],
        });
    }
    if let Some(tag) = filters.tag.as_deref() {
        filter.insert("tags", tag);
    }
    if let Some(model_config_id) = filters.model_config_id.as_deref() {
        filter.insert("default_model_config_id", model_config_id);
    }
    if let Some(project_id) = filters.project_id.as_deref() {
        if project_id == PUBLIC_PROJECT_ID {
            and_clauses.push(doc! {
                "$or": [
                    { "project_id": PUBLIC_PROJECT_ID },
                    { "project_id": "0" },
                    { "project_id": "" },
                    { "project_id": null },
                    { "project_id": { "$exists": false } }
                ]
            });
        } else {
            filter.insert("project_id", project_id);
        }
    }
    if let Some(owner_user_id) = filters.creator_user_id.as_deref() {
        filter.insert(
            "$or",
            vec![
                doc! { "owner_user_id": owner_user_id },
                doc! {
                    "$and": [
                        {
                            "$or": [
                                { "owner_user_id": { "$exists": false } },
                                { "owner_user_id": null },
                                { "owner_user_id": "" }
                            ]
                        },
                        { "creator_user_id": owner_user_id }
                    ]
                },
            ],
        );
    }
    if filters.scheduled_only.unwrap_or(false) {
        filter.insert("schedule.mode", doc! { "$ne": "manual" });
    }
    if let Some(parent_task_id) = filters.parent_task_id.as_deref() {
        filter.insert("parent_task_id", parent_task_id);
    } else if filters.include_subtasks == Some(false) {
        and_clauses.push(doc! {
            "$or": [
                { "parent_task_id": { "$exists": false } },
                { "parent_task_id": null },
                { "parent_task_id": "" }
            ]
        });
    }
    if let Some(source_run_id) = filters.source_run_id.as_deref() {
        filter.insert("source_run_id", source_run_id);
    }
    if let Some(source_session_id) = filters.source_session_id.as_deref() {
        filter.insert("source_session_id", source_session_id);
    }
    if !filters.source_user_message_ids.is_empty() || !filters.source_turn_ids.is_empty() {
        let mut source_clauses = Vec::new();
        if !filters.source_user_message_ids.is_empty() {
            source_clauses.push(doc! {
                "source_user_message_id": {
                    "$in": filters
                        .source_user_message_ids
                        .iter()
                        .cloned()
                        .map(Bson::String)
                        .collect::<Vec<_>>()
                }
            });
        }
        if !filters.source_turn_ids.is_empty() {
            source_clauses.push(doc! {
                "source_turn_id": {
                    "$in": filters
                        .source_turn_ids
                        .iter()
                        .cloned()
                        .map(Bson::String)
                        .collect::<Vec<_>>()
                }
            });
        }
        and_clauses.push(doc! { "$or": source_clauses });
    }
    if !and_clauses.is_empty() {
        filter.insert("$and", and_clauses);
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
