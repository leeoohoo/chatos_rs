// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use chatos_mcp_runtime::{
    BuiltinMcpPromptLocale, BuiltinToolProvider, McpBuiltinServer, ToolCallContext,
    ToolStreamChunkCallback,
};

use crate::models::{RecordTaskProcessRequest, TaskMcpConfig, TaskProcessLogOperation};

use super::TaskService;

pub(super) const TASK_PROCESS_LOG_INTERNAL_SERVER_NAME: &str = "task_run_process";
const TASK_PROCESS_LOG_INTERNAL_TOOL_NAME: &str = "record_process";

pub(super) fn task_process_logging_enabled(mcp_config: &TaskMcpConfig) -> bool {
    mcp_config.enabled
}

pub(super) fn task_process_log_builtin_server() -> McpBuiltinServer {
    McpBuiltinServer {
        name: TASK_PROCESS_LOG_INTERNAL_SERVER_NAME.to_string(),
        kind: TASK_PROCESS_LOG_INTERNAL_SERVER_NAME.to_string(),
        workspace_dir: String::new(),
        user_id: None,
        project_id: None,
        remote_connection_id: None,
        contact_agent_id: None,
        auto_create_task: false,
        allow_writes: true,
        max_file_bytes: 0,
        max_write_bytes: 0,
        search_limit: 0,
    }
}

pub(super) fn task_process_log_prefixed_input_items(locale: BuiltinMcpPromptLocale) -> Vec<Value> {
    let tool_name = format!(
        "{}_{}",
        TASK_PROCESS_LOG_INTERNAL_SERVER_NAME, TASK_PROCESS_LOG_INTERNAL_TOOL_NAME
    );
    let text = task_process_log_prompt_text(locale, tool_name.as_str());
    vec![json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": text
        }]
    })]
}

pub(super) fn task_process_log_preview_text(locale: BuiltinMcpPromptLocale) -> String {
    let tool_name = format!(
        "{}_{}",
        TASK_PROCESS_LOG_INTERNAL_SERVER_NAME, TASK_PROCESS_LOG_INTERNAL_TOOL_NAME
    );
    task_process_log_prompt_text(locale, tool_name.as_str())
}

fn task_process_log_prompt_text(locale: BuiltinMcpPromptLocale, tool_name: &str) -> String {
    if locale.is_english() {
        format!(
            "[Task Execution Process]\nThe run-scoped system MCP tool `{tool_name}` is available during this Task Runner run. Use it for short visible breadcrumbs only: selected approach, reused existing code or platform capability, root-cause finding, verification result, blocker, or next step. Prefer concise entries that help a later reviewer understand what changed and why. Do not record hidden chain-of-thought, credentials, secrets, raw dumps, or unrelated drafts. This MCP is mounted only inside the current Task Runner execution and is not part of the external Task Runner management API."
        )
    } else {
        format!(
            "[任务执行过程]\n本次 Task Runner 运行期间提供运行期系统 MCP 工具 `{tool_name}`。只记录简短、可展示的执行路标：选择的方案、复用的已有代码或平台能力、根因发现、验证结果、阻塞和下一步。记录要能帮助后续 review 看懂改了什么、为什么这样改。不要记录隐藏思维链、凭证、密钥、原始大段输出或无关草稿。这个 MCP 只挂载在当前 Task Runner 执行内部，不属于对外的 Task Runner 管理 API。"
        )
    }
}

#[derive(Debug, Deserialize)]
struct InternalRecordProcessArgs {
    #[serde(default)]
    operation: TaskProcessLogOperation,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    heading: Option<String>,
}

impl InternalRecordProcessArgs {
    fn into_request(self) -> RecordTaskProcessRequest {
        RecordTaskProcessRequest {
            operation: self.operation,
            content: self.content,
            heading: self.heading,
        }
    }
}

#[derive(Clone)]
pub(super) struct TaskProcessLogBuiltinProvider {
    server_name: String,
    task_service: TaskService,
    task_id: String,
    run_id: String,
}

impl TaskProcessLogBuiltinProvider {
    pub(super) fn new(
        server_name: impl Into<String>,
        task_service: TaskService,
        task_id: String,
        run_id: String,
    ) -> Self {
        Self {
            server_name: server_name.into(),
            task_service,
            task_id,
            run_id,
        }
    }
}

#[async_trait]
impl BuiltinToolProvider for TaskProcessLogBuiltinProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
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
        if name != TASK_PROCESS_LOG_INTERNAL_TOOL_NAME {
            return Err(format!("未知任务过程记录工具: {name}"));
        }
        let input: InternalRecordProcessArgs =
            serde_json::from_value(args).map_err(|err| err.to_string())?;
        let task = self
            .task_service
            .record_task_process(self.task_id.as_str(), input.into_request())
            .await?
            .ok_or_else(|| format!("任务不存在: {}", self.task_id))?;
        Ok(task_process_log_ack(
            task.id.as_str(),
            self.run_id.as_str(),
            task.process_log.as_deref(),
            task.updated_at.as_str(),
        ))
    }
}

fn task_process_log_ack(
    task_id: &str,
    run_id: &str,
    process_log: Option<&str>,
    updated_at: &str,
) -> Value {
    json!({
        "recorded": true,
        "task_id": task_id,
        "run_id": run_id,
        "process_log_chars": process_log
            .map(|value| value.chars().count())
            .unwrap_or_default(),
        "updated_at": updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_log_tool_returns_compact_ack_without_replaying_history() {
        let history = "旧运行日志".repeat(8_000);
        let response = task_process_log_ack(
            "task-1",
            "run-2",
            Some(history.as_str()),
            "2026-07-28T00:00:00Z",
        );

        assert_eq!(
            response.get("recorded").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            response.get("process_log_chars").and_then(Value::as_u64),
            Some(history.chars().count() as u64)
        );
        assert!(response.get("process_log").is_none());
        assert!(response.to_string().len() < 256);
    }
}
