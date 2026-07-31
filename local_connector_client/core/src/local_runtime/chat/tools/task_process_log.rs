// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use chatos_mcp_runtime::{BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback};

use crate::local_now_rfc3339;
use crate::local_runtime::storage::LocalDatabase;

const TASK_PROCESS_LOG_TOOL_NAME: &str = "record_process";
const TASK_PROCESS_LOG_MAX_CHARS: usize = 200_000;

#[derive(Clone)]
pub(super) struct LocalTaskProcessLogProvider {
    database: LocalDatabase,
    owner_user_id: String,
    session_id: String,
    task_id: String,
    run_id: String,
}

impl LocalTaskProcessLogProvider {
    pub(super) fn new(
        database: LocalDatabase,
        owner_user_id: impl Into<String>,
        session_id: impl Into<String>,
        task_id: impl Into<String>,
        run_id: impl Into<String>,
    ) -> Self {
        Self {
            database,
            owner_user_id: owner_user_id.into(),
            session_id: session_id.into(),
            task_id: task_id.into(),
            run_id: run_id.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RecordProcessArgs {
    #[serde(default)]
    operation: ProcessLogOperation,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    heading: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum ProcessLogOperation {
    #[default]
    Append,
    Replace,
    Clear,
}

#[async_trait]
impl BuiltinToolProvider for LocalTaskProcessLogProvider {
    fn server_name(&self) -> &str {
        "task_run_process"
    }

    fn list_tools(&self) -> Vec<Value> {
        chatos_mcp::task_process_log_tool_definitions()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        if name != TASK_PROCESS_LOG_TOOL_NAME {
            return Err(format!("unknown task process log tool: {name}"));
        }
        let input: RecordProcessArgs =
            serde_json::from_value(args).map_err(|error| error.to_string())?;
        record_process_log(self, input).await
    }
}

async fn record_process_log(
    provider: &LocalTaskProcessLogProvider,
    input: RecordProcessArgs,
) -> Result<Value, String> {
    let now = local_now_rfc3339();
    let current = current_task_process_log(provider).await?;
    let next = apply_process_log_update(current, &input, now.as_str())?;
    persist_task_process_log(provider, next.as_deref(), &input, now.as_str()).await?;
    Ok(json!({
        "recorded": true,
        "task_id": provider.task_id,
        "run_id": provider.run_id,
        "process_log_chars": next
            .as_ref()
            .map(|value| value.chars().count())
            .unwrap_or_default(),
        "updated_at": now,
    }))
}

async fn current_task_process_log(
    provider: &LocalTaskProcessLogProvider,
) -> Result<Option<String>, String> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT resume_hint
        FROM task_board_tasks
        WHERE id = ? AND owner_user_id = ? AND session_id = ? AND task_kind = 'task_runner'
        "#,
    )
    .bind(provider.task_id.as_str())
    .bind(provider.owner_user_id.as_str())
    .bind(provider.session_id.as_str())
    .fetch_optional(provider.database.pool())
    .await
    .map_err(|error| error.to_string())
}

async fn persist_task_process_log(
    provider: &LocalTaskProcessLogProvider,
    process_log: Option<&str>,
    input: &RecordProcessArgs,
    now: &str,
) -> Result<(), String> {
    if let Some(process_log) = process_log {
        sqlx::query(
            r#"
            UPDATE task_board_tasks
            SET resume_hint = ?, updated_at = ?
            WHERE id = ? AND owner_user_id = ? AND session_id = ? AND task_kind = 'task_runner'
            "#,
        )
        .bind(process_log)
        .bind(now)
        .bind(provider.task_id.as_str())
        .bind(provider.owner_user_id.as_str())
        .bind(provider.session_id.as_str())
        .execute(provider.database.pool())
        .await
        .map_err(|error| error.to_string())?;
    } else {
        sqlx::query(
            r#"
            UPDATE task_board_tasks
            SET resume_hint = '', updated_at = ?
            WHERE id = ? AND owner_user_id = ? AND session_id = ? AND task_kind = 'task_runner'
            "#,
        )
        .bind(now)
        .bind(provider.task_id.as_str())
        .bind(provider.owner_user_id.as_str())
        .bind(provider.session_id.as_str())
        .execute(provider.database.pool())
        .await
        .map_err(|error| error.to_string())?;
    }
    provider
        .database
        .append_local_task_run_event(
            provider.owner_user_id.as_str(),
            provider.run_id.as_str(),
            "task.process_log",
            json!({
                "task_id": provider.task_id,
                "run_id": provider.run_id,
                "operation": operation_key(input.operation),
                "heading": normalized_optional(input.heading.clone()),
                "content": normalized_optional(input.content.clone()),
                "process_log_chars": process_log
                    .map(|value| value.chars().count())
                    .unwrap_or_default(),
            }),
        )
        .await
        .map_err(|error| error.to_string())
}

fn apply_process_log_update(
    current: Option<String>,
    input: &RecordProcessArgs,
    now: &str,
) -> Result<Option<String>, String> {
    match input.operation {
        ProcessLogOperation::Clear => Ok(None),
        ProcessLogOperation::Replace => {
            let content = normalized_optional(input.content.clone());
            validate_process_log_length(content.as_deref())?;
            Ok(content)
        }
        ProcessLogOperation::Append => {
            let content = normalized_optional(input.content.clone())
                .ok_or_else(|| "content 不能为空".to_string())?;
            let entry = format_process_log_entry(now, input.heading.clone(), content);
            let next = match normalized_optional(current) {
                Some(existing) => format!("{existing}\n\n{entry}"),
                None => entry,
            };
            validate_process_log_length(Some(next.as_str()))?;
            Ok(Some(next))
        }
    }
}

fn format_process_log_entry(now: &str, heading: Option<String>, content: String) -> String {
    match normalized_optional(heading) {
        Some(heading) => format!("[{now}] {heading}\n{content}"),
        None => format!("[{now}]\n{content}"),
    }
}

fn validate_process_log_length(value: Option<&str>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let len = value.chars().count();
    if len > TASK_PROCESS_LOG_MAX_CHARS {
        Err(format!(
            "过程记录不能超过 {TASK_PROCESS_LOG_MAX_CHARS} 字符，当前 {len} 字符"
        ))
    } else {
        Ok(())
    }
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn operation_key(operation: ProcessLogOperation) -> &'static str {
    match operation {
        ProcessLogOperation::Append => "append",
        ProcessLogOperation::Replace => "replace",
        ProcessLogOperation::Clear => "clear",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_process_log_matches_task_runner_format() {
        let input = RecordProcessArgs {
            operation: ProcessLogOperation::Append,
            heading: Some("定位".to_string()),
            content: Some("已确认配置中心缺少该 MCP。".to_string()),
        };
        let next = apply_process_log_update(None, &input, "2026-07-30T00:00:00+08:00")
            .expect("append process log")
            .expect("content");

        assert!(next.contains("[2026-07-30T00:00:00+08:00] 定位"));
        assert!(next.contains("已确认配置中心缺少该 MCP。"));
    }
}
