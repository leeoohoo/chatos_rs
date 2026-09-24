// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(crate) struct RegisteredToolBatch {
    pub record: RuntimeToolBatchRecord,
}

pub(crate) async fn register_tool_call_command(
    state: &AppState,
    command: &McpToolCallCommand,
) -> Result<RegisteredToolBatch, String> {
    let snapshot = state
        .runtime_sessions
        .get(command.mcp_runtime_session_ref.trim())
        .await?
        .ok_or_else(|| "runtime session was not found or has expired".to_string())?;
    command.validate()?;
    if command.owner_service != snapshot.caller_service || command.agent_key != snapshot.agent_key {
        return Err(
            "MCP tool call command identity does not match its runtime session".to_string(),
        );
    }
    if command.calls.is_empty() || command.calls.len() > 128 {
        return Err("MCP tool call command must contain between 1 and 128 calls".to_string());
    }
    if command.batch_id.trim().is_empty() || command.batch_id.len() > 200 {
        return Err("MCP tool call command batch_id is invalid".to_string());
    }
    if let Some(existing) = state
        .runtime_tool_batches
        .get(command.batch_id.as_str())
        .await?
    {
        if serde_json::to_value(&existing.command).map_err(|error| error.to_string())?
            != serde_json::to_value(command).map_err(|error| error.to_string())?
        {
            return Err("Runtime Tool Batch id conflicts with a different command".to_string());
        }
        return Ok(RegisteredToolBatch { record: existing });
    }

    struct RegisteredCall {
        call_index: usize,
    }

    let mut results = vec![None; command.calls.len()];
    let mut registered = Vec::new();
    let mut seen_tool_call_ids = BTreeSet::new();
    let mut seen_invocation_ids = BTreeSet::new();
    for (call_index, call) in command.calls.iter().enumerate() {
        let item_error = if call.call_index != call_index {
            Some("call_index must match the calls array order".to_string())
        } else if call.tool_call_id.trim().is_empty()
            || !seen_tool_call_ids.insert(call.tool_call_id.clone())
        {
            Some("tool_call_id is empty or duplicated".to_string())
        } else if call.invocation_id.trim().is_empty()
            || !seen_invocation_ids.insert(call.invocation_id.clone())
        {
            Some("invocation_id is empty or duplicated".to_string())
        } else if let Some(error) = call.preflight_error.clone() {
            Some(error)
        } else if !call.arguments.is_object() {
            Some("tool arguments must be an object".to_string())
        } else {
            None
        };
        if let Some(error) = item_error {
            results[call_index] = Some(failed_command_item(call, MCP_ERROR_INVALID_PARAMS, error));
            continue;
        }
        if let Some(existing) = state
            .runtime_invocations
            .get_for_caller(
                call.invocation_id.as_str(),
                snapshot.caller_service.as_str(),
            )
            .await?
        {
            if existing.session_id != snapshot.session_id
                || existing.request_id_key
                    != serde_json::to_string(&Value::String(call.tool_call_id.clone()))
                        .map_err(|error| error.to_string())?
                || existing.exposed_tool_name != call.name
            {
                results[call_index] = Some(failed_command_item(
                    call,
                    MCP_ERROR_INVALID_PARAMS,
                    "invocation identity conflicts with an existing call".to_string(),
                ));
            } else if matches!(
                existing.status,
                RuntimeInvocationStatus::Completed
                    | RuntimeInvocationStatus::Failed
                    | RuntimeInvocationStatus::Cancelled
                    | RuntimeInvocationStatus::UnknownExecutionState
            ) {
                results[call_index] = Some(result_item_from_record(call, existing));
            } else {
                registered.push(RegisteredCall { call_index });
            }
            continue;
        }
        let Some(tool) = snapshot
            .tools
            .iter()
            .find(|tool| tool.exposed_name == call.name)
            .cloned()
        else {
            results[call_index] = Some(failed_command_item(
                call,
                MCP_ERROR_INVALID_PARAMS,
                format!("tool not found: {}", call.name),
            ));
            continue;
        };
        let Some(route) = snapshot
            .routes
            .iter()
            .find(|route| route.resource_id == tool.resource_id)
            .cloned()
        else {
            results[call_index] = Some(failed_command_item(
                call,
                MCP_ERROR_INTERNAL,
                "tool route snapshot is missing".to_string(),
            ));
            continue;
        };
        if !route_allows_system_tool(&route, tool.original_name.as_str()) {
            results[call_index] = Some(failed_command_item(
                call,
                MCP_ERROR_AUTH_REQUIRED,
                "tool is blocked by the immutable read-only route policy".to_string(),
            ));
            continue;
        }
        if let Err(error) = validate_product_skill_binding(&route, &tool) {
            results[call_index] = Some(failed_command_item(call, MCP_ERROR_AUTH_REQUIRED, error));
            continue;
        }
        if route.provider_kind == McpProviderKind::Unavailable {
            results[call_index] = Some(failed_command_item(
                call,
                MCP_ERROR_INTERNAL,
                format!("provider unavailable: {}", route.reason),
            ));
            continue;
        }
        let mutation_may_have_started = route.allow_writes
            && tool
                .definition
                .pointer("/annotations/readOnlyHint")
                .and_then(Value::as_bool)
                != Some(true);
        let invocation = RuntimeInvocationRecord {
            invocation_id: call.invocation_id.clone(),
            session_id: snapshot.session_id.clone(),
            request_id_key: serde_json::to_string(&Value::String(call.tool_call_id.clone()))
                .map_err(|error| error.to_string())?,
            caller_service: snapshot.caller_service.clone(),
            tenant_id: snapshot.tenant_id.clone(),
            owner_user_id: snapshot.owner_user_id.clone(),
            project_id: snapshot.project_id.clone(),
            device_id: snapshot.device_id.clone(),
            resource_id: route.resource_id.clone(),
            exposed_tool_name: tool.exposed_name.clone(),
            original_tool_name: tool.original_name.clone(),
            mutation_may_have_started,
            cancel_supported: route.cancel_supported,
            status: RuntimeInvocationStatus::Queued,
            created_at_unix_ms: chrono::Utc::now().timestamp_millis(),
            started_at_unix_ms: None,
            completed_at_unix_ms: None,
            terminal_result: None,
            terminal_error_code: None,
            terminal_error_message: None,
            file_modification_outcome: None,
            expires_at: required_timestamp(snapshot.expires_at_unix, "session expiry")?,
            expires_at_unix: snapshot.expires_at_unix,
        };
        match register_runtime_invocation(state, &snapshot, invocation, false).await {
            Ok(()) => registered.push(RegisteredCall { call_index }),
            Err(error) => {
                results[call_index] = Some(failed_command_item(
                    call,
                    MCP_ERROR_INTERNAL,
                    format!("register MCP invocation failed: {error}"),
                ));
            }
        }
    }

    if let Some(run_id) = snapshot.run_id.as_deref() {
        let invocations = registered
            .iter()
            .map(|registered| {
                (
                    command.calls[registered.call_index].invocation_id.clone(),
                    registered.call_index,
                )
            })
            .collect::<Vec<_>>();
        if let Err(error) = state
            .runtime_execution_scopes
            .enqueue_invocation_batch(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                command.batch_id.as_str(),
                invocations.as_slice(),
            )
            .await
        {
            for registered in &registered {
                let call = &command.calls[registered.call_index];
                let _ = state
                    .runtime_invocations
                    .discard_queued_registration(
                        call.invocation_id.as_str(),
                        snapshot.session_id.as_str(),
                    )
                    .await;
                results[registered.call_index] = Some(failed_command_item(
                    call,
                    MCP_ERROR_INTERNAL,
                    format!("enqueue MCP invocation failed: {error}"),
                ));
            }
            registered.clear();
        }
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let record = RuntimeToolBatchRecord {
        batch_id: command.batch_id.clone(),
        command: command.clone(),
        session_id: snapshot.session_id.clone(),
        status: RuntimeToolBatchStatus::Active,
        next_call_index: 0,
        items: results,
        invocation_ids: command
            .calls
            .iter()
            .map(|call| call.invocation_id.clone())
            .collect(),
        waiting_user_prompt_ids: vec![None; command.calls.len()],
        pending_event: None,
        revision: 0,
        created_at_unix_ms: now_ms,
        updated_at_unix_ms: now_ms,
        expires_at: required_timestamp(snapshot.expires_at_unix, "tool batch expiry")?,
        expires_at_unix: snapshot.expires_at_unix,
    };
    let record = state.runtime_tool_batches.insert_or_get(record).await?;
    Ok(RegisteredToolBatch { record })
}
