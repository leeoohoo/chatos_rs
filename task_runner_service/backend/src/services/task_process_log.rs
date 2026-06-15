use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

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
        && !matches!(
            mcp_config.init_mode,
            chatos_ai_runtime::TaskMcpInitMode::Disabled
        )
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
            "[Task Execution Process]\nA private internal tool `{tool_name}` is available during this task run. Use it to append visible progress notes, observations, verification results, blockers, and next steps for the current task only. Do not record hidden chain-of-thought, credentials, secrets, or unrelated drafts. This tool is internal to Task Runner execution and is not part of the external Task Runner MCP API."
        )
    } else {
        format!(
            "[任务执行过程]\n本次任务执行期间提供内部专用工具 `{tool_name}`。仅用它为当前任务追加可展示的进展、观察、验证结果、阻塞和下一步；不要记录隐藏思维链、凭证、密钥或无关草稿。这个工具只属于 Task Runner 内部执行，不属于对外 Task Runner MCP API。"
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
        vec![json!({
            "name": TASK_PROCESS_LOG_INTERNAL_TOOL_NAME,
            "description": "Record visible execution process notes for the current Task Runner task only. Use append for progress, observations, verification results, blockers, and next steps. Do not record hidden chain-of-thought, credentials, secrets, or unrelated drafts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["append", "replace", "clear"],
                        "default": "append",
                        "description": "append adds one timestamped entry; replace rewrites the full process log; clear removes the process log."
                    },
                    "heading": {
                        "type": ["string", "null"],
                        "description": "Short visible heading for append entries, or null when not needed."
                    },
                    "content": {
                        "type": ["string", "null"],
                        "description": "Visible process content. Required for append/replace; pass null for clear."
                    }
                },
                "required": ["operation", "heading", "content"],
                "additionalProperties": false
            }
        })]
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
        Ok(json!({
            "task_id": task.id,
            "run_id": self.run_id,
            "process_log": task.process_log,
            "updated_at": task.updated_at,
        }))
    }
}
