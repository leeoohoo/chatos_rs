// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LiveBatchWatchdogAction {
    None,
    EnsureInvocationReady,
    ResumeTerminal,
}

pub(super) fn live_batch_watchdog_action(
    status: RuntimeInvocationStatus,
) -> LiveBatchWatchdogAction {
    match status {
        RuntimeInvocationStatus::Queued => LiveBatchWatchdogAction::EnsureInvocationReady,
        RuntimeInvocationStatus::Completed
        | RuntimeInvocationStatus::Failed
        | RuntimeInvocationStatus::Cancelled
        | RuntimeInvocationStatus::UnknownExecutionState => LiveBatchWatchdogAction::ResumeTerminal,
        RuntimeInvocationStatus::Running
        | RuntimeInvocationStatus::WaitingForUser
        | RuntimeInvocationStatus::CancelRequested => LiveBatchWatchdogAction::None,
    }
}

pub(super) async fn reconcile_live_batches(
    state: &AppState,
    topology: &AsyncToolDispatchTopology,
    channel: &Channel,
) -> Result<(), String> {
    for batch in state.runtime_tool_batches.list_active(1_000).await? {
        let outcome: Result<(), String> = async {
            let Some(call) = batch.command.calls.get(batch.next_call_index) else {
                return Ok(());
            };
            let Some(invocation) = state
                .runtime_invocations
                .get_for_recovery(
                    call.invocation_id.as_str(),
                    batch.command.owner_service.as_str(),
                )
                .await?
            else {
                state
                    .runtime_tool_batches
                    .ensure_invocation_ready_for(call.invocation_id.as_str())
                    .await?;
                return Ok(());
            };
            match live_batch_watchdog_action(invocation.status) {
                LiveBatchWatchdogAction::None => {}
                LiveBatchWatchdogAction::EnsureInvocationReady => {
                    state
                        .runtime_tool_batches
                        .ensure_invocation_ready_for(invocation.invocation_id.as_str())
                        .await?;
                }
                LiveBatchWatchdogAction::ResumeTerminal => {
                    resume_terminal_invocation_with_session_fallback(
                        state,
                        invocation.invocation_id.as_str(),
                    )
                    .await?;
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = outcome {
            warn!(
                batch_id = batch.batch_id.as_str(),
                error = error.as_str(),
                "MCP batch watchdog skipped one invalid batch"
            );
        }
    }
    reconcile_pending_batches(state, topology, channel).await
}

pub(super) async fn resume_terminal_invocation_with_session_fallback(
    state: &AppState,
    invocation_id: &str,
) -> Result<Option<crate::runtime::RuntimeToolBatchRecord>, String> {
    match crate::api::mcp::resume_terminal_tool_batch_invocation(state, invocation_id).await {
        Ok(batch) => Ok(batch),
        Err(error) if error == "runtime session was not found or has expired" => {
            crate::api::mcp::persist_terminal_tool_batch_invocation_without_session(
                state,
                invocation_id,
            )
            .await
        }
        Err(error) => Err(error),
    }
}

pub(super) async fn reconcile_expired_invocations(state: &AppState) -> Result<(), String> {
    const RECOVERY_CLAIM_LEASE: std::time::Duration = std::time::Duration::from_secs(30);

    let claims = state
        .runtime_invocations
        .claim_expired_active(
            state.config.runtime_retention_batch_size,
            RECOVERY_CLAIM_LEASE,
        )
        .await?;
    state
        .async_tool_dispatch
        .observe_expired_recovery_claims(claims.len());
    for claim in claims {
        let outcome: Result<bool, String> = async {
            if claim.cancellation_required {
                state
                    .runtime_invocations
                    .signal_cancellation(claim.record.invocation_id.as_str())?;
                state
                    .async_tool_dispatch
                    .publish_cancellation(claim.record.invocation_id.as_str())
                    .await
                    .map_err(|error| error.to_string())?;
            }
            let transitioned = state
                .runtime_invocations
                .finish_expired_claim(&claim)
                .await?;
            if !transitioned {
                let Some(current) = state
                    .runtime_invocations
                    .get_for_recovery(
                        claim.record.invocation_id.as_str(),
                        claim.record.caller_service.as_str(),
                    )
                    .await?
                else {
                    return Ok(false);
                };
                if !is_recoverable_terminal_invocation_status(current.status) {
                    return Ok(false);
                }
            }
            let batch = resume_terminal_invocation_with_session_fallback(
                state,
                claim.record.invocation_id.as_str(),
            )
            .await?;
            if batch.is_none() {
                state
                    .runtime_execution_scopes
                    .release_invocation_turn_by_id_and_next(claim.record.invocation_id.as_str())
                    .await?;
            }
            if let Some(batch) = batch {
                if batch.pending_event.is_some() {
                    tracing::info!(
                        invocation_id = claim.record.invocation_id.as_str(),
                        batch_id = batch.batch_id.as_str(),
                        "recovered expired MCP invocation"
                    );
                }
            }
            Ok(true)
        }
        .await;
        match outcome {
            Ok(true) => state
                .async_tool_dispatch
                .observe_expired_recovery_completed(),
            Ok(false) => {}
            Err(error) => {
                state.async_tool_dispatch.observe_expired_recovery_failed();
                warn!(
                    invocation_id = claim.record.invocation_id.as_str(),
                    error = error.as_str(),
                    "recover one expired MCP invocation failed"
                );
            }
        }
    }
    Ok(())
}
