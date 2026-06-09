use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::task::{Id, JoinSet};
use tracing::{error, warn};

use crate::core::mcp_tools::{
    execute_tools_stream as execute_tools_stream_common, inject_agent_builder_args,
    jsonrpc_http_call, jsonrpc_stdio_call, to_text_and_structured_result, BuiltinToolService,
    ToolInfo, ToolResult, ToolResultCallback, ToolStreamChunkCallback,
};
use crate::core::tool_call::{
    clone_tool_call_arguments, extract_tool_call_id, extract_tool_call_name,
};
use crate::utils::abort_registry;

use super::parallelism::should_parallelize_tool_batch;

const TASK_RUNNER_MCP_SERVER_NAME: &str = "task_runner_service";

pub(crate) fn tool_call_name(tool_call: &Value) -> Option<&str> {
    extract_tool_call_name(tool_call)
}

pub(crate) fn tool_call_id(tool_call: &Value) -> Option<&str> {
    extract_tool_call_id(tool_call)
}

pub(crate) fn parse_tool_args(args_val: Value) -> Result<Value, serde_json::Error> {
    if let Some(raw) = args_val.as_str() {
        serde_json::from_str::<Value>(raw)
    } else {
        Ok(args_val)
    }
}

pub(crate) async fn execute_tools_stream_with_registry(
    tool_calls: &[Value],
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    caller_model: Option<&str>,
    on_tool_result: Option<ToolResultCallback>,
    tool_metadata: &HashMap<String, ToolInfo>,
    builtin_services: &HashMap<String, BuiltinToolService>,
) -> Vec<ToolResult> {
    if should_parallelize_tool_batch(tool_calls, tool_metadata) {
        return execute_tools_stream_parallel_with_registry(
            tool_calls,
            session_id,
            conversation_turn_id,
            caller_model,
            on_tool_result,
            tool_metadata,
            builtin_services,
        )
        .await;
    }

    execute_tools_stream_common(
        tool_calls,
        session_id,
        conversation_turn_id,
        on_tool_result,
        |tool_name, args, on_stream_chunk| async move {
            call_tool_once(
                tool_metadata,
                builtin_services,
                tool_name.as_str(),
                args,
                session_id,
                conversation_turn_id,
                caller_model,
                on_stream_chunk,
            )
            .await
        },
    )
    .await
}

pub(crate) async fn call_tool_once(
    tool_metadata: &HashMap<String, ToolInfo>,
    builtin_services: &HashMap<String, BuiltinToolService>,
    tool_name: &str,
    args: Value,
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    caller_model: Option<&str>,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
) -> Result<(String, Option<Value>), String> {
    let info = tool_metadata
        .get(tool_name)
        .ok_or_else(|| format!("工具未找到: {}", tool_name))?;

    if info.server_type == "http" {
        let url = info.server_url.clone().ok_or("missing server url")?;
        let headers = http_tool_call_headers(info, session_id, conversation_turn_id);
        let result = jsonrpc_http_call(
            &url,
            headers.as_ref(),
            "tools/call",
            json!({"name": info.original_name, "arguments": args}),
        )
        .await?;
        Ok(to_text_and_structured_result(&result))
    } else if info.server_type == "builtin" {
        let service = builtin_services
            .get(&info.server_name)
            .ok_or_else(|| "missing builtin service".to_string())?;

        let args = if matches!(service, BuiltinToolService::AgentBuilder(_)) {
            inject_agent_builder_args(args, caller_model)
        } else {
            args
        };

        let result = service.call_tool(
            &info.original_name,
            args,
            session_id,
            conversation_turn_id,
            on_stream_chunk,
        )?;
        Ok(to_text_and_structured_result(&result))
    } else {
        let config = info.server_config.clone().ok_or("missing server config")?;
        let result = jsonrpc_stdio_call(
            &config,
            "tools/call",
            json!({"name": info.original_name, "arguments": args}),
            session_id,
        )
        .await?;
        Ok(to_text_and_structured_result(&result))
    }
}

