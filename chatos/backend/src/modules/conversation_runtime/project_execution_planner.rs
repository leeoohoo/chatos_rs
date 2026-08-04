// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(super) const FINALIZATION_PROMPT: &str =
    "[Execution Plan Finalization]\nThe program has verified that the complete execution task graph was persisted successfully and is awaiting user confirmation. Do not call any more tools. Return only a concise user-facing confirmation that the plan is ready to preview and that execution will begin only after the user confirms it. Do not expose internal tool names, service names, IDs, routing details, or protocol fields.";

pub(super) fn materialization_succeeded(payload: &Value) -> bool {
    payload
        .get("tool_results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|result| {
            result.get("name").and_then(Value::as_str)
                == Some("task_runner_service_create_project_execution_tasks")
                && result.get("success").and_then(Value::as_bool) == Some(true)
                && result.get("is_error").and_then(Value::as_bool) != Some(true)
        })
}
