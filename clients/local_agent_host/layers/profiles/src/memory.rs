// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{MemoryScope, RuntimeRecordOptions, SaveRecordInput};
use chatos_local_agent_protocol::LocalAgentRunClaim;
use chatos_mcp_runtime::ToolResult;
use serde_json::{json, Value};

use crate::MAIN_CHAT_PROFILE_KEY;

pub(super) struct LocalMemoryPlan {
    pub(super) thread_id: String,
    pub(super) turn_id: String,
    pub(super) scope: MemoryScope,
    pub(super) record_options: RuntimeRecordOptions,
    pub(super) user_record: Option<SaveRecordInput>,
    pub(super) external_tool_results: Vec<ToolResult>,
}

pub(super) fn memory_plan(
    claim: &LocalAgentRunClaim,
    source_id: &str,
    initial_text_field: &str,
) -> Result<LocalMemoryPlan, String> {
    let is_main_chat = claim.run.profile_key == MAIN_CHAT_PROFILE_KEY;
    let thread_id = if is_main_chat {
        optional_input_string(&claim.run.input, "conversation_id")
            .unwrap_or_else(|| claim.run.owner_entity_id.clone())
    } else {
        optional_input_string(&claim.run.input, "memory_thread_id")
            .unwrap_or_else(|| claim.run.owner_entity_id.clone())
    };
    let turn_id = if is_main_chat {
        optional_input_string(&claim.run.input, "turn_id")
            .unwrap_or_else(|| claim.run.owner_entity_id.clone())
    } else {
        claim.run.run_id.clone()
    };
    let metadata = json!({
        "tenant_id": claim.run.owner_user_id,
        "profile_key": claim.run.profile_key,
        "run_id": claim.run.run_id,
        "owner_entity_type": claim.run.owner_entity_type,
        "owner_entity_id": claim.run.owner_entity_id,
    });
    let message_mode = if is_main_chat {
        "main_chat"
    } else {
        "task_run"
    };
    let batch_id = claim
        .run
        .continuation_input
        .as_ref()
        .and_then(|value| value.get("batch_id"))
        .and_then(Value::as_str)
        .unwrap_or("model");
    let record_options = RuntimeRecordOptions::persist_all()
        .with_assistant_message_id(format!(
            "{}:assistant:{}",
            claim.run.run_id, claim.run.iteration
        ))
        .with_tool_message_id_prefix(format!("{}:tool:{batch_id}", claim.run.run_id))
        .with_assistant_message_mode(message_mode)
        .with_assistant_message_source(claim.run.profile_key.clone())
        .with_assistant_metadata(metadata.clone())
        .with_tool_message_mode(message_mode)
        .with_tool_message_source(claim.run.profile_key.clone())
        .with_tool_metadata(metadata.clone());
    let user_record = initial_user_record(
        claim,
        initial_text_field,
        thread_id.as_str(),
        turn_id.as_str(),
        message_mode,
        metadata,
    );
    Ok(LocalMemoryPlan {
        scope: MemoryScope::thread(
            claim.run.owner_user_id.clone(),
            source_id,
            thread_id.clone(),
        ),
        thread_id,
        turn_id,
        record_options,
        user_record,
        external_tool_results: external_tool_results(claim)?,
    })
}

fn initial_user_record(
    claim: &LocalAgentRunClaim,
    initial_text_field: &str,
    thread_id: &str,
    turn_id: &str,
    message_mode: &str,
    metadata: Value,
) -> Option<SaveRecordInput> {
    if !claim.run.checkpoint.is_null() || claim.run.model_attempt != 1 || claim.run.iteration > 1 {
        return None;
    }
    let content =
        optional_input_string(&claim.run.input, initial_text_field).unwrap_or_else(|| {
            claim
                .run
                .input
                .get("input_items")
                .map(Value::to_string)
                .unwrap_or_default()
        });
    let message_id = optional_input_string(&claim.run.input, "message_id")
        .unwrap_or_else(|| format!("{}:user", claim.run.run_id));
    Some(
        SaveRecordInput::user_message(thread_id, content)
            .with_conversation_turn_id(turn_id)
            .with_message_id(message_id)
            .with_message_mode(message_mode)
            .with_message_source(claim.run.profile_key.clone())
            .with_metadata(metadata),
    )
}

