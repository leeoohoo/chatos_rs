// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use chatos_mcp_runtime::BuiltinMcpPromptLocale;

use crate::models::TaskMcpConfig;

pub(super) const TASK_PROCESS_LOG_INTERNAL_SERVER_NAME: &str = "task_run_process";
const TASK_PROCESS_LOG_INTERNAL_TOOL_NAME: &str = "record_process";

pub(super) fn task_process_logging_enabled(mcp_config: &TaskMcpConfig) -> bool {
    mcp_config.enabled
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
            "[Task Execution Process]\nThe run-scoped system MCP tool `{tool_name}` is available during this Task Runner run. You must call it to append short visible breadcrumbs at meaningful milestones: selected approach, reused existing code or platform capability, root-cause finding, implementation completion, verification result, blocker, or next step. Always record at least one implementation milestone and the final verification outcome before finishing. Keep entries concise so a later reviewer can understand what changed and why. Do not record hidden chain-of-thought, credentials, secrets, raw dumps, or unrelated drafts. This MCP is mounted only inside the current Task Runner execution and is not part of the external Task Runner management API."
        )
    } else {
        format!(
            "[任务执行过程]\n本次 Task Runner 运行期间提供运行期系统 MCP 工具 `{tool_name}`。你必须在有意义的里程碑调用它追加简短、可展示的执行路标：选择的方案、复用的已有代码或平台能力、根因发现、实现完成、验证结果、阻塞和下一步。结束前至少记录一条实现里程碑和最终验证结果。记录要简洁，并能帮助后续 review 看懂改了什么、为什么这样改。不要记录隐藏思维链、凭证、密钥、原始大段输出或无关草稿。这个 MCP 只挂载在当前 Task Runner 执行内部，不属于对外的 Task Runner 管理 API。"
        )
    }
}
