// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(super) const FINALIZATION_PROMPT: &str = "[Planning Delegation Finalization]\nThe program verified that a planning task was created for this turn and accepted for background execution. Do not call more tools. Return only a concise user-facing acknowledgement that planning has started and results will arrive through the normal completion callback. Do not claim that project artifacts are already complete, and do not expose internal tool names, service names, IDs, routing details, or protocol fields.";

fn successful_tool_result(result: &Value, names: &[&str]) -> bool {
    result
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| names.contains(&name))
        && result.get("success").and_then(Value::as_bool) == Some(true)
        && result.get("is_error").and_then(Value::as_bool) != Some(true)
}

pub(super) fn task_creation_succeeded(payload: &Value) -> bool {
    payload
        .get("tool_results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|result| {
            successful_tool_result(
                result,
                &[
                    "task_runner_service_create_task",
                    "task_runner_service_create_tasks_with_prerequisites",
                    "create_task",
                    "create_tasks_with_prerequisites",
                ],
            )
        })
}

pub(super) fn background_wait_succeeded(payload: &Value) -> bool {
    payload
        .get("tool_results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|result| {
            successful_tool_result(
                result,
                &[
                    "task_runner_service_wait_for_task_completion",
                    "wait_for_task_completion",
                ],
            )
        })
}
