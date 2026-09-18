// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Instant;

use chatos_mcp::{system_mcp_descriptor, SystemMcpKey};

use super::*;

pub(super) async fn dispatch_provider_call(
    state: &AppState,
    snapshot: &RuntimeSessionSnapshot,
    route: &ResolvedMcpRoute,
    tool: &RuntimeToolDescriptor,
    arguments: Value,
    invocation_id: &str,
) -> (DispatchResult, u64) {
    let started = Instant::now();
    let mut acquired_turn = false;
    let mut cancelled_before_start = false;
    let mut deferred_turn = false;
    let mut coordination_error = None;
    if let Some(run_id) = snapshot.run_id.as_deref() {
        match state
            .runtime_execution_scopes
            .try_acquire_invocation_turn(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                invocation_id,
            )
            .await
        {
            Ok(RuntimeExecutionTurnState::Acquired) => acquired_turn = true,
            Ok(RuntimeExecutionTurnState::Terminal) => cancelled_before_start = true,
            Ok(RuntimeExecutionTurnState::Waiting) => {
                match state
                    .runtime_invocations
                    .cancellation_requested(invocation_id)
                    .await
                {
                    Ok(true) => cancelled_before_start = true,
                    Ok(false) => {
                        deferred_turn = true;
                        coordination_error = Some(
                            "MCP invocation is waiting for its persisted run FIFO turn; defer the command delivery"
                                .to_string(),
                        )
                    }
                    Err(error) => coordination_error = Some(error),
                }
            }
            Err(error) => coordination_error = Some(error),
        }
    }
    if let Some(run_id) = snapshot.run_id.as_deref() {
        if (cancelled_before_start || coordination_error.is_some()) && !deferred_turn {
            if let Err(error) = state
                .runtime_execution_scopes
                .release_invocation_turn(
                    snapshot.owner_user_id.as_str(),
                    snapshot.project_id.as_deref(),
                    run_id,
                    snapshot.execution_scope_provider(),
                    invocation_id,
                )
                .await
            {
                coordination_error.get_or_insert(error);
            }
        }
    }
    if let Some(error) = coordination_error {
        return (
            DispatchResult::RegistryFailed(error),
            started.elapsed().as_millis() as u64,
        );
    }
    if cancelled_before_start {
        if let Err(error) = state
            .runtime_invocations
            .cancel_without_start(invocation_id)
            .await
        {
            return (
                DispatchResult::RegistryFailed(error),
                started.elapsed().as_millis() as u64,
            );
        }
        return (
            DispatchResult::CancelledBeforeStart,
            started.elapsed().as_millis() as u64,
        );
    }
    if snapshot.run_id.is_some() {
        match state.runtime_invocations.mark_running(invocation_id).await {
            Ok(true) => {}
            Ok(false) => {
                return (
                    DispatchResult::AlreadyRunning,
                    started.elapsed().as_millis() as u64,
                );
            }
            Err(error) => {
                if let Some(run_id) = snapshot.run_id.as_deref() {
                    let _ = state
                        .runtime_execution_scopes
                        .release_invocation_turn(
                            snapshot.owner_user_id.as_str(),
                            snapshot.project_id.as_deref(),
                            run_id,
                            snapshot.execution_scope_provider(),
                            invocation_id,
                        )
                        .await;
                }
                return (
                    DispatchResult::RegistryFailed(error),
                    started.elapsed().as_millis() as u64,
                );
            }
        }
    }
    if route_waits_for_user(route) {
        match state
            .runtime_invocations
            .mark_waiting_for_user(invocation_id)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                if acquired_turn {
                    let run_id = snapshot
                        .run_id
                        .as_deref()
                        .expect("acquired run invocation turn requires run_id");
                    if let Err(error) = state
                        .runtime_execution_scopes
                        .release_invocation_turn(
                            snapshot.owner_user_id.as_str(),
                            snapshot.project_id.as_deref(),
                            run_id,
                            snapshot.execution_scope_provider(),
                            invocation_id,
                        )
                        .await
                    {
                        return (
                            DispatchResult::RegistryFailed(error),
                            started.elapsed().as_millis() as u64,
                        );
                    }
                }
                return (
                    DispatchResult::CancelRequested,
                    started.elapsed().as_millis() as u64,
                );
            }
            Err(error) => {
                if acquired_turn {
                    let run_id = snapshot
                        .run_id
                        .as_deref()
                        .expect("acquired run invocation turn requires run_id");
                    if let Err(release_error) = state
                        .runtime_execution_scopes
                        .release_invocation_turn(
                            snapshot.owner_user_id.as_str(),
                            snapshot.project_id.as_deref(),
                            run_id,
                            snapshot.execution_scope_provider(),
                            invocation_id,
                        )
                        .await
                    {
                        return (
                            DispatchResult::RegistryFailed(format!(
                                "{error}; release invocation turn failed: {release_error}"
                            )),
                            started.elapsed().as_millis() as u64,
                        );
                    }
                }
                return (
                    DispatchResult::RegistryFailed(error),
                    started.elapsed().as_millis() as u64,
                );
            }
        }
    }
    let dispatch = {
        let outcome = state.providers.call_tool(
            snapshot,
            route,
            tool.original_name.as_str(),
            arguments,
            invocation_id,
        );
        tokio::pin!(outcome);
        tokio::select! {
            outcome = &mut outcome => {
                match outcome {
                    Ok(success) => match state.runtime_invocations.complete(invocation_id, success.result.clone()).await {
                        Ok(true) => DispatchResult::Completed(Ok(success)),
                        Ok(false) => DispatchResult::CancelRequested,
                        Err(error) => DispatchResult::RegistryFailed(error),
                    },
                    Err(error) => match state.runtime_invocations.fail(invocation_id, error.code, error.message.clone()).await {
                        Ok(true) => DispatchResult::Completed(Err(error)),
                        Ok(false) => DispatchResult::CancelRequested,
                        Err(registry_error) => DispatchResult::RegistryFailed(registry_error),
                    },
                }
            }
            cancellation = wait_for_cancellation(state, invocation_id) => {
                match cancellation {
                    Ok(()) => DispatchResult::CancelRequested,
                    Err(error) => DispatchResult::RegistryFailed(error),
                }
            }
        }
    };
    if acquired_turn {
        let run_id = snapshot
            .run_id
            .as_deref()
            .expect("acquired run invocation turn requires run_id");
        if let Err(error) = state
            .runtime_execution_scopes
            .release_invocation_turn(
                snapshot.owner_user_id.as_str(),
                snapshot.project_id.as_deref(),
                run_id,
                snapshot.execution_scope_provider(),
                invocation_id,
            )
            .await
        {
            return (
                DispatchResult::RegistryFailed(error),
                started.elapsed().as_millis() as u64,
            );
        }
    }
    (dispatch, started.elapsed().as_millis() as u64)
}

pub(super) fn route_waits_for_user(route: &ResolvedMcpRoute) -> bool {
    route.resource_id == system_mcp_descriptor(SystemMcpKey::AskUser).resource_id
}
