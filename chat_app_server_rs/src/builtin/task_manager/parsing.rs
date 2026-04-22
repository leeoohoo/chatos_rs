use serde_json::{Map, Value};

use crate::services::task_manager::{TaskDraft, TaskOutcomeItem, TaskUpdatePatch};

pub(super) fn parse_task_drafts(args: &Value) -> Result<Vec<TaskDraft>, String> {
    if let Some(items) = args.get("tasks").and_then(|value| value.as_array()) {
        let mut out = Vec::new();
        for item in items {
            out.push(task_draft_from_value(item)?);
        }
        return Ok(out);
    }

    if args.get("title").and_then(|value| value.as_str()).is_some() {
        return Ok(vec![task_draft_from_map(
            args.as_object()
                .ok_or_else(|| "task payload must be an object".to_string())?,
        )?]);
    }

    Err("tasks or title is required".to_string())
}

pub(super) fn parse_update_patch(value: &Value) -> Result<TaskUpdatePatch, String> {
    let map = match value {
        Value::Object(map) => map.clone(),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err("changes cannot be empty".to_string());
            }
            let parsed: Value = serde_json::from_str(trimmed)
                .map_err(|_| "changes must be valid JSON".to_string())?;
            parsed
                .as_object()
                .cloned()
                .ok_or_else(|| "changes must be a JSON object".to_string())?
        }
        _ => return Err("changes must be a JSON object string".to_string()),
    };

    if map.is_empty() {
        return Err("changes cannot be empty".to_string());
    }

    let mut patch = TaskUpdatePatch::default();
    for (key, value) in &map {
        match key.as_str() {
            "title" => patch.title = Some(expect_string(value, "changes.title")?),
            "details" | "description" => {
                patch.details = Some(expect_string(value, "changes.details")?)
            }
            "priority" => patch.priority = Some(expect_string(value, "changes.priority")?),
            "status" => patch.status = Some(expect_string(value, "changes.status")?),
            "tags" => {
                patch.tags = Some(parse_tags(value, "changes.tags")?);
            }
            "due_at" | "dueAt" => {
                patch.due_at = Some(parse_due_at(value, "changes.due_at")?);
            }
            "outcome_summary" | "outcomeSummary" => {
                patch.outcome_summary = Some(expect_string(value, "changes.outcome_summary")?)
            }
            "outcome_items" | "outcomeItems" => {
                patch.outcome_items = Some(parse_outcome_items(value, "changes.outcome_items")?);
            }
            "resume_hint" | "resumeHint" => {
                patch.resume_hint = Some(expect_string(value, "changes.resume_hint")?)
            }
            "blocker_reason" | "blockerReason" => {
                patch.blocker_reason = Some(expect_string(value, "changes.blocker_reason")?)
            }
            "blocker_needs" | "blockerNeeds" => {
                patch.blocker_needs = Some(parse_tags(value, "changes.blocker_needs")?);
            }
            "blocker_kind" | "blockerKind" => {
                patch.blocker_kind = Some(expect_string(value, "changes.blocker_kind")?)
            }
            "completed_at" | "completedAt" => {
                patch.completed_at = Some(parse_due_at(value, "changes.completed_at")?);
            }
            "last_outcome_at" | "lastOutcomeAt" => {
                patch.last_outcome_at = Some(parse_due_at(value, "changes.last_outcome_at")?);
            }
            other => return Err(format!("unsupported changes field: {other}")),
        }
    }

    if patch.title.is_none()
        && patch.details.is_none()
        && patch.priority.is_none()
        && patch.status.is_none()
        && patch.tags.is_none()
        && patch.due_at.is_none()
        && patch.outcome_summary.is_none()
        && patch.outcome_items.is_none()
        && patch.resume_hint.is_none()
        && patch.blocker_reason.is_none()
        && patch.blocker_needs.is_none()
        && patch.blocker_kind.is_none()
        && patch.completed_at.is_none()
        && patch.last_outcome_at.is_none()
    {
        return Err("changes cannot be empty".to_string());
    }

    Ok(patch)
}

