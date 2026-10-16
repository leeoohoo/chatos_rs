// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::task_tools::{envelope, LocalTaskToolExecutor};
use chatos_local_agent_protocol::{
    HostCommand, HostResult, LocalAgentRunRecord, LocalAgentToolInvocationRecord,
    ResolveTaskFollowupCommand, TaskFollowupDecision,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    review_message_id: String,
    decisions: Vec<TaskFollowupDecision>,
}

pub(super) async fn resolve(
    executor: &LocalTaskToolExecutor,
    invocation: &LocalAgentToolInvocationRecord,
    parent: &LocalAgentRunRecord,
    side_effects_started: &mut bool,
) -> Result<Value, String> {
    let args: Args = serde_json::from_value(invocation.arguments.clone())
        .map_err(|error| format!("invalid resolve_task_followup input: {error}"))?;
    let command = ResolveTaskFollowupCommand {
        owner_user_id: parent.owner_user_id.clone(),
        run_id: parent.run_id.clone(),
        review_message_id: args.review_message_id,
        decisions: args.decisions,
    };
    command.validate()?;
    // Parent facts are freshly loaded for this invocation. Catch obvious stale/omitted
    // decisions before entering the transaction; storage still rechecks races atomically.
    if let Some(review) = parent
        .input
        .get("task_followup_review")
        .filter(|v| v["pending"] == true)
    {
        let expected = review["tasks"]
            .as_array()
            .ok_or("invalid pending follow-up review")?
            .iter()
            .filter(|t| t["review_state"] == "pending")
            .map(|t| t["task_id"].as_str().ok_or("invalid pending review task"))
            .collect::<Result<HashSet<_>, _>>()?;
        let supplied = command
            .decisions
            .iter()
            .map(|d| d.task_id.as_str())
            .collect::<HashSet<_>>();
        if review["review_message_id"] != command.review_message_id
            || expected.is_empty()
            || expected != supplied
        {
            return Err("review must decide every current candidate exactly once; stale or foreign task ids are rejected".into());
        }
    }
    *side_effects_started = true;
    let result = executor
        .runtime
        .try_handle(envelope(
            format!("task-followup-{}", invocation.invocation_id),
            HostCommand::ResolveTaskFollowup(command),
        ))
        .await
        .map_err(|error| error.to_string())?;
    match result {
        HostResult::TaskFollowupReview { review } => Ok(review),
        other => Err(format!("unexpected follow-up review response: {other:?}")),
    }
}

pub(super) fn definition() -> Value {
    json!({
        "type": "function", "name": "resolve_task_followup",
        "description": "Commit decisions for every pending task_followup_review candidate. continue releases unchanged work; cancel atomically stops conflicting work and its dependents; clarify keeps execution held pending user confirmation. Old review tokens and omitted or foreign tasks are rejected.",
        "parameters": {
            "type": "object", "additionalProperties": false,
            "properties": {
                "review_message_id": {"type": "string", "minLength": 1},
                "decisions": {"type": "array", "minItems": 1, "maxItems": 500,
                    "items": {"type": "object", "additionalProperties": false,
                        "properties": {
                            "task_id": {"type": "string", "minLength": 1},
                            "action": {"type": "string", "enum": ["continue", "cancel", "clarify"]},
                            "reason": {"type": "string", "minLength": 1, "maxLength": 1000}
                        }, "required": ["task_id", "action", "reason"]
                    }
                }
            }, "required": ["review_message_id", "decisions"]
        }
    })
}
