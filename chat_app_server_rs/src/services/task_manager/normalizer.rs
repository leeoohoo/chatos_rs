use super::types::TaskDraft;

pub(super) fn normalize_task_drafts(drafts: Vec<TaskDraft>) -> Result<Vec<TaskDraft>, String> {
    let mut out = Vec::new();
    for draft in drafts {
        out.push(normalize_task_draft(draft)?);
    }
    Ok(out)
}

pub(super) fn normalize_task_draft(mut draft: TaskDraft) -> Result<TaskDraft, String> {
    draft.title = draft.title.trim().to_string();
    if draft.title.is_empty() {
        return Err("task title is required".to_string());
    }
    draft.details = draft.details.trim().to_string();
    draft.priority = normalize_priority(draft.priority.as_str());
    draft.status = normalize_status(draft.status.as_str());
    draft.tags = normalize_tags(draft.tags);
    draft.due_at = draft
        .due_at
        .as_deref()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    Ok(draft)
}

pub(super) fn normalize_priority(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => "high".to_string(),
        "low" => "low".to_string(),
        _ => "medium".to_string(),
    }
}

pub(super) fn normalize_status(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "doing" => "doing".to_string(),
        "blocked" => "blocked".to_string(),
        "done" => "done".to_string(),
        _ => "todo".to_string(),
    }
}

pub(super) fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

pub(super) fn parse_tags_json(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .ok()
        .map(normalize_tags)
        .unwrap_or_default()
}

pub(super) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
