// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use chatos_mcp_runtime::BuiltinMcpPromptLocale;

use crate::models::{TaskMcpConfig, TaskReportedOutcomeStatus, TaskRunEventRecord};

use super::RunService;

pub(super) const TASK_PROCESS_LOG_INTERNAL_SERVER_NAME: &str = "task_run_process";
const TASK_PROCESS_LOG_INTERNAL_TOOL_NAME: &str = "record_process";
const TASK_OUTCOME_INTERNAL_TOOL_NAME: &str = "report_outcome";
const TASK_OUTCOME_REASON_MAX_CHARS: usize = 2_000;

pub(super) fn task_process_logging_enabled(mcp_config: &TaskMcpConfig) -> bool {
    mcp_config.enabled
}

pub(super) fn task_process_log_prefixed_input_items(locale: BuiltinMcpPromptLocale) -> Vec<Value> {
    let tool_name = format!(
        "{}_{}",
        TASK_PROCESS_LOG_INTERNAL_SERVER_NAME, TASK_PROCESS_LOG_INTERNAL_TOOL_NAME
    );
    let outcome_tool_name = format!(
        "{}_{}",
        TASK_PROCESS_LOG_INTERNAL_SERVER_NAME, TASK_OUTCOME_INTERNAL_TOOL_NAME
    );
    let text = task_process_log_prompt_text(locale, tool_name.as_str(), outcome_tool_name.as_str());
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
    let outcome_tool_name = format!(
        "{}_{}",
        TASK_PROCESS_LOG_INTERNAL_SERVER_NAME, TASK_OUTCOME_INTERNAL_TOOL_NAME
    );
    task_process_log_prompt_text(locale, tool_name.as_str(), outcome_tool_name.as_str())
}

fn task_process_log_prompt_text(
    locale: BuiltinMcpPromptLocale,
    tool_name: &str,
    outcome_tool_name: &str,
) -> String {
    if locale.is_english() {
        format!(
            "[Task Execution Process]\nThe run-scoped system MCP tools `{tool_name}` and `{outcome_tool_name}` are available during this Task Runner run. Use `{tool_name}` to append short visible breadcrumbs at meaningful milestones: selected approach, reused existing code or platform capability, root-cause finding, implementation completion, verification result, blocker, or next step. After all implementation and verification work is finished, you must call `{outcome_tool_name}` exactly once with `succeeded`, `failed`, or `blocked` and a concrete reason. That outcome call must be your final tool call immediately before the user-facing final response. The runtime will reject a final response when no outcome has been reported. Keep entries concise. Do not record hidden chain-of-thought, credentials, secrets, raw dumps, or unrelated drafts. This MCP is mounted only inside the current Task Runner execution and is not part of the external Task Runner management API."
        )
    } else {
        format!(
            "[任务执行过程]\n本次 Task Runner 运行期间提供运行期系统 MCP 工具 `{tool_name}` 和 `{outcome_tool_name}`。使用 `{tool_name}` 在有意义的里程碑追加简短、可展示的执行路标：选择的方案、复用的已有代码或平台能力、根因发现、实现完成、验证结果、阻塞和下一步。全部实现与验证结束后，你必须且只能调用一次 `{outcome_tool_name}`，明确上报 `succeeded`、`failed` 或 `blocked`，并给出具体理由。该终态上报必须是最终用户答复之前的最后一次工具调用；未上报终态时，运行时不会接受最终答复。记录要简洁。不要记录隐藏思维链、凭证、密钥、原始大段输出或无关草稿。这个 MCP 只挂载在当前 Task Runner 执行内部，不属于对外的 Task Runner 管理 API。"
        )
    }
}

impl RunService {
    pub(crate) async fn record_ai_reported_task_outcome(
        &self,
        run_id: &str,
        status: TaskReportedOutcomeStatus,
        reason: &str,
    ) -> Result<(TaskRunEventRecord, bool), String> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err("task outcome reason must not be empty".to_string());
        }
        let reason_chars = reason.chars().count();
        if reason_chars > TASK_OUTCOME_REASON_MAX_CHARS {
            return Err(format!(
                "task outcome reason cannot exceed {TASK_OUTCOME_REASON_MAX_CHARS} characters; received {reason_chars}"
            ));
        }
        let existing = self
            .store
            .get_run_event_by_type(run_id, "task_outcome_reported")
            .await?;
        if let Some(existing) = existing {
            let same_status = existing
                .payload
                .as_ref()
                .and_then(|payload| payload.get("status"))
                .and_then(Value::as_str)
                == Some(status.as_str());
            let same_reason = existing
                .payload
                .as_ref()
                .and_then(|payload| payload.get("reason"))
                .and_then(Value::as_str)
                == Some(reason);
            if same_status && same_reason {
                return Ok((existing, true));
            }
            return Err("task outcome was already reported for this run".to_string());
        }
        let event = TaskRunEventRecord::new(
            run_id.to_string(),
            "task_outcome_reported",
            Some(format!("AI 已上报任务终态：{}", status.as_str())),
            Some(json!({
                "status": status.as_str(),
                "reason": reason,
                "reported_by": "task_runner_ai",
            })),
        );
        self.store.append_run_event(event.clone()).await?;
        Ok((event, false))
    }
}
