// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{MemoryScope, RuntimeRecordOptions, SaveRecordInput};
use chatos_local_agent_protocol::LocalAgentRunClaim;
use chatos_mcp_runtime::ToolResult;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::MAIN_CHAT_PROFILE_KEY;

const INLINE_TOOL_RESULT_MAX_BYTES: usize = 160_000;
const INLINE_TOOL_RESULTS_TOTAL_MAX_BYTES: usize = 800_000;
const TOOL_RESULT_READ_MAX_CHARS: usize = 8_000;

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
        optional_input_string(&claim.run.input, "memory_thread_id").unwrap_or_else(|| {
            if task_retry_instructions(&claim.run.input).is_some() {
                claim.run.run_id.clone()
            } else {
                claim.run.owner_entity_id.clone()
            }
        })
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

fn task_retry_instructions(input: &Value) -> Option<&Vec<Value>> {
    input
        .get("retry_instructions")
        .and_then(Value::as_array)
        .filter(|instructions| !instructions.is_empty())
}

pub(super) fn external_tool_results(claim: &LocalAgentRunClaim) -> Result<Vec<ToolResult>, String> {
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
    let mut inline_bytes = 0usize;
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
            let call_id = required("call_id")?.to_string();
            let tool_name = required("tool_name")?.to_string();
            let conversation_turn_id = Some(if claim.run.profile_key == MAIN_CHAT_PROFILE_KEY {
                optional_input_string(&claim.run.input, "turn_id")
                    .unwrap_or_else(|| claim.run.owner_entity_id.clone())
            } else {
                claim.run.run_id.clone()
            });
            if success {
                let result = result.unwrap_or(Value::Null);
                let projected = chatos_mcp_runtime::execution::external_tool_result_from_value(
                    call_id.clone(),
                    tool_name.clone(),
                    conversation_turn_id,
                    &result,
                    Some(usize::MAX),
                );
                return Ok(progressive_tool_result(
                    projected,
                    &call_id,
                    &tool_name,
                    &mut inline_bytes,
                ));
            }
            let projected = chatos_mcp_runtime::execution::external_tool_failure_from_value(
                call_id.clone(),
                tool_name.clone(),
                conversation_turn_id,
                result.as_ref(),
                error,
                Some(usize::MAX),
            );
            Ok(progressive_tool_result(
                projected,
                &call_id,
                &tool_name,
                &mut inline_bytes,
            ))
        })
        .collect()
}

