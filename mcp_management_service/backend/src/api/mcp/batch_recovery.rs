// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(crate) async fn resolve_waiting_user_tool_invocation(
    state: &AppState,
    prompt_id: &str,
) -> Result<Option<RuntimeToolBatchRecord>, String> {
    let Some(batch) = state
        .runtime_tool_batches
        .find_by_waiting_user_prompt(prompt_id)
        .await?
    else {
        return Ok(None);
    };
    let Some(call_index) = batch
        .waiting_user_prompt_ids
        .iter()
        .position(|item| item.as_deref() == Some(prompt_id))
    else {
        return Ok(None);
    };
    let call = &batch.command.calls[call_index];
    let snapshot = state
        .runtime_sessions
        .get(batch.session_id.as_str())
        .await?
        .ok_or_else(|| "runtime session was not found or has expired".to_string())?;
    let tool = snapshot
        .tools
        .iter()
        .find(|tool| tool.exposed_name == call.name)
        .cloned()
        .ok_or_else(|| format!("tool not found in Runtime Session Snapshot: {}", call.name))?;
    let route = snapshot
        .routes
        .iter()
        .find(|route| route.resource_id == tool.resource_id)
        .cloned()
        .ok_or_else(|| "tool route snapshot is missing".to_string())?;
    let Some(result) = state
        .providers
        .resolve_waiting_user_call(&snapshot, &route, prompt_id, call.invocation_id.as_str())
        .await
        .map_err(|error| error.message)?
    else {
        return Ok(Some(batch));
    };
    state
        .runtime_invocations
        .complete(call.invocation_id.as_str(), result)
        .await?;
    resume_terminal_tool_batch_invocation(state, call.invocation_id.as_str()).await
}

pub(crate) async fn resume_terminal_tool_batch_invocation(
    state: &AppState,
    invocation_id: &str,
) -> Result<Option<RuntimeToolBatchRecord>, String> {
    let Some(batch) = state
        .runtime_tool_batches
        .find_by_invocation(invocation_id)
        .await?
    else {
        return Ok(None);
    };
    let Some(call_index) = batch
        .command
        .calls
        .iter()
        .position(|call| call.invocation_id == invocation_id)
    else {
        return Ok(None);
    };
    let call = &batch.command.calls[call_index];
    let record = state
        .runtime_invocations
        .get_for_caller(invocation_id, batch.command.owner_service.as_str())
        .await?
        .ok_or_else(|| "resolved Runtime Tool Batch invocation record is missing".to_string())?;
    if !is_terminal_invocation_status(record.status) {
        return Ok(Some(batch));
    }
    let snapshot = state
        .runtime_sessions
        .get(batch.session_id.as_str())
        .await?
        .ok_or_else(|| "runtime session was not found or has expired".to_string())?;
    let next_invocation_id = if let Some(run_id) = snapshot.run_id.as_deref() {
        state
            .runtime_execution_scopes
            .release_invocation_turn_and_next(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                invocation_id,
            )
            .await?
            .next_invocation_id
    } else {
        None
    };
    let batch = state
        .runtime_tool_batches
        .record_terminal_item(
            batch.batch_id.as_str(),
            call_index,
            result_item_from_record(call, record),
        )
        .await?;
    if let Some(next_invocation_id) = next_invocation_id {
        return state
            .runtime_tool_batches
            .ensure_invocation_ready_for(next_invocation_id.as_str())
            .await
            .map(Some);
    }
    Ok(Some(batch))
}

pub(crate) async fn persist_terminal_tool_batch_invocation_without_session(
    state: &AppState,
    invocation_id: &str,
) -> Result<Option<RuntimeToolBatchRecord>, String> {
    let Some(batch) = state
        .runtime_tool_batches
        .find_by_invocation(invocation_id)
        .await?
    else {
        return Ok(None);
    };
    let Some(call_index) = batch
        .command
        .calls
        .iter()
        .position(|call| call.invocation_id == invocation_id)
    else {
        return Ok(None);
    };
    let call = &batch.command.calls[call_index];
    let record = state
        .runtime_invocations
        .get_for_recovery(invocation_id, batch.command.owner_service.as_str())
        .await?
        .ok_or_else(|| "recovered Runtime Invocation record is missing".to_string())?;
    if !is_terminal_invocation_status(record.status) {
        return Ok(Some(batch));
    }
    let next_invocation_id = state
        .runtime_execution_scopes
        .release_invocation_turn_by_id_and_next(invocation_id)
        .await?
        .next_invocation_id;
    let batch = state
        .runtime_tool_batches
        .record_terminal_item(
            batch.batch_id.as_str(),
            call_index,
            result_item_from_record(call, record),
        )
        .await?;
    if let Some(next_invocation_id) = next_invocation_id {
        return state
            .runtime_tool_batches
            .ensure_invocation_ready_for(next_invocation_id.as_str())
            .await
            .map(Some);
    }
    Ok(Some(batch))
}

pub(super) fn is_terminal_invocation_status(status: RuntimeInvocationStatus) -> bool {
    matches!(
        status,
        RuntimeInvocationStatus::Completed
            | RuntimeInvocationStatus::Failed
            | RuntimeInvocationStatus::Cancelled
            | RuntimeInvocationStatus::UnknownExecutionState
    )
}