fn optional_input_string(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn external_tool_results(claim: &LocalAgentRunClaim) -> Result<Vec<ToolResult>, String> {
    let Some(continuation) = claim
        .run
        .continuation_input
        .as_ref()
        .filter(|value| value.get("type").and_then(Value::as_str) == Some("tool_results"))
    else {
        return Ok(Vec::new());
    };
    let invocations = continuation
        .get("invocations")
        .and_then(Value::as_array)
        .ok_or_else(|| "tool continuation requires invocations".to_string())?;
    invocations
        .iter()
        .map(|invocation| {
            let required = |key: &str| {
                invocation
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("tool result requires {key}"))
            };
            let status = required("status")?;
            let success = status == "succeeded";
            let result = invocation
                .get("result")
                .filter(|value| !value.is_null())
                .cloned();
            let error = invocation
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(ToolResult {
                tool_call_id: required("call_id")?.to_string(),
                name: required("tool_name")?.to_string(),
                success,
                is_error: !success,
                is_stream: false,
                conversation_turn_id: Some(if claim.run.profile_key == MAIN_CHAT_PROFILE_KEY {
                    optional_input_string(&claim.run.input, "turn_id")
                        .unwrap_or_else(|| claim.run.owner_entity_id.clone())
                } else {
                    claim.run.run_id.clone()
                }),
                content: result
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| error.to_string()),
                result,
                fatal_error: false,
                transient_model_input: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentRunStatus};

    fn claim(profile_key: &str, input: Value) -> LocalAgentRunClaim {
        LocalAgentRunClaim {
            worker_id: "worker-1".to_string(),
            claim_token: "claim-1".to_string(),
            run: LocalAgentRunRecord {
                run_id: "run-1".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: if profile_key == MAIN_CHAT_PROFILE_KEY {
                    "conversation_turn".to_string()
                } else {
                    "task".to_string()
                },
                owner_entity_id: if profile_key == MAIN_CHAT_PROFILE_KEY {
                    "turn-1".to_string()
                } else {
                    "task-1".to_string()
                },
                profile_key: profile_key.to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input,
                status: LocalAgentRunStatus::ModelRunning,
                iteration: 1,
                model_attempt: 1,
                max_iterations: 8,
                version: 2,
                claim_token: Some("claim-1".to_string()),
                claim_until_unix_ms: Some(20_000),
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                checkpoint: Value::Null,
                continuation_input: None,
                terminal_outcome: None,
                created_at_unix_ms: 1,
                updated_at_unix_ms: 2,
            },
        }
    }

    #[test]
    fn main_chat_memory_uses_conversation_thread_and_stable_message_ids() {
        let claim = claim(
            MAIN_CHAT_PROFILE_KEY,
            json!({
                "conversation_id": "conversation-1",
                "turn_id": "turn-1",
                "message_id": "message-1",
                "message": "hello"
            }),
        );
        let plan = memory_plan(&claim, "local_agent", "message").expect("plan");

        assert_eq!(plan.thread_id, "conversation-1");
        assert_eq!(plan.turn_id, "turn-1");
        assert_eq!(plan.scope.tenant_id, "user-1");
        assert_eq!(plan.scope.source_id, "local_agent");
        assert_eq!(plan.scope.thread_id, "conversation-1");
        let user = plan.user_record.expect("user record");
        assert_eq!(user.message_id.as_deref(), Some("message-1"));
        assert_eq!(user.conversation_id, "conversation-1");
        assert_eq!(
            plan.record_options.assistant_message_id.as_deref(),
            Some("run-1:assistant:1")
        );
        assert_eq!(user.metadata.expect("metadata")["tenant_id"], "user-1");
    }

    #[test]
    fn retries_and_checkpointed_steps_do_not_resave_initial_user_record() {
        let mut retry = claim(MAIN_CHAT_PROFILE_KEY, json!({"message": "hello"}));
        retry.run.model_attempt = 2;
        assert!(memory_plan(&retry, "local_agent", "message")
            .expect("retry plan")
            .user_record
            .is_none());

        let mut continued = claim(MAIN_CHAT_PROFILE_KEY, json!({"message": "hello"}));
        continued.run.checkpoint = json!({"response": {}});
        assert!(memory_plan(&continued, "local_agent", "message")
            .expect("continued plan")
            .user_record
            .is_none());
    }

    #[test]
    fn task_memory_uses_task_thread_and_decodes_external_tool_results() {
        let mut claim = claim("task_execution", json!({"prompt": "inspect repo"}));
        claim.run.iteration = 2;
        claim.run.continuation_input = Some(json!({
            "type": "tool_results",
            "batch_id": "batch-1",
            "invocations": [{
                "call_id": "call-1",
                "tool_name": "read_file",
                "status": "succeeded",
                "result": {"content": "ok"},
                "error": null
            }]
        }));
        let plan = memory_plan(&claim, "local_agent", "prompt").expect("plan");

        assert_eq!(plan.thread_id, "task-1");
        assert_eq!(plan.turn_id, "run-1");
        assert!(plan.user_record.is_none());
        assert_eq!(plan.external_tool_results.len(), 1);
        assert_eq!(plan.external_tool_results[0].tool_call_id, "call-1");
        assert_eq!(plan.external_tool_results[0].name, "read_file");
        assert!(plan.external_tool_results[0].success);
        assert_eq!(
            plan.record_options.tool_message_id_prefix.as_deref(),
            Some("run-1:tool:batch-1")
        );
    }
}