fn http_tool_call_headers(
    info: &ToolInfo,
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
) -> Option<HashMap<String, String>> {
    let mut headers = info.server_headers.clone().unwrap_or_default();
    if info.server_name == TASK_RUNNER_MCP_SERVER_NAME {
        if let Some(session_id) = normalized_context_value(session_id) {
            headers.insert("X-Chatos-Session-Id".to_string(), session_id.clone());
            headers.insert("X-Chatos-Conversation-Id".to_string(), session_id);
        }
        if let Some(turn_id) = normalized_context_value(conversation_turn_id) {
            headers.insert("X-Chatos-Turn-Id".to_string(), turn_id);
        }
    }
    (!headers.is_empty()).then_some(headers)
}

fn normalized_context_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn execute_tools_stream_parallel_with_registry(
    tool_calls: &[Value],
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    caller_model: Option<&str>,
    on_tool_result: Option<ToolResultCallback>,
    tool_metadata: &HashMap<String, ToolInfo>,
    builtin_services: &HashMap<String, BuiltinToolService>,
) -> Vec<ToolResult> {
    let tool_metadata = tool_metadata.clone();
    let builtin_services = builtin_services.clone();

    execute_tools_stream_parallel(
        tool_calls,
        session_id,
        conversation_turn_id,
        caller_model,
        on_tool_result,
        move |tool_name,
              args,
              session_id_owned,
              turn_id_owned,
              caller_model_owned,
              on_stream_chunk| {
            let tool_metadata = tool_metadata.clone();
            let builtin_services = builtin_services.clone();
            async move {
                call_tool_once(
                    &tool_metadata,
                    &builtin_services,
                    tool_name.as_str(),
                    args,
                    session_id_owned.as_deref(),
                    turn_id_owned.as_deref(),
                    caller_model_owned.as_deref(),
                    on_stream_chunk,
                )
                .await
            }
        },
    )
    .await
}

