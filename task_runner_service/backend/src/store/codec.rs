use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::models::{AskUserPromptStatus, TaskRunStatus, TaskStatus, UserRole};

pub(super) fn encode_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|err| err.to_string())
}

pub(super) fn encode_json_option(value: &Option<Value>) -> Result<String, String> {
    match value {
        Some(value) => encode_json(value),
        None => Ok("null".to_string()),
    }
}

pub(super) fn encode_json_optional<T: Serialize>(value: Option<&T>) -> Result<String, String> {
    match value {
        Some(value) => encode_json(value),
        None => Ok("null".to_string()),
    }
}

pub(super) fn decode_json<T>(text: String) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_str(&text).map_err(|err| err.to_string())
}

pub(super) fn decode_json_option(text: String) -> Result<Option<Value>, String> {
    let value: Value = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

pub(super) fn decode_json_optional_typed<T>(text: String) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    let value: Value = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    if value.is_null() {
        Ok(None)
    } else {
        serde_json::from_value(value)
            .map(Some)
            .map_err(|err| err.to_string())
    }
}

pub(super) fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

pub(super) fn int_to_bool(value: i64) -> bool {
    value != 0
}

pub(super) fn task_status_to_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Draft => "draft",
        TaskStatus::Ready => "ready",
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Succeeded => "succeeded",
        TaskStatus::Failed => "failed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Archived => "archived",
    }
}

pub(super) fn task_status_from_str(value: &str) -> TaskStatus {
    match value {
        "ready" => TaskStatus::Ready,
        "queued" => TaskStatus::Queued,
        "running" => TaskStatus::Running,
        "succeeded" => TaskStatus::Succeeded,
        "failed" => TaskStatus::Failed,
        "blocked" => TaskStatus::Blocked,
        "cancelled" => TaskStatus::Cancelled,
        "archived" => TaskStatus::Archived,
        _ => TaskStatus::Draft,
    }
}

pub(super) fn user_role_to_str(role: UserRole) -> &'static str {
    match role {
        UserRole::Admin => "admin",
        UserRole::Agent => "agent",
    }
}

pub(super) fn user_role_from_str(value: &str) -> UserRole {
    match value {
        "admin" => UserRole::Admin,
        _ => UserRole::Agent,
    }
}

pub(super) fn task_run_status_to_str(status: TaskRunStatus) -> &'static str {
    match status {
        TaskRunStatus::Queued => "queued",
        TaskRunStatus::Running => "running",
        TaskRunStatus::Succeeded => "succeeded",
        TaskRunStatus::Failed => "failed",
        TaskRunStatus::Cancelled => "cancelled",
        TaskRunStatus::Blocked => "blocked",
    }
}

pub(super) fn task_run_status_from_str(value: &str) -> TaskRunStatus {
    match value {
        "running" => TaskRunStatus::Running,
        "succeeded" => TaskRunStatus::Succeeded,
        "failed" => TaskRunStatus::Failed,
        "cancelled" => TaskRunStatus::Cancelled,
        "blocked" => TaskRunStatus::Blocked,
        _ => TaskRunStatus::Queued,
    }
}

pub(super) fn ask_user_prompt_status_to_str(status: AskUserPromptStatus) -> &'static str {
    match status {
        AskUserPromptStatus::Pending => "pending",
        AskUserPromptStatus::Submitted => "submitted",
        AskUserPromptStatus::Cancelled => "cancelled",
        AskUserPromptStatus::TimedOut => "timed_out",
        AskUserPromptStatus::Failed => "failed",
    }
}

pub(super) fn ask_user_prompt_status_from_str(value: &str) -> AskUserPromptStatus {
    match value {
        "submitted" => AskUserPromptStatus::Submitted,
        "cancelled" => AskUserPromptStatus::Cancelled,
        "timed_out" => AskUserPromptStatus::TimedOut,
        "failed" => AskUserPromptStatus::Failed,
        _ => AskUserPromptStatus::Pending,
    }
}
