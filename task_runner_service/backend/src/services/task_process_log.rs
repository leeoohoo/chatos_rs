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
            "[Task Execution Process]\nThe run-scoped system MCP tools `{tool_name}` and `{outcome_tool_name}` are available during this Task Runner run. Keep the visible process updated across the task instead of writing only one opening note. Record key steps and phase changes: task start, approach or root cause, completion of a major phase or artifact, important verification results, a changed path after failure, blockers, and next step. Do not log every tool call, every file, or each read/search/edit within the same phase; combine operations with one purpose into one clear update and add another update when the phase changes or a material result appears. After all implementation and verification work is finished, you must call `{outcome_tool_name}` exactly once with `succeeded`, `failed`, or `blocked` and a concrete reason. That outcome call must be your final tool call immediately before the user-facing final response. The runtime will reject a final response when no outcome has been reported. Keep entries concise. Do not record hidden chain-of-thought, credentials, secrets, raw dumps, or unrelated drafts. This MCP is mounted only inside the current Task Runner execution and is not part of the external Task Runner management API."
        )
    } else {
        format!(
            "[任务执行过程]\n本次 Task Runner 运行期间提供运行期系统 MCP 工具 `{tool_name}` 和 `{outcome_tool_name}`。过程记录应贯穿任务，不能只留一条开场说明。请在关键步骤和阶段变化时记录：任务开始、方案或根因确定、一个主要阶段或产物完成、关键验证结果、失败后的路径调整、阻塞与下一步。不要为每次工具调用、每个文件或同一阶段内的连续读取/搜索/编辑逐条记录；相同目的的操作合并成一条清晰进展，在阶段变化或出现实质结果时再更新。全部实现与验证结束后，你必须且只能调用一次 `{outcome_tool_name}`，明确上报 `succeeded`、`failed` 或 `blocked`，并给出具体理由。该终态上报必须是最终用户答复之前的最后一次工具调用；未上报终态时，运行时不会接受最终答复。记录要简洁。不要记录隐藏思维链、凭证、密钥、原始大段输出或无关草稿。这个 MCP 只挂载在当前 Task Runner 执行内部，不属于对外的 Task Runner 管理 API。"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_prompt_requires_updates_across_action_phases() {
        let chinese = task_process_log_preview_text(BuiltinMcpPromptLocale::ZhCn);
        assert!(chinese.contains("过程记录应贯穿任务"));
        assert!(chinese.contains("关键步骤和阶段变化"));
        assert!(chinese.contains("不要为每次工具调用"));

        let english = task_process_log_preview_text(BuiltinMcpPromptLocale::EnUs);
        assert!(english.contains("updated across the task"));
        assert!(english.contains("key steps and phase changes"));
        assert!(english.contains("Do not log every tool call"));
    }
}
