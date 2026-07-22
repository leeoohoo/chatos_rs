// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use chatos_mcp_runtime::parallelism::should_parallelize_tool_batch;
use chatos_mcp_runtime::{execute_tool_calls_parallel, ToolCallContext, ToolCallerModelRuntime};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::core::mcp_tools::{
    execute_tools_stream as execute_tools_stream_common, inject_agent_builder_args,
    jsonrpc_http_call, jsonrpc_stdio_call, to_text_and_structured_result, BuiltinToolService,
    ToolInfo, ToolResult, ToolResultCallback, ToolStreamChunkCallback,
};
use crate::core::tool_call::extract_tool_call_name;
use crate::utils::abort_registry;

const HEAVY_IO_TOOL_SESSION_LIMIT: usize = 2;
const HEAVY_IO_TOOL_GLOBAL_LIMIT: usize = 8;
const HEAVY_IO_TOOL_NAMES: &[&str] = &[
    "apply_patch",
    "delete_file",
    "delete_path",
    "edit_file",
    "list_dir",
    "list_directory",
    "read_file",
    "read_file_range",
    "read_file_raw",
    "search_files",
    "search_text",
    "write_file",
];

static HEAVY_IO_TOOL_GLOBAL_LIMITER: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(HEAVY_IO_TOOL_GLOBAL_LIMIT)));
static HEAVY_IO_TOOL_SESSION_LIMITERS: Lazy<Mutex<HashMap<String, Weak<Semaphore>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

struct HeavyIoToolPermits {
    _session: OwnedSemaphorePermit,
    _global: OwnedSemaphorePermit,
}

