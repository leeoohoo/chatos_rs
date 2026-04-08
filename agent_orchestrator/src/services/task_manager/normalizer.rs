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
    draft.task_ref = draft
        .task_ref
        .as_deref()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    draft.task_kind = draft
        .task_kind
        .as_deref()
        .and_then(trimmed_non_empty)
        .map(normalize_task_kind);
    draft.depends_on_refs = normalize_unique_string_list(draft.depends_on_refs);
    draft.verification_of_refs = normalize_unique_string_list(draft.verification_of_refs);
    draft.acceptance_criteria = normalize_unique_string_list(draft.acceptance_criteria);
    draft.priority = normalize_priority(draft.priority.as_str());
    draft.status = normalize_status(draft.status.as_str());
    draft.tags = normalize_tags(draft.tags);
    draft.due_at = draft
        .due_at
        .as_deref()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    draft.required_builtin_capabilities = normalize_unique_string_list(
        draft
            .required_builtin_capabilities
            .into_iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .collect(),
    );
    draft.required_context_assets = normalize_required_context_assets(draft.required_context_assets);
    draft.planned_builtin_mcp_ids = normalize_unique_string_list(draft.planned_builtin_mcp_ids);
    Ok(draft)
}

fn normalize_required_context_assets(
    values: Vec<super::types::TaskRequiredContextAssetDraft>,
) -> Vec<super::types::TaskRequiredContextAssetDraft> {
    let mut out = Vec::new();
    for value in values {
        let asset_type = value.asset_type.trim().to_ascii_lowercase();
        let asset_ref = value.asset_ref.trim().to_string();
        if asset_type.is_empty() || asset_ref.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &super::types::TaskRequiredContextAssetDraft| {
            existing.asset_type == asset_type && existing.asset_ref == asset_ref
        }) {
            continue;
        }
        out.push(super::types::TaskRequiredContextAssetDraft {
            asset_type,
            asset_ref,
        });
    }
    out
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
        "pending_confirm" => "pending_confirm".to_string(),
        "pending_execute" => "pending_execute".to_string(),
        "running" => "running".to_string(),
        "paused" => "paused".to_string(),
        "blocked" => "blocked".to_string(),
        "completed" => "completed".to_string(),
        "failed" => "failed".to_string(),
        "cancelled" => "cancelled".to_string(),
        "skipped" => "skipped".to_string(),
        _ => "pending_confirm".to_string(),
    }
}

pub(super) fn normalize_task_kind(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "analysis" => "analysis".to_string(),
        "implementation" => "implementation".to_string(),
        "verification" => "verification".to_string(),
        "documentation" => "documentation".to_string(),
        "delivery" => "delivery".to_string(),
        "migration" => "migration".to_string(),
        "research" => "research".to_string(),
        _ => "general".to_string(),
    }
}

pub(super) fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    normalize_unique_string_list(tags)
}

pub(super) fn normalize_unique_string_list(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
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

pub(super) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
