// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::task_tools::{envelope, LocalTaskToolExecutor};
use chatos_local_agent_ports::ClientStorageError;
use chatos_local_agent_protocol::{
    HostCommand, HostResult, LocalAgentRunRecord, LocalAgentToolInvocationRecord,
    ResolveTaskFollowupCommand, TaskFollowupDecision,
};
use chatos_local_agent_runtime::LocalAgentRuntimeError;
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
    let review = parent
        .input
        .get("task_followup_review")
        .filter(|v| v["pending"] == true)
        .ok_or("no pending task_followup_review for this Run; do not invent a review_message_id or reuse historical task ids. Use list_tasks/get_task to answer status questions, or use only the current pending review supplied in Run context")?;
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
        let mut pending_task_ids = expected.into_iter().collect::<Vec<_>>();
        pending_task_ids.sort_unstable();
        let current = json!({
            "review_message_id": review["review_message_id"],
            "pending_task_ids": pending_task_ids,
        });
        return Err(format!(
            "review must decide every current candidate exactly once; stale or foreign task ids are rejected. Retry with this authoritative current review: {current}"
        ));
    }
    *side_effects_started = true;
    let result = executor
        .runtime
        .try_handle(envelope(
            format!("task-followup-{}", invocation.invocation_id),
            HostCommand::ResolveTaskFollowup(command),
        ))
        .await;
    let result = match result {
        // This command performs all writes in one storage transaction. A typed
        // conflict means it was rejected/rolled back, even if the review changed
        // after the preflight read. Do not turn that known failure into Unknown.
        Err(error @ LocalAgentRuntimeError::Storage(ClientStorageError::Conflict(_))) => {
            *side_effects_started = false;
            return Err(format!(
                "{error}; use only the current pending task_followup_review from Run context"
            ));
        }
        // Transport, database and unexpected-response errors remain uncertain.
        other => other.map_err(|error| error.to_string())?,
    };
    match result {
        HostResult::TaskFollowupReview { review } => Ok(review),
        other => Err(format!("unexpected follow-up review response: {other:?}")),
    }
}

pub(super) fn definition() -> Value {
    json!({
        "type": "function", "name": "resolve_task_followup",
        "description": "Use ONLY when the current Run context contains task_followup_review with pending=true. The context marked context_authority=current_run supersedes every historical review in retained conversation or checkpoint data. Copy its review_message_id and decide every pending candidate exactly once; never invent a token or reuse historical task ids. For status questions without a pending review, use list_tasks/get_task instead. continue releases unchanged work; cancel atomically stops conflicting work and its dependents; clarify keeps execution held pending user confirmation. Old review tokens and omitted or foreign tasks are rejected with the current token and candidate ids so the call can be corrected.",
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