pub(crate) async fn execute_tools_stream_with_registry(
    tool_calls: &[Value],
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
    caller_model: Option<&str>,
    caller_model_runtime: Option<&ToolCallerModelRuntime>,
    on_tool_result: Option<ToolResultCallback>,
    tool_metadata: &HashMap<String, ToolInfo>,
    tool_aliases: &HashMap<String, String>,
    builtin_services: &HashMap<String, BuiltinToolService>,
) -> Vec<ToolResult> {
    let normalized_tool_calls = normalize_tool_calls(tool_calls, tool_metadata, tool_aliases);
    let execution_context = ToolCallContext::new(
        session_id.map(ToOwned::to_owned),
        normalized_context_value(conversation_turn_id),
        caller_model.map(ToOwned::to_owned),
    )
    .with_caller_model_runtime(caller_model_runtime.cloned())
    .with_abort_checker(Arc::new(abort_registry::is_aborted));

    if should_parallelize_tool_batch(normalized_tool_calls.as_slice(), tool_metadata) {
        return execute_tools_stream_parallel_with_registry(
            normalized_tool_calls.as_slice(),
            execution_context,
            on_tool_result,
            tool_metadata,
            tool_aliases,
            builtin_services,
        )
        .await;
    }

    execute_tools_stream_common(
        normalized_tool_calls.as_slice(),
        execution_context,
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
                caller_model_runtime,
                tool_aliases,
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
    caller_model_runtime: Option<&ToolCallerModelRuntime>,
    tool_aliases: &HashMap<String, String>,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
) -> Result<(String, Option<Value>), String> {
    let resolved_tool_name =
        resolve_tool_name(tool_name, tool_metadata, tool_aliases).unwrap_or(tool_name);
    let info = tool_metadata
        .get(resolved_tool_name)
        .ok_or_else(|| format!("工具未找到: {}", tool_name))?;

    let _io_permits = acquire_heavy_io_tool_permits(info, session_id).await?;

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
        let tool_call_context = ToolCallContext::new(
            session_id.map(ToOwned::to_owned),
            normalized_context_value(conversation_turn_id),
            caller_model.map(ToOwned::to_owned),
        )
        .with_caller_model_runtime(caller_model_runtime.cloned())
        .with_abort_checker(Arc::new(abort_registry::is_aborted));

        let result = service.call_tool(
            &info.original_name,
            args,
            &tool_call_context,
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

async fn acquire_heavy_io_tool_permits(
    info: &ToolInfo,
    session_id: Option<&str>,
) -> Result<Option<HeavyIoToolPermits>, String> {
    if !is_heavy_io_tool_name(info.original_name.as_str()) {
        return Ok(None);
    }

    let session_limiter = heavy_io_session_limiter(session_id);
    let session_permit = session_limiter
        .acquire_owned()
        .await
        .map_err(|_| "heavy IO tool session limiter closed".to_string())?;
    let global_permit = Arc::clone(&HEAVY_IO_TOOL_GLOBAL_LIMITER)
        .acquire_owned()
        .await
        .map_err(|_| "heavy IO tool global limiter closed".to_string())?;

    Ok(Some(HeavyIoToolPermits {
        _session: session_permit,
        _global: global_permit,
    }))
}

pub(crate) fn is_heavy_io_tool_name(tool_name: &str) -> bool {
    HEAVY_IO_TOOL_NAMES.contains(&tool_name)
}

fn heavy_io_session_limiter(session_id: Option<&str>) -> Arc<Semaphore> {
    let key = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("anonymous")
        .to_string();
    let mut limiters = HEAVY_IO_TOOL_SESSION_LIMITERS
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    if let Some(existing) = limiters.get(&key).and_then(Weak::upgrade) {
        return existing;
    }

    limiters.retain(|_, limiter| limiter.strong_count() > 0);
    let limiter = Arc::new(Semaphore::new(HEAVY_IO_TOOL_SESSION_LIMIT));
    limiters.insert(key, Arc::downgrade(&limiter));
    limiter
}

fn http_tool_call_headers(
    info: &ToolInfo,
    session_id: Option<&str>,
    conversation_turn_id: Option<&str>,
) -> Option<HashMap<String, String>> {
    let mut headers = info.server_headers.clone().unwrap_or_default();
    if info.server_name
        == chatos_mcp::system_mcp_descriptor(
            chatos_plugin_management_sdk::SystemMcpKey::TaskRunnerService,
        )
        .server_name
    {
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
    context: ToolCallContext,
    on_tool_result: Option<ToolResultCallback>,
    tool_metadata: &HashMap<String, ToolInfo>,
    tool_aliases: &HashMap<String, String>,
    builtin_services: &HashMap<String, BuiltinToolService>,
) -> Vec<ToolResult> {
    let tool_metadata = tool_metadata.clone();
    let tool_aliases = tool_aliases.clone();
    let builtin_services = builtin_services.clone();

    execute_tool_calls_parallel(
        tool_calls,
        context,
        on_tool_result,
        move |tool_name, args, context, on_stream_chunk| {
            let tool_metadata = tool_metadata.clone();
            let tool_aliases = tool_aliases.clone();
            let builtin_services = builtin_services.clone();
            async move {
                call_tool_once(
                    &tool_metadata,
                    &builtin_services,
                    tool_name.as_str(),
                    args,
                    context.conversation_id.as_deref(),
                    context.conversation_turn_id.as_deref(),
                    context.caller_model.as_deref(),
                    context.caller_model_runtime.as_ref(),
                    &tool_aliases,
                    on_stream_chunk,
                )
                .await
            }
        },
    )
    .await
}

fn resolve_tool_name<'a>(
    tool_name: &'a str,
    tool_metadata: &'a HashMap<String, ToolInfo>,
    tool_aliases: &'a HashMap<String, String>,
) -> Option<&'a str> {
    if tool_metadata.contains_key(tool_name) {
        Some(tool_name)
    } else {
        tool_aliases.get(tool_name).map(String::as_str)
    }
}

fn normalize_tool_calls(
    tool_calls: &[Value],
    tool_metadata: &HashMap<String, ToolInfo>,
    tool_aliases: &HashMap<String, String>,
) -> Vec<Value> {
    tool_calls
        .iter()
        .map(|tool_call| normalize_tool_call(tool_call, tool_metadata, tool_aliases))
        .collect()
}

fn normalize_tool_call(
    tool_call: &Value,
    tool_metadata: &HashMap<String, ToolInfo>,
    tool_aliases: &HashMap<String, String>,
) -> Value {
    let Some(requested_name) = extract_tool_call_name(tool_call) else {
        return tool_call.clone();
    };
    let Some(resolved_name) = resolve_tool_name(requested_name, tool_metadata, tool_aliases) else {
        return tool_call.clone();
    };
    if resolved_name == requested_name {
        return tool_call.clone();
    }

    let mut normalized = tool_call.clone();
    if let Some(function) = normalized
        .get_mut("function")
        .and_then(Value::as_object_mut)
    {
        function.insert("name".to_string(), Value::String(resolved_name.to_string()));
    } else if let Some(object) = normalized.as_object_mut() {
        object.insert("name".to_string(), Value::String(resolved_name.to_string()));
    }
    normalized
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
