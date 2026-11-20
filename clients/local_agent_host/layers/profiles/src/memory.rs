// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{MemoryScope, RuntimeRecordOptions, SaveRecordInput};
use chatos_local_agent_protocol::LocalAgentRunClaim;
use chatos_mcp_runtime::ToolResult;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{MAIN_CHAT_PROFILE_KEY, TASK_EXECUTION_PROFILE_KEY};

const JIGULI_AGENT_ID: &str = "jiguli";
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
    let is_task_execution = claim.run.profile_key == TASK_EXECUTION_PROFILE_KEY;
    let agent_id = memory_agent_id(claim)?;
    let agent_name = memory_agent_name(claim, agent_id.as_str())?;
    let project_id = memory_project_id(claim)?;
    let project_name = memory_project_name(claim)?;
    if project_name.is_some() && project_id.is_none() {
        return Err("Memory project_name requires project_id".to_string());
    }
    let thread_id = if !is_task_execution {
        format!("client-agent:agent:{agent_id}")
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
        "conversation_id": optional_input_string(&claim.run.input, "conversation_id"),
        "conversation_title": optional_input_string(&claim.run.input, "conversation_title"),
        "conversation_resource": claim.run.input.get("conversation_resource").cloned(),
        "ui_message_metadata": claim.run.input.get("ui_message_metadata").cloned(),
        "ui_attachments": claim.run.input.get("attachments").cloned(),
        "profile_key": claim.run.profile_key,
        "agent_id": agent_id,
        "agent_name": agent_name,
        "project_id": project_id,
        "project_name": project_name,
        "memory_project_id": project_id,
        "thread_project_aggregation_id": if is_task_execution { project_id.clone() } else { None },
        "thread_kind": if is_task_execution && memory_context_string(claim, "todo_id").is_some() { "todo_execution" } else if is_task_execution { "agent_task" } else if is_main_chat && agent_id == JIGULI_AGENT_ID { "main_chat" } else { "agent_conversation" },
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
    let mut assistant_metadata = metadata.clone();
    assistant_metadata["local_agent_step_claim"] = json!({
        "run_id": claim.run.run_id,
        "claim_token": claim.claim_token,
        "version": claim.run.version,
    });
    let record_options = RuntimeRecordOptions::persist_all()
        .with_assistant_message_id(format!(
            "{}:assistant:{}",
            claim.run.run_id, claim.run.iteration
        ))
        .with_tool_message_id_prefix(format!("{}:tool:{batch_id}", claim.run.run_id))
        .with_assistant_message_mode(message_mode)
        .with_assistant_message_source(claim.run.profile_key.clone())
        .with_assistant_metadata(assistant_metadata)
        .with_tool_message_mode(message_mode)
        .with_tool_message_source(claim.run.profile_key.clone())
        .with_tool_metadata(metadata.clone())
        .with_summary_on_final_response(terminal_memory_summary_requested(claim));
    let user_record = initial_user_record(
        claim,
        initial_text_field,
        thread_id.as_str(),
        turn_id.as_str(),
        message_mode,
        metadata,
    );
    let agent_subject_id = format!("agent:{agent_id}");
    let mut scope = MemoryScope::thread(
        claim.run.owner_user_id.clone(),
        source_id,
        thread_id.clone(),
    )
    .with_subject_id(agent_subject_id)
    .with_compose_context(
        optional_input_string(&claim.run.input, initial_text_field),
        Some(turn_id.clone()),
        claim.run.model_config_revision.clone(),
    );
    if let Some(project_id) = project_id {
        scope = scope.with_context_project_subject_id(format!("project:{project_id}"));
    }
    Ok(LocalMemoryPlan {
        scope,
        thread_id,
        turn_id,
        record_options,
        user_record,
        external_tool_results: external_tool_results(claim)?,
    })
}

pub(super) fn terminal_memory_summary_requested(claim: &LocalAgentRunClaim) -> bool {
    if claim.run.profile_key != TASK_EXECUTION_PROFILE_KEY {
        return false;
    }
    if claim.run.owner_entity_type == "task" {
        return task_outcome_reported(claim.run.continuation_input.as_ref());
    }
    successful_tool_result(
        claim.run.continuation_input.as_ref(),
        &["todo_complete", "todo_block"],
    )
}

fn task_outcome_reported(continuation: Option<&Value>) -> bool {
    successful_tool_result(continuation, &["task_run_process_report_outcome"])
        && continuation
            .and_then(|value| value.get("invocations"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|invocation| {
                invocation.get("tool_name").and_then(Value::as_str)
                    == Some("task_run_process_report_outcome")
                    && invocation
                        .get("result")
                        .and_then(|result| result.get("reported"))
                        .and_then(Value::as_bool)
                        == Some(true)
            })
}

fn successful_tool_result(continuation: Option<&Value>, tool_names: &[&str]) -> bool {
    continuation
        .and_then(|value| value.get("invocations"))
        .and_then(Value::as_array)
        .is_some_and(|invocations| {
            invocations.iter().any(|invocation| {
                invocation.get("status").and_then(Value::as_str) == Some("succeeded")
                    && invocation
                        .get("tool_name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| tool_names.contains(&name))
            })
        })
}

fn memory_agent_id(claim: &LocalAgentRunClaim) -> Result<String, String> {
    let agent_id = consistent_memory_context_string(claim, "agent_id")?
        .ok_or_else(|| "Memory identity requires an explicit agent_id".to_string())?;
    if matches!(
        agent_id.as_str(),
        "main-chat" | "main_chat" | "task-execution" | "task_execution"
    ) {
        return Err("Memory identity must not use a runtime profile key as agent_id".to_string());
    }
    Ok(agent_id)
}

fn memory_project_id(claim: &LocalAgentRunClaim) -> Result<Option<String>, String> {
    consistent_memory_context_string(claim, "project_id")
}

fn memory_project_name(claim: &LocalAgentRunClaim) -> Result<Option<String>, String> {
    consistent_memory_context_string(claim, "project_name")
}

fn memory_agent_name(claim: &LocalAgentRunClaim, agent_id: &str) -> Result<Option<String>, String> {
    Ok(consistent_memory_context_string(claim, "agent_name")?
        .or_else(|| (agent_id == JIGULI_AGENT_ID).then(|| "叽咕狸".to_string())))
}

fn memory_context_string(claim: &LocalAgentRunClaim, key: &str) -> Option<String> {
    optional_input_string(&claim.run.input, key).or_else(|| {
        [
            "agent_execution_context",
            "agent_team_execution",
            "team_executor_context",
        ]
        .into_iter()
        .find_map(|container| nested_memory_context_string(claim, container, key))
    })
}

fn consistent_memory_context_string(
    claim: &LocalAgentRunClaim,
    key: &str,
) -> Result<Option<String>, String> {
    let values = [
        nested_memory_context_string(claim, "agent_execution_context", key),
        optional_input_string(&claim.run.input, key),
        nested_memory_context_string(claim, "agent_team_execution", key),
        nested_memory_context_string(claim, "team_executor_context", key),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let Some(value) = values.first() else {
        return Ok(None);
    };
    if values.iter().any(|candidate| candidate != value) {
        return Err(format!("Memory identity has conflicting {key} values"));
    }
    Ok(Some(value.clone()))
}

fn nested_memory_context_string(
    claim: &LocalAgentRunClaim,
    container: &str,
    key: &str,
) -> Option<String> {
    claim
        .run
        .input
        .get(container)
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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
mod tests;
