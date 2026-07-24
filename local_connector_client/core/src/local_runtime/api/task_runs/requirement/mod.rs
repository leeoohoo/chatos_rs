// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod confirm;
mod execute;
mod pause;
mod plan;
mod rerun;
mod stop;

use serde::Deserialize;

pub(super) use confirm::confirm_requirement_execution;
pub(super) use execute::execute_requirement;
pub(super) use pause::{pause_requirement_execution, resume_requirement_execution};
pub(super) use plan::get_requirement_execution_plan;
pub(super) use rerun::rerun_requirement_execution;
pub(super) use stop::stop_requirement;

#[derive(Debug, Default, Deserialize)]
pub(super) struct ExecuteRequirementPayload {
    #[allow(dead_code)]
    contact_id: Option<String>,
    #[serde(default, alias = "includePrerequisiteDependents")]
    #[allow(dead_code)]
    include_prerequisite_dependents: bool,
    #[serde(alias = "modelConfigId")]
    model_config_id: Option<String>,
    #[serde(alias = "planningFeedback")]
    planning_feedback: Option<String>,
    #[serde(alias = "replacesExecutionGroupId")]
    replaces_execution_group_id: Option<String>,
    #[serde(alias = "replacesConversationId")]
    replaces_conversation_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ConfirmRequirementExecutionPayload {
    #[serde(alias = "executionGroupId")]
    execution_group_id: String,
    #[serde(alias = "conversationId")]
    conversation_id: String,
}

type MutateRequirementExecutionDispatchPayload = ConfirmRequirementExecutionPayload;

#[derive(Debug, Default, Deserialize)]
pub(super) struct StopRequirementExecutionPayload {
    #[allow(dead_code)]
    contact_id: Option<String>,
    #[serde(alias = "executionGroupId")]
    execution_group_id: Option<String>,
    #[serde(alias = "conversationId")]
    conversation_id: Option<String>,
    #[serde(default, alias = "discardTasks")]
    discard_tasks: bool,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct RerunRequirementExecutionPayload {
    #[serde(alias = "executionGroupId")]
    execution_group_id: String,
    #[serde(alias = "conversationId")]
    conversation_id: String,
}

#[cfg(test)]
mod tests {
    use super::StopRequirementExecutionPayload;

    #[test]
    fn stop_payload_accepts_discard_tasks_in_both_naming_styles() {
        let snake: StopRequirementExecutionPayload = serde_json::from_value(serde_json::json!({
            "execution_group_id": "group-1",
            "conversation_id": "session-1",
            "discard_tasks": true
        }))
        .expect("snake case stop payload");
        assert!(snake.discard_tasks);

        let camel: StopRequirementExecutionPayload = serde_json::from_value(serde_json::json!({
            "executionGroupId": "group-1",
            "conversationId": "session-1",
            "discardTasks": true
        }))
        .expect("camel case stop payload");
        assert!(camel.discard_tasks);
    }
}
