use serde_json::Value;
use std::path::Path as FsPath;

use crate::models::terminal::Terminal;
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::services::terminal_manager::TerminalsManager;

use super::{DEFAULT_TERMINAL_HISTORY_LIMIT, MAX_TERMINAL_HISTORY_LIMIT};

pub(super) fn normalize_history_limit(limit: Option<i64>) -> i64 {
    limit
        .unwrap_or(DEFAULT_TERMINAL_HISTORY_LIMIT)
        .clamp(1, MAX_TERMINAL_HISTORY_LIMIT)
}

pub(super) fn normalize_history_offset(offset: Option<i64>) -> i64 {
    offset.unwrap_or(0).max(0)
}

pub(super) fn normalize_history_before(before: Option<String>) -> Option<String> {
    before
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

pub(super) async fn list_terminal_logs_recent_page(
    terminal_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TerminalLog>, String> {
    let fetch_limit = limit.saturating_add(offset).max(limit);
    let mut logs = TerminalLogService::list_recent(terminal_id, fetch_limit).await?;

    if offset > 0 {
        let keep = logs.len().saturating_sub(offset as usize);
        logs.truncate(keep);
    }

    Ok(logs)
}

pub(super) async fn list_terminal_logs_before_page(
    terminal_id: &str,
    limit: i64,
    before_created_at: &str,
) -> Result<Vec<TerminalLog>, String> {
    TerminalLogService::list_before(terminal_id, before_created_at, limit).await
}

pub(super) fn derive_terminal_name(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(&['/', '\\'][..]);
    if trimmed.is_empty() {
        return "Terminal".to_string();
    }
    FsPath::new(trimmed)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Terminal".to_string())
}

pub(super) fn attach_busy(manager: &TerminalsManager, terminal: Terminal) -> Value {
    let mut value = serde_json::to_value(&terminal).unwrap_or(Value::Null);
    let busy = manager.get_busy(&terminal.id).unwrap_or(false);
    if let Value::Object(ref mut map) = value {
        map.insert("busy".to_string(), Value::Bool(busy));
    }
    value
}
