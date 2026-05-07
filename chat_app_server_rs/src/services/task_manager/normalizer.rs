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
    draft.outcome_summary = draft.outcome_summary.trim().to_string();
    draft.outcome_items = draft
        .outcome_items
        .into_iter()
        .filter_map(|mut item| {
            item.kind = item.kind.trim().to_ascii_lowercase();
            item.text = item.text.trim().to_string();
            item.importance = item
                .importance
                .as_deref()
                .and_then(trimmed_non_empty)
                .map(|value| match value.trim().to_ascii_lowercase().as_str() {
                    "high" => "high".to_string(),
                    "low" => "low".to_string(),
                    _ => "medium".to_string(),
                });
            item.refs = item
                .refs
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect();
            if item.text.is_empty() {
                None
            } else {
                Some(item)
            }
        })
        .collect();
    draft.resume_hint = draft.resume_hint.trim().to_string();
    draft.blocker_reason = draft.blocker_reason.trim().to_string();
    draft.blocker_needs = draft
        .blocker_needs
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    draft.blocker_kind = match draft.blocker_kind.trim().to_ascii_lowercase().as_str() {
        "external_dependency" => "external_dependency".to_string(),
        "permission" => "permission".to_string(),
        "missing_information" => "missing_information".to_string(),
        "design_decision" => "design_decision".to_string(),
        "environment_failure" => "environment_failure".to_string(),
        "upstream_bug" => "upstream_bug".to_string(),
        _ => {
            if draft.blocker_reason.is_empty() {
                String::new()
            } else {
                "unknown".to_string()
            }
        }
    };
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
