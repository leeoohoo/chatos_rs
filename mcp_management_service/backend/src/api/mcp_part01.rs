pub(crate) async fn execute_async_tool_call(
    state: AppState,
    snapshot: Arc<RuntimeSessionSnapshot>,
    route: ResolvedMcpRoute,
    tool: RuntimeToolDescriptor,
    arguments: Value,
    invocation_id: String,
    mutation_may_have_started: bool,
) -> Result<(), String> {
    let Some(record) = state
        .runtime_invocations
        .get_for_caller(invocation_id.as_str(), snapshot.caller_service.as_str())
        .await?
    else {
        return Ok(());
    };
    match record.status {
        RuntimeInvocationStatus::Completed
        | RuntimeInvocationStatus::Failed
        | RuntimeInvocationStatus::Cancelled
        | RuntimeInvocationStatus::UnknownExecutionState => {
            if let Some(run_id) = snapshot.run_id.as_deref() {
                state
                    .runtime_execution_scopes
                    .release_invocation_turn(
                        snapshot.owner_user_id.as_str(),
                        snapshot.project_id.as_deref(),
                        run_id,
                        snapshot.execution_scope_provider(),
                        invocation_id.as_str(),
                    )
                    .await?;
            }
            return Ok(());
        }
        RuntimeInvocationStatus::Running | RuntimeInvocationStatus::WaitingForUser => {
            return Ok(());
        }
        RuntimeInvocationStatus::CancelRequested if record.started_at_unix_ms.is_some() => {
            return Ok(());
        }
        RuntimeInvocationStatus::CancelRequested => {
            state
                .runtime_invocations
                .cancel_without_start(invocation_id.as_str())
                .await?;
            if let Some(run_id) = snapshot.run_id.as_deref() {
                state
                    .runtime_execution_scopes
                    .release_invocation_turn(
                        snapshot.owner_user_id.as_str(),
                        snapshot.project_id.as_deref(),
                        run_id,
                        snapshot.execution_scope_provider(),
                        invocation_id.as_str(),
                    )
                    .await?;
            }
            return Ok(());
        }
        RuntimeInvocationStatus::Queued => {}
    }
    if snapshot.run_id.is_none() {
        match state
            .runtime_invocations
            .mark_running(invocation_id.as_str())
            .await
        {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            Err(error) => {
                tracing::error!(
                    invocation_id = invocation_id.as_str(),
                    error = error.as_str(),
                    "mark queued Runtime Invocation as running failed"
                );
                return Err(format!(
                    "mark queued Runtime Invocation as running failed: {error}"
                ));
            }
        }
    }
    let (dispatch, duration_ms) = dispatch_provider_call(
        &state,
        &snapshot,
        &route,
        &tool,
        arguments,
        invocation_id.as_str(),
    )
    .await;
    match dispatch {
        DispatchResult::AlreadyRunning => {}
        DispatchResult::CancelledBeforeStart => {
            record_tool_access_audit(
                &snapshot,
                &route,
                tool.exposed_name.as_str(),
                "cancelled_before_start",
            );
        }
        DispatchResult::CancelRequested => {
            let _ = handle_cancelled_tool_call(
                Value::Null,
                &snapshot,
                &route,
                tool.exposed_name.as_str(),
                invocation_id.as_str(),
                mutation_may_have_started,
                duration_ms,
                &state,
            )
            .await;
        }
        DispatchResult::RegistryFailed(error) => {
            record_tool_access_audit(
                &snapshot,
                &route,
                tool.exposed_name.as_str(),
                "registry_failed",
            );
            tracing::error!(
                invocation_id = invocation_id.as_str(),
                session_id = snapshot.session_id.as_str(),
                error = error.as_str(),
                status = "registry_failed",
                "async MCP Provider invocation coordination failed"
            );
        }
        DispatchResult::Completed(Ok(outcome)) => {
            record_tool_access_audit(&snapshot, &route, tool.exposed_name.as_str(), "succeeded");
            tracing::info!(
                invocation_id = invocation_id.as_str(),
                session_id = snapshot.session_id.as_str(),
                resource_id = route.resource_id.as_str(),
                exposed_tool_name = tool.exposed_name.as_str(),
                provider_kind = route.provider_kind.as_str(),
                duration_ms,
                result_bytes = outcome.response_bytes,
                status = "succeeded",
                mode = "async",
                "async MCP Provider invocation completed"
            );
        }
        DispatchResult::Completed(Err(error)) => {
            record_tool_access_audit(&snapshot, &route, tool.exposed_name.as_str(), "failed");
            tracing::warn!(
                invocation_id = invocation_id.as_str(),
                session_id = snapshot.session_id.as_str(),
                resource_id = route.resource_id.as_str(),
                exposed_tool_name = tool.exposed_name.as_str(),
                provider_kind = route.provider_kind.as_str(),
                duration_ms,
                error_code = error.code,
                status = "failed",
                mode = "async",
                "async MCP Provider invocation failed"
            );
        }
    }
    Ok(())
}

pub(super) fn cancel_response_status(record: &RuntimeInvocationRecord) -> &'static str {
    cancellation::cancel_response_status(record)
}

fn grant_matches_snapshot(
    claims: &crate::runtime::RuntimeGrantClaims,
    snapshot: &RuntimeSessionSnapshot,
) -> bool {
    let claim_resource_ids = claims
        .allowed_resource_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let snapshot_resource_ids = snapshot
        .routes
        .iter()
        .map(|route| route.resource_id.as_str())
        .collect::<BTreeSet<_>>();
    claims.session_id == snapshot.session_id
        && claims.sub == snapshot.caller_service
        && claims.trace_id == snapshot.trace_id
        && claims.tenant_id == snapshot.tenant_id
        && claims.owner_user_id == snapshot.owner_user_id
        && claims.agent_key == snapshot.agent_key
        && claims.task_profile == snapshot.task_profile
        && claims.project_id == snapshot.project_id
        && claims.device_id == snapshot.device_id
        && claims.run_id == snapshot.run_id
        && claims.turn_id == snapshot.turn_id
        && claims.task_id == snapshot.task_id
        && claims.source_session_id == snapshot.source_session_id
        && claims.source_user_message_id == snapshot.source_user_message_id
        && claims.contact_agent_id == snapshot.contact_agent_id
        && claims.default_model_config_id == snapshot.default_model_config_id
        && claims.policy_revision == snapshot.policy_revision
        && claims.route_revision == snapshot.route_revision
        && i64::try_from(claims.exp).ok() == Some(snapshot.expires_at_unix)
        && claim_resource_ids == snapshot_resource_ids
}

#[cfg(test)]
mod tests;
