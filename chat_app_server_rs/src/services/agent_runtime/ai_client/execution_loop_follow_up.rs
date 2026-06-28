use serde_json::{json, Value};

use crate::core::mcp_tools::ToolResult;
use crate::modules::conversation_runtime::task_board::TaskTurnFollowUpMode;
use crate::services::ai_common::attach_ai_client_success_extra;

use super::AiClientCallbacks;

pub(super) const TASK_RUNNER_ASYNC_PLANNER_FINAL_SUMMARY_PROMPT: &str = "Task planning is complete. Do not call any more tools. Reply to the user now with a concise final summary that confirms the tasks were created or adjusted, summarizes the execution plan and prerequisite relationships, and states that the tasks will run automatically in the background and results will be sent later when completed.\n任务安排已经完成。不要再调用任何工具。现在直接给用户简要总结：确认任务已创建或调整，概括执行计划和前置关系，并说明任务会在后台自动执行，完成后会再把结果发送给用户。";

pub(super) fn task_runner_async_planner_requested_final_summary(
    tool_results: &[ToolResult],
) -> bool {
    tool_results
        .iter()
        .any(|result| result.success && result.name.as_str() == "wait_for_task_completion")
}

pub(super) fn should_persist_tool_messages_for_turn(
    purpose: &str,
    _task_runner_async_plan_mode: bool,
) -> bool {
    purpose != "agent_builder"
}

pub(super) fn emit_turn_phase_event(
    callbacks: &AiClientCallbacks,
    phase: &'static str,
    mode: Option<TaskTurnFollowUpMode>,
    iteration: i64,
) {
    if let Some(cb) = &callbacks.on_turn_phase {
        cb(json!({
            "phase": phase,
            "reason": "task_follow_up",
            "task_follow_up_mode": mode.map(|item| match item {
                TaskTurnFollowUpMode::ContinueExecution => "continue",
                TaskTurnFollowUpMode::ReviewExecution => "review",
            }),
            "iteration": iteration
        }));
    }
}

fn build_review_metadata_payload(attempted: bool, outcome: &str, rounds: usize) -> Value {
    json!({
        "task_turn_review": {
            "attempted": attempted,
            "outcome": outcome,
            "rounds": rounds
        }
    })
}

pub(super) fn attach_review_metadata(
    payload: Value,
    attempted: bool,
    outcome: &str,
    rounds: usize,
) -> Value {
    attach_ai_client_success_extra(
        payload,
        build_review_metadata_payload(attempted, outcome, rounds),
    )
}

#[cfg(test)]
mod tests {
    use super::should_persist_tool_messages_for_turn;

    #[test]
    fn task_runner_async_planner_still_persists_tool_messages() {
        assert!(should_persist_tool_messages_for_turn("chat", true));
    }

    #[test]
    fn agent_builder_keeps_tool_message_persistence_disabled() {
        assert!(!should_persist_tool_messages_for_turn(
            "agent_builder",
            false
        ));
    }
}