fn progressive_tool_result(
    mut result: ToolResult,
    source_call_id: &str,
    tool_name: &str,
    inline_bytes: &mut usize,
) -> ToolResult {
    let reference_content =
        chatos_mcp_runtime::execution::external_tool_result_reference_content(&result);
    let result_bytes = serde_json::to_vec(&result)
        .map(|encoded| encoded.len())
        .unwrap_or_else(|_| reference_content.len());
    if result_bytes <= INLINE_TOOL_RESULT_MAX_BYTES
        && inline_bytes.saturating_add(result_bytes) <= INLINE_TOOL_RESULTS_TOTAL_MAX_BYTES
    {
        *inline_bytes += result_bytes;
        return result;
    }
    let total_chars = reference_content.chars().count();
    let total_bytes = reference_content.len();
    let sha256 = format!("{:x}", Sha256::digest(reference_content.as_bytes()));
    let descriptor = json!({
        "type": "tool_result_reference",
        "source_call_id": source_call_id,
        "source_tool_name": tool_name,
        "total_chars": total_chars,
        "total_bytes": total_bytes,
        "sha256": sha256,
        "source_format": if result.result.is_some() { "tool_result_envelope_json" } else { "text" },
        "read_tool": "tool_result_read",
        "query_tool": "tool_result_query",
        "max_page_chars": TOOL_RESULT_READ_MAX_CHARS,
        "complete": false,
        "guidance": "This result is complete in the durable tool ledger. Prefer 1-3 targeted tool_result_query calls (json_pointer, search, head, tail, or range). Use tool_result_read only when exact full reconstruction is required; concatenate pages in offset order and verify sha256."
    });
    result.content = descriptor.to_string();
    result.result = Some(descriptor);
    *inline_bytes = inline_bytes.saturating_add(
        serde_json::to_vec(&result)
            .map(|encoded| encoded.len())
            .unwrap_or_else(|_| result.content.len()),
    );
    result
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

    #[test]
    fn retried_task_uses_a_fresh_run_memory_thread() {
        let claim = claim(
            "task_execution",
            json!({
                "prompt": "review the complete solution again",
                "retry_instructions": ["重新读取完整方案，不要复用上一次的截断结论"]
            }),
        );

        let plan = memory_plan(&claim, "local_agent", "prompt").expect("retry plan");

        assert_eq!(plan.thread_id, "run-1");
        assert_eq!(plan.scope.thread_id, "run-1");
        assert_eq!(plan.turn_id, "run-1");
    }

    #[test]
    fn nineteen_thousand_character_solution_result_is_preserved_without_truncation() {
        let content = "方案".repeat(9_980) + "终";
        assert_eq!(content.chars().count(), 19_961);
        let mut claim = claim("task_execution", json!({"prompt": "review solution"}));
        claim.run.continuation_input = Some(json!({
            "type": "tool_results",
            "batch_id": "batch-solution",
            "invocations": [{
                "call_id": "call-solution",
                "tool_name": "capability_invoke",
                "status": "succeeded",
                "result": content,
                "error": null
            }]
        }));

        let result = external_tool_results(&claim).unwrap().remove(0);

        assert_eq!(result.content, content);
        assert!(!result.content.contains("truncated"));
        assert!(!result.content.contains("tool_result_reference"));
    }

    #[test]
    fn over_one_hundred_sixty_thousand_bytes_returns_a_durable_page_reference() {
        let content = "x".repeat(INLINE_TOOL_RESULT_MAX_BYTES + 1);
        let mut claim = claim("task_execution", json!({"prompt": "review large result"}));
        claim.run.continuation_input = Some(json!({
            "type": "tool_results",
            "invocations": [{
                "call_id": "call-large",
                "tool_name": "capability_invoke",
                "status": "succeeded",
                "result": content
            }]
        }));

        let result = external_tool_results(&claim).unwrap().remove(0);
        let descriptor: Value = serde_json::from_str(&result.content).expect("descriptor");

        assert_eq!(descriptor["type"], "tool_result_reference");
        assert_eq!(descriptor["source_call_id"], "call-large");
        assert_eq!(descriptor["total_chars"], INLINE_TOOL_RESULT_MAX_BYTES + 1);
        assert_eq!(descriptor["total_bytes"], INLINE_TOOL_RESULT_MAX_BYTES + 1);
        assert_eq!(descriptor["read_tool"], "tool_result_read");
        assert_eq!(descriptor["sha256"].as_str().unwrap().len(), 64);
        assert!(!result.content.contains("...[truncated"));
    }

    #[test]
    fn browser_screenshots_in_structured_result_become_a_durable_reference() {
        let screenshot = "a".repeat(621_248);
        let mut claim = claim("task_execution", json!({"prompt": "verify browser"}));
        claim.run.continuation_input = Some(json!({
            "type": "tool_results",
            "invocations": [{
                "call_id": "call-browser",
                "tool_name": "capability_invoke",
                "status": "succeeded",
                "result": {
                    "content": {
                        "content": [{
                            "type": "text",
                            "text": "[Duplicate JSON text omitted; use structuredContent as the canonical result.]"
                        }],
                        "structuredContent": {
                            "normalized_result": {
                                "steps": [
                                    {"artifacts": {"screenshot": {"value": {"data_base64": screenshot}}}},
                                    {"artifacts": {"screenshot": {"value": {"data_base64": screenshot}}}}
                                ]
                            }
                        }
                    }
                }
            }]
        }));

        let result = external_tool_results(&claim).unwrap().remove(0);
        let descriptor: Value = serde_json::from_str(&result.content).expect("descriptor");

        assert_eq!(descriptor["type"], "tool_result_reference");
        assert_eq!(descriptor["source_call_id"], "call-browser");
        assert_eq!(descriptor["source_format"], "tool_result_envelope_json");
        assert!(descriptor["total_bytes"].as_u64().unwrap() > 1_048_576);
        assert!(serde_json::to_vec(&result).unwrap().len() < 8_000);
        assert!(!result.content.contains(&"a".repeat(8_192)));
    }

    #[test]
    fn cumulative_large_results_use_references_before_the_runtime_total_budget() {
        let content = "x".repeat(140_000);
        let invocations: Vec<Value> = (0..6)
            .map(|index| {
                json!({
                    "call_id": format!("call-{index}"),
                    "tool_name": "capability_invoke",
                    "status": "succeeded",
                    "result": content
                })
            })
            .collect();
        let mut claim = claim("task_execution", json!({"prompt": "review result batch"}));
        claim.run.continuation_input = Some(json!({
            "type": "tool_results",
            "invocations": invocations
        }));

        let results = external_tool_results(&claim).unwrap();

        assert_eq!(results.len(), 6);
        assert!(results[..5]
            .iter()
            .all(|result| result.content.chars().count() == 140_000));
        assert!(results[5].content.contains("tool_result_reference"));
        assert!(!results[5].content.contains("tool_result_truncated"));
    }

    #[test]
    fn failed_external_tools_preserve_actionable_details_for_both_profiles() {
        for profile in [MAIN_CHAT_PROFILE_KEY, "task_execution"] {
            let mut claim = claim(profile, json!({"message": "browse", "turn_id": "turn-1"}));
            let detail = json!({"content": {"content": [
                {"type": "text", "text": "no browser session is bound; call browser_session_open first"},
                {"type": "text", "text": "Then retry browser_navigate with a non-empty url."},
                {"type": "image", "data": "/9j/AA==", "mimeType": "image/jpeg"}
            ]}, "is_error": true});
            claim.run.continuation_input = Some(json!({
                "type": "tool_results", "invocations": [{
                    "call_id": "call-failed", "tool_name": "capability_invoke", "status": "failed",
                    "error": "native_tool_reported_error", "result": detail
                }]
            }));
            let plan = memory_plan(&claim, "local_agent", "prompt").unwrap();
            let result = &plan.external_tool_results[0];
            assert!(!result.success && result.is_error && !result.fatal_error);
            assert_eq!(result.content, "native_tool_reported_error\nno browser session is bound; call browser_session_open first\nThen retry browser_navigate with a non-empty url.");
            assert_eq!(result.result.as_ref(), Some(&detail));
            assert!(result.transient_model_input.is_none());
            assert_eq!(
                result.conversation_turn_id.as_deref(),
                Some(if profile == MAIN_CHAT_PROFILE_KEY {
                    "turn-1"
                } else {
                    "run-1"
                })
            );
        }
    }

    #[test]
    fn failed_external_tools_support_plain_structured_and_missing_diagnostics() {
        let cases = [
            (
                json!({"content": "url must be a non-empty string"}),
                "bad_arguments",
                "url must be a non-empty string",
            ),
            (
                json!({"content": {"error": "skill_activation_required", "required_skill_refs": ["product-skill:test"]}}),
                "native_tool_reported_error",
                "product-skill:test",
            ),
            (
                json!("permission denied"),
                "permission_denied",
                "permission denied",
            ),
            (Value::Null, "connection_lost", "connection_lost"),
            (
                json!({"is_error": true, "content": {"message": "runtime unavailable"}}),
                "",
                "runtime unavailable",
            ),
            (Value::Null, "", "tool execution failed"),
        ];
        for (detail, error, expected) in cases {
            let mut claim = claim(MAIN_CHAT_PROFILE_KEY, json!({"message": "test"}));
            claim.run.continuation_input = Some(json!({"type": "tool_results", "invocations": [{
                "call_id": "call", "tool_name": "external_mcp__test", "status": "failed",
                "result": detail, "error": error
            }]}));
            let result = external_tool_results(&claim).unwrap().remove(0);
            assert!(result.content.contains(expected), "{}", result.content);
            assert!(error.is_empty() || result.content.starts_with(error));
            assert!(result.is_error && !result.success);
        }
    }

    #[test]
    fn task_memory_restores_wrapped_mcp_images_as_transient_model_input() {
        let mut claim = claim("task_execution", json!({"task": "observe"}));
        claim.run.continuation_input = Some(json!({
            "type": "tool_results",
            "batch_id": "batch-visual",
            "invocations": [{
                "call_id": "call-visual",
                "tool_name": "capability_invoke",
                "status": "succeeded",
                "result": {
                    "content": {
                        "content": [
                            {"type": "text", "text": "Active application: ChatOS"},
                            {"type": "image", "data": "/9j/AA==", "mimeType": "image/jpeg"}
                        ],
                        "structuredContent": {
                            "activeApplication": {"name": "ChatOS"}
                        }
                    },
                    "is_error": false,
                    "made_progress": true
                }
            }]
        }));

        let plan = memory_plan(&claim, "local_agent", "task").expect("memory plan");
        let result = plan.external_tool_results.first().expect("tool result");

        assert_eq!(result.content, "Active application: ChatOS");
        assert_eq!(
            result
                .result
                .as_ref()
                .and_then(|value| value.pointer("/activeApplication/name"))
                .and_then(Value::as_str),
            Some("ChatOS")
        );
        assert_eq!(
            result
                .transient_model_input
                .as_ref()
                .and_then(|input| input.items().first())
                .and_then(|value| value.get("image_url"))
                .and_then(Value::as_str),
            Some("data:image/jpeg;base64,/9j/AA==")
        );
    }
}
