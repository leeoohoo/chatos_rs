use std::future::Future;
use std::sync::Arc;

use serde_json::Value;
use tracing::warn;

use crate::utils::abort_registry;

use super::{ToolResult, ToolResultCallback, ToolStreamChunkCallback};

pub async fn execute_tools_stream<F, Fut>(
    tool_calls: &[Value],
    conversation_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    on_tool_result: Option<ToolResultCallback>,
    mut call_tool_once: F,
) -> Vec<ToolResult>
where
    F: FnMut(String, Value, Option<ToolStreamChunkCallback>) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let mut results = Vec::new();
    let normalized_turn_id = conversation_turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    for tc in tool_calls {
        if is_aborted(conversation_id) {
            break;
        }

        let tool_name = tc
            .get("function")
            .and_then(|func| func.get("name"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();

        let call_id = tc
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();

        if tool_name.is_empty() {
            push_tool_result(
                &mut results,
                ToolResult {
                    tool_call_id: call_id,
                    name: "unknown".to_string(),
                    success: false,
                    is_error: true,
                    is_stream: false,
                    conversation_turn_id: normalized_turn_id.clone(),
                    content: "工具名称不能为空".to_string(),
                },
                conversation_id,
                on_tool_result.as_ref(),
            );
            continue;
        }

        let args_val = tc
            .get("function")
            .and_then(|func| func.get("arguments"))
            .cloned()
            .unwrap_or_else(|| Value::String("{}".to_string()));

        let args = match parse_tool_args(args_val) {
            Ok(value) => value,
            Err(err) => {
                warn!(
                    "[MCP] parse tool args failed: tool={}, call_id={}, err={}",
                    tool_name, call_id, err
                );
                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id.clone(),
                        name: tool_name.clone(),
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: normalized_turn_id.clone(),
                        content: format!("参数解析失败: {}", err),
                    },
                    conversation_id,
                    on_tool_result.as_ref(),
                );
                continue;
            }
        };

        let stream_turn_id = normalized_turn_id.clone();
        let on_stream_chunk = on_tool_result.as_ref().map(|callback| {
            let callback = Arc::clone(callback);
            let sid = conversation_id.map(|value| value.to_string());
            let stream_call_id = call_id.clone();
            let stream_tool_name = tool_name.clone();
            let stream_turn_id = stream_turn_id.clone();
            Arc::new(move |chunk: String| {
                if chunk.is_empty() {
                    return;
                }
                if !is_active(sid.as_deref()) {
                    return;
                }
                let event = ToolResult {
                    tool_call_id: stream_call_id.clone(),
                    name: stream_tool_name.clone(),
                    success: true,
                    is_error: false,
                    is_stream: true,
                    conversation_turn_id: stream_turn_id.clone(),
                    content: chunk,
                };
                callback(&event);
            }) as ToolStreamChunkCallback
        });

        match call_tool_once(tool_name.clone(), args, on_stream_chunk).await {
            Ok(text) => {
                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id,
                        name: tool_name,
                        success: true,
                        is_error: false,
                        is_stream: false,
                        conversation_turn_id: normalized_turn_id.clone(),
                        content: text,
                    },
                    conversation_id,
                    on_tool_result.as_ref(),
                );
            }
            Err(err) => {
                if err == "aborted" {
                    break;
                }
                warn!(
                    "[MCP] tool execution failed: tool={}, call_id={}, err={}",
                    tool_name, call_id, err
                );

                push_tool_result(
                    &mut results,
                    ToolResult {
                        tool_call_id: call_id,
                        name: tool_name,
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: normalized_turn_id.clone(),
                        content: format!("工具执行失败: {}", err),
                    },
                    conversation_id,
                    on_tool_result.as_ref(),
                );
            }
        }
    }

    results
}

fn parse_tool_args(args_val: Value) -> Result<Value, serde_json::Error> {
    if let Some(raw) = args_val.as_str() {
        serde_json::from_str::<Value>(raw)
    } else {
        Ok(args_val)
    }
}

fn push_tool_result(
    results: &mut Vec<ToolResult>,
    result: ToolResult,
    conversation_id: Option<&str>,
    on_tool_result: Option<&ToolResultCallback>,
) {
    results.push(result);

    let Some(callback) = on_tool_result else {
        return;
    };

    if !is_active(conversation_id) {
        return;
    }

    if let Some(last) = results.last() {
        callback(last);
    }
}

fn is_aborted(conversation_id: Option<&str>) -> bool {
    conversation_id
        .map(abort_registry::is_aborted)
        .unwrap_or(false)
}

fn is_active(conversation_id: Option<&str>) -> bool {
    !is_aborted(conversation_id)
}