fn parse_tags(value: &Value, field: &str) -> Result<Vec<String>, String> {
    match value {
        Value::Array(values) => Ok(values
            .iter()
            .filter_map(|value| value.as_str().map(|item| item.to_string()))
            .collect()),
        Value::String(raw) => Ok(raw
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()),
        _ => Err(format!(
            "{field} must be an array or comma-separated string"
        )),
    }
}

fn parse_due_at(value: &Value, field: &str) -> Result<Option<String>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        _ => Err(format!("{field} must be a string or null")),
    }
}

fn parse_outcome_items(value: &Value, field: &str) -> Result<Vec<TaskOutcomeItem>, String> {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|item| {
                let map = item
                    .as_object()
                    .ok_or_else(|| format!("{field} items must be objects"))?;
                let text = map
                    .get("text")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| format!("{field} item.text is required"))?
                    .to_string();
                let refs = map
                    .get("refs")
                    .map(|refs| parse_tags(refs, "changes.outcome_items.refs"))
                    .transpose()?
                    .unwrap_or_default();
                Ok(TaskOutcomeItem {
                    kind: map
                        .get("kind")
                        .and_then(|value| value.as_str())
                        .unwrap_or("finding")
                        .to_string(),
                    text,
                    importance: map
                        .get("importance")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string()),
                    refs,
                })
            })
            .collect(),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(Vec::new());
            }
            let parsed: Value = serde_json::from_str(trimmed)
                .map_err(|_| format!("{field} must be valid JSON when provided as string"))?;
            parse_outcome_items(&parsed, field)
        }
        _ => Err(format!("{field} must be an array or JSON string")),
    }
}

fn expect_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .as_str()
        .map(|item| item.to_string())
        .ok_or_else(|| format!("{field} must be a string"))
}

fn task_draft_from_value(value: &Value) -> Result<TaskDraft, String> {
    let map = value
        .as_object()
        .ok_or_else(|| "each task must be an object".to_string())?;
    task_draft_from_map(map)
}

fn task_draft_from_map(map: &Map<String, Value>) -> Result<TaskDraft, String> {
    let title = map
        .get("title")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "task title is required".to_string())?
        .to_string();

    let details = optional_string(map, "details")
        .or_else(|| optional_string(map, "description"))
        .unwrap_or_default();

    let priority = optional_string(map, "priority").unwrap_or_else(|| "medium".to_string());
    let status = optional_string(map, "status").unwrap_or_else(|| "todo".to_string());
    let due_at = optional_string(map, "due_at").or_else(|| optional_string(map, "dueAt"));
    let outcome_summary = optional_string(map, "outcome_summary")
        .or_else(|| optional_string(map, "outcomeSummary"))
        .unwrap_or_default();
    let resume_hint = optional_string(map, "resume_hint")
        .or_else(|| optional_string(map, "resumeHint"))
        .unwrap_or_default();
    let blocker_reason = optional_string(map, "blocker_reason")
        .or_else(|| optional_string(map, "blockerReason"))
        .unwrap_or_default();
    let blocker_kind = optional_string(map, "blocker_kind")
        .or_else(|| optional_string(map, "blockerKind"))
        .unwrap_or_default();

    let tags = match map.get("tags") {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(|item| item.to_string()))
            .collect(),
        Some(Value::String(raw)) => raw
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    let blocker_needs = match map.get("blocker_needs").or_else(|| map.get("blockerNeeds")) {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(|item| item.to_string()))
            .collect(),
        Some(Value::String(raw)) => raw
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    let outcome_items = match map.get("outcome_items").or_else(|| map.get("outcomeItems")) {
        Some(value) => parse_outcome_items(value, "task.outcome_items")?,
        None => Vec::new(),
    };

    Ok(TaskDraft {
        title,
        details,
        priority,
        status,
        tags,
        due_at,
        outcome_summary,
        outcome_items,
        resume_hint,
        blocker_reason,
        blocker_needs,
        blocker_kind,
    })
}

fn optional_string(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|value| value.as_str())
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string())
}

fn required_string<'a>(args: &'a Value, field: &str) -> Result<&'a str, String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{field} is required"))
}

pub(super) fn required_string_arg(args: &Value, field: &str) -> Result<String, String> {
    let value = required_string(args, field)?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(trimmed.to_string())
}

pub(super) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