pub(crate) async fn execute_tools_stream_parallel<E, Fut>(
    tool_calls: &[Value],
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    caller_model: Option<&str>,
    on_tool_result: Option<ToolResultCallback>,
    execute_one: E,
) -> Vec<ToolResult>
where
    E: Fn(
            String,
            Value,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<ToolStreamChunkCallback>,
        ) -> Fut
        + Clone
        + Send
        + Sync
        + 'static,
    Fut: Future<Output = Result<(String, Option<Value>), String>> + Send + 'static,
{
    let normalized_turn_id = conversation_turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    let mut results: Vec<Option<ToolResult>> = vec![None; tool_calls.len()];
    let mut join_set: JoinSet<(usize, ToolResult)> = JoinSet::new();
    let mut pending_contexts: HashMap<Id, PendingToolContext> = HashMap::new();

    for (index, tool_call) in tool_calls.iter().enumerate() {
        if is_aborted(session_id) {
            break;
        }

        let tool_name = tool_call_name(tool_call).unwrap_or("").to_string();
        let call_id = tool_call_id(tool_call).unwrap_or("").to_string();
        if tool_name.trim().is_empty() {
            results[index] = Some(build_tool_result(
                call_id,
                "unknown".to_string(),
                false,
                true,
                false,
                normalized_turn_id.clone(),
                "工具名称不能为空".to_string(),
                None,
            ));
            continue;
        }

        let args_val = clone_tool_call_arguments(tool_call);
        let args = match parse_tool_args(args_val) {
            Ok(value) => value,
            Err(err) => {
                results[index] = Some(build_tool_result(
                    call_id,
                    tool_name,
                    false,
                    true,
                    false,
                    normalized_turn_id.clone(),
                    format!("参数解析失败: {}", err),
                    None,
                ));
                continue;
            }
        };

        let stream_turn_id_for_callback = normalized_turn_id.clone();
        let stream_turn_id_for_result = normalized_turn_id.clone();
        let on_stream_chunk = on_tool_result.as_ref().map(|callback| {
            let callback = Arc::clone(callback);
            let sid = session_id.map(|value| value.to_string());
            let stream_call_id = call_id.clone();
            let stream_tool_name = tool_name.clone();
            Arc::new(move |chunk: String| {
                if chunk.is_empty() {
                    return;
                }
                if !is_active(sid.as_deref()) {
                    return;
                }
                let event = build_tool_result(
                    stream_call_id.clone(),
                    stream_tool_name.clone(),
                    true,
                    false,
                    true,
                    stream_turn_id_for_callback.clone(),
                    chunk,
                    None,
                );
                callback(&event);
            }) as ToolStreamChunkCallback
        });

        let execute_one = execute_one.clone();
        let session_id_owned = session_id.map(|value| value.to_string());
        let turn_id_owned = conversation_turn_id.map(|value| value.to_string());
        let caller_model_owned = caller_model.map(|value| value.to_string());
        let pending_call_id = call_id.clone();
        let pending_tool_name = tool_name.clone();
        let pending_turn_id = stream_turn_id_for_result.clone();
        let task = join_set.spawn(async move {
            let outcome = execute_one(
                tool_name.clone(),
                args,
                session_id_owned,
                turn_id_owned,
                caller_model_owned,
                on_stream_chunk,
            )
            .await;

            let result = match outcome {
                Ok((content, structured_result)) => build_tool_result(
                    call_id,
                    tool_name,
                    true,
                    false,
                    false,
                    stream_turn_id_for_result.clone(),
                    content,
                    structured_result,
                ),
                Err(err) => build_tool_result(
                    call_id,
                    tool_name,
                    false,
                    true,
                    false,
                    stream_turn_id_for_result.clone(),
                    format!("工具执行失败: {}", err),
                    None,
                ),
            };

            (index, result)
        });
        pending_contexts.insert(
            task.id(),
            PendingToolContext {
                index,
                call_id: pending_call_id,
                tool_name: pending_tool_name,
                conversation_turn_id: pending_turn_id,
            },
        );
    }

    while let Some(joined) = join_set.join_next_with_id().await {
        match joined {
            Ok((task_id, (index, result))) => {
                pending_contexts.remove(&task_id);
                results[index] = Some(result);
            }
            Err(err) => {
                let task_id = err.id();
                if let Some(context) = pending_contexts.remove(&task_id) {
                    error!(
                        tool_name = %context.tool_name,
                        tool_call_id = %context.call_id,
                        "parallel tool task panicked or was cancelled: {}",
                        err
                    );
                    results[context.index] = Some(build_tool_result(
                        context.call_id,
                        context.tool_name,
                        false,
                        true,
                        false,
                        context.conversation_turn_id,
                        build_join_error_message(&err),
                        None,
                    ));
                } else {
                    warn!("parallel tool join error without context: {}", err);
                }
            }
        }

        if is_aborted(session_id) {
            join_set.abort_all();
            break;
        }
    }

    let mut ordered_results = Vec::new();
    for result in results.into_iter().flatten() {
        if !is_active(session_id) {
            break;
        }
        if let Some(callback) = on_tool_result.as_ref() {
            callback(&result);
        }
        ordered_results.push(result);
    }

    ordered_results
}

pub(crate) fn response_tool_name(tool: &Value) -> Option<&str> {
    tool.get("name")
        .and_then(|value| value.as_str())
        .or_else(|| {
            tool.get("function")
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str())
        })
}

pub(crate) fn is_aborted(session_id: Option<&str>) -> bool {
    session_id.map(abort_registry::is_aborted).unwrap_or(false)
}

fn build_tool_result(
    tool_call_id: String,
    name: String,
    success: bool,
    is_error: bool,
    is_stream: bool,
    conversation_turn_id: Option<String>,
    content: String,
    result: Option<Value>,
) -> ToolResult {
    ToolResult {
        tool_call_id,
        name,
        success,
        is_error,
        is_stream,
        conversation_turn_id,
        content,
        result,
    }
}

fn is_active(session_id: Option<&str>) -> bool {
    !is_aborted(session_id)
}

fn build_join_error_message(err: &tokio::task::JoinError) -> String {
    if err.is_cancelled() {
        "工具执行失败: parallel task cancelled before completion".to_string()
    } else if err.is_panic() {
        "工具执行失败: internal panic in parallel tool execution".to_string()
    } else {
        format!("工具执行失败: parallel tool task join error: {}", err)
    }
}

struct PendingToolContext {
    index: usize,
    call_id: String,
    tool_name: String,
    conversation_turn_id: Option<String>,
}
