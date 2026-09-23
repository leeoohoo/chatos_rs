// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::CloudAgentModelTrigger;
use chatos_cloud_agent_protocol::CloudAgentRunRecord;
use serde_json::Value;

pub fn cloud_agent_trigger_execution_identity(trigger: &CloudAgentModelTrigger) -> (String, usize) {
    match trigger {
        CloudAgentModelTrigger::RunStarted { .. } => ("initial".to_string(), 1),
        CloudAgentModelTrigger::ToolResults { .. } => ("tool_results".to_string(), 1),
        CloudAgentModelTrigger::Continuation { payload, .. } => (
            payload
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("continuation")
                .to_string(),
            1,
        ),
        CloudAgentModelTrigger::Retry {
            model_attempt,
            payload,
            ..
        } => (
            payload
                .get("retry_kind")
                .or_else(|| payload.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or("model_retry")
                .to_string(),
            (*model_attempt).max(1),
        ),
    }
}

pub fn cloud_agent_trigger_input_items(
    run: &CloudAgentRunRecord,
    trigger: &CloudAgentModelTrigger,
    initial_input_items: Vec<Value>,
) -> Result<Vec<Value>, String> {
    match trigger {
        CloudAgentModelTrigger::RunStarted { .. } => Ok(initial_input_items),
        CloudAgentModelTrigger::Continuation { .. } | CloudAgentModelTrigger::Retry { .. } => {
            Ok(if run.response_input_items.is_empty() {
                initial_input_items
            } else {
                run.response_input_items.clone()
            })
        }
        CloudAgentModelTrigger::ToolResults { items, .. } => cloud_agent_mcp_result_input_items(
            run.response_input_items.as_slice(),
            run.pending_tool_calls.as_slice(),
            items.as_slice(),
        ),
    }
}

pub fn cloud_agent_mcp_result_input_items(
    response_input_items: &[Value],
    calls: &[Value],
    results: &[Value],
) -> Result<Vec<Value>, String> {
    if calls.len() != results.len() {
        return Err("MCP aggregate result count does not match pending tool calls".to_string());
    }
    let mut items = Vec::with_capacity(response_input_items.len().saturating_add(calls.len()));
    items.extend_from_slice(response_input_items);
    for (index, (call, result)) in calls.iter().zip(results).enumerate() {
        let call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)
            .ok_or_else(|| format!("pending tool call {index} has no call id"))?;
        if result.get("status").and_then(Value::as_str) == Some("completed") {
            let name = chatos_ai_runtime::tool_call::extract_tool_call_name(call)
                .ok_or_else(|| format!("pending tool call {index} has no name"))?;
            let tool_result = chatos_mcp_runtime::execution::external_tool_result_from_value(
                call_id.to_string(),
                name.to_string(),
                None,
                result.get("result").unwrap_or(&Value::Null),
                None,
            );
            items.extend(chatos_ai_runtime::tool_runtime::build_tool_output_items(&[
                tool_result,
            ]));
        } else {
            let output = result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("MCP tool call failed")
                .to_string();
            items.push(
                chatos_ai_runtime::tool_call::build_function_call_output_item(
                    call_id,
                    output.as_str(),
                ),
            );
        }
    }
    Ok(items)
}

pub fn cloud_agent_mcp_result_callback_payload(
    calls: &[Value],
    results: &[Value],
) -> Result<Value, String> {
    if calls.len() != results.len() {
        return Err("MCP aggregate result count does not match pending tool calls".to_string());
    }
    let tool_results = calls
        .iter()
        .zip(results)
        .enumerate()
        .map(|(index, (call, result))| {
            let tool_call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)
                .ok_or_else(|| format!("pending tool call {index} has no call id"))?;
            let name = chatos_ai_runtime::tool_call::extract_tool_call_name(call)
                .ok_or_else(|| format!("pending tool call {index} has no name"))?;
            let completed = result.get("status").and_then(Value::as_str) == Some("completed");
            let normalized = if completed {
                Some(
                    chatos_mcp_runtime::execution::external_tool_result_from_value(
                        tool_call_id.to_string(),
                        name.to_string(),
                        call.get("conversation_turn_id")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        result.get("result").unwrap_or(&Value::Null),
                        None,
                    ),
                )
            } else {
                None
            };
            let content = normalized
                .as_ref()
                .map(|item| item.content.clone())
                .unwrap_or_else(|| {
                    result
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("MCP tool call failed")
                        .to_string()
                });
            let success = normalized
                .as_ref()
                .map(|item| item.success)
                .unwrap_or(false);
            let is_error = normalized
                .as_ref()
                .map(|item| item.is_error)
                .unwrap_or(true);
            let mut payload = serde_json::json!({
                "tool_call_id": tool_call_id,
                "name": name,
                "success": success,
                "is_error": is_error,
                "is_stream": false,
                "content": content,
                "result": normalized.and_then(|item| item.result).unwrap_or(Value::Null),
                "error": result.get("error").cloned().unwrap_or(Value::Null),
            });
            if let Some(invocation_id) = call.get("invocation_id").and_then(Value::as_str) {
                payload["invocation_id"] = Value::String(invocation_id.to_string());
            }
            if let Some(conversation_turn_id) =
                call.get("conversation_turn_id").and_then(Value::as_str)
            {
                payload["conversation_turn_id"] = Value::String(conversation_turn_id.to_string());
            }
            Ok(payload)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(serde_json::json!({ "tool_results": tool_results }))
}
