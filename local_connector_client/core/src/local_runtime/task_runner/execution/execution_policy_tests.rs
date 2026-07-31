// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{
    RuntimeIterationContext, RuntimeLifecycleHook, TaskFinalizationLifecycleHook,
    DEFAULT_TASK_RUN_MAX_ITERATIONS,
};
use serde_json::Value;

use super::{local_task_runner_agent_key, task_run_idempotency_key};

#[tokio::test]
async fn implementation_tasks_reserve_a_tool_free_finalization_round() {
    assert_eq!(DEFAULT_TASK_RUN_MAX_ITERATIONS, 600);
    let hook = TaskFinalizationLifecycleHook::new(DEFAULT_TASK_RUN_MAX_ITERATIONS);

    let normal = hook
        .before_model_request(iteration_context(hook.finalization_iteration() - 1))
        .await
        .expect("normal task iteration");
    assert!(normal.tools_enabled);
    assert!(normal.input_items.is_empty());

    let finalization = hook
        .before_model_request(iteration_context(hook.finalization_iteration()))
        .await
        .expect("task finalization iteration");
    assert!(!finalization.tools_enabled);
    assert_eq!(finalization.input_items.len(), 1);
    assert!(finalization.input_items[0]
        .to_string()
        .contains("不要再调用任何工具"));
}

#[test]
fn retry_attempts_use_distinct_turn_idempotency_keys() {
    assert_ne!(
        task_run_idempotency_key("run-1", 1),
        task_run_idempotency_key("run-1", 2),
    );
}

#[test]
fn local_task_runner_execution_never_uses_cloud_agent_keys() {
    assert_eq!(
        local_task_runner_agent_key(true).as_str(),
        "task_runner_local_plan_phase"
    );
    assert_eq!(
        local_task_runner_agent_key(false).as_str(),
        "task_runner_local_run_phase"
    );
}

fn iteration_context(iteration: usize) -> RuntimeIterationContext {
    RuntimeIterationContext {
        conversation_id: Some("session-1".to_string()),
        conversation_turn_id: Some("turn-1".to_string()),
        iteration,
        reason: "tool_results".to_string(),
        input: Value::Array(Vec::new()),
    }
}
