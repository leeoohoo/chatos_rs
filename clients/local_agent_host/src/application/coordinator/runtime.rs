impl LocalAgentHostCoordinator {
    async fn run_memory_until_shutdown(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), LocalAgentCoordinatorError> {
        let Some(worker) = self.memory_sync_worker.as_ref() else {
            return Ok(());
        };
        let mut accepting_new_work = self.accepting_new_work.subscribe();
        let mut failure_backoff = Duration::from_millis(100);
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            while !*accepting_new_work.borrow() {
                tokio::select! {
                    changed = accepting_new_work.changed() => {
                        if changed.is_err() {
                            return Ok(());
                        }
                    }
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            return Ok(());
                        }
                    }
                }
            }
            let observed_revision = worker.outbox_revision();
            let tick = {
                let Some(_activity) = self.begin_admitted_work(&self.memory_work_activity).await
                else {
                    continue;
                };
                worker.run_once().await
            };
            match tick {
                Ok(MemorySyncTick::Synced { .. }) => {
                    self.signal_activity();
                    failure_backoff = Duration::from_millis(100);
                    continue;
                }
                Ok(MemorySyncTick::RetryScheduled { .. }) => {
                    self.signal_activity();
                    failure_backoff = Duration::from_millis(100);
                }
                Ok(MemorySyncTick::Idle) => {
                    failure_backoff = Duration::from_millis(100);
                }
                Err(_) => {
                    tokio::select! {
                        _ = tokio::time::sleep(failure_backoff) => {}
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() {
                                return Ok(());
                            }
                        }
                    }
                    failure_backoff = (failure_backoff * 2).min(Duration::from_secs(5));
                    continue;
                }
            }
            if worker.outbox_revision() != observed_revision {
                continue;
            }
            match worker.next_retry_at().await? {
                Some(next_retry_at) => {
                    let now = system_now_unix_ms()?;
                    let delay = Duration::from_millis(
                        u64::try_from(next_retry_at.saturating_sub(now).max(0)).unwrap_or(0),
                    );
                    tokio::select! {
                        changed = worker.wait_for_outbox_revision(observed_revision) => {
                            changed?;
                        }
                        _ = tokio::time::sleep(delay) => {}
                        changed = accepting_new_work.changed() => {
                            if changed.is_err() {
                                return Ok(());
                            }
                        }
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() {
                                return Ok(());
                            }
                        }
                    }
                }
                None => {
                    tokio::select! {
                        changed = worker.wait_for_outbox_revision(observed_revision) => {
                            changed?;
                        }
                        changed = accepting_new_work.changed() => {
                            if changed.is_err() {
                                return Ok(());
                            }
                        }
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }

    fn signal_activity(&self) {
        self.activity
            .send_modify(|value| *value = value.wrapping_add(1));
    }

    async fn wait_for_events(
        &self,
        request: HostRequestEnvelope,
        timeout_ms: u64,
    ) -> HostResponseEnvelope {
        let mut activity = self.activity.subscribe();
        let mut response = self.runtime.handle(request.clone()).await;
        if !response.ok || response_has_events(&response) {
            return response;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    // A recovery transaction can persist events without an in-process activity
                    // signal. Always perform one final durable read at the timeout boundary.
                    return self.runtime.handle(request.clone()).await;
                },
                changed = activity.changed() => {
                    if changed.is_err() {
                        return response;
                    }
                    response = self.runtime.handle(request.clone()).await;
                    if !response.ok || response_has_events(&response) {
                        return response;
                    }
                }
            }
        }
    }

    fn route_external_tool_claim(&self, request: &mut HostRequestEnvelope) {
        let HostCommand::ClaimNextTool(command) = &mut request.command else {
            return;
        };
        for tool_name in &self.reserved_ipc_tools {
            if !command.exclude_tool_names.contains(tool_name) {
                command.exclude_tool_names.push(tool_name.clone());
            }
        }
        command.execution_route = Some(LocalAgentToolExecutionRoute::Platform);
    }

    async fn reject_external_reserved_tool_commit(
        &self,
        request: &HostRequestEnvelope,
    ) -> Option<HostResponseEnvelope> {
        let HostCommand::CommitTool(command) = &request.command else {
            return None;
        };
        let invocation = match self
            .runtime
            .get_tool_invocation_for_host_worker(&command.invocation_id)
            .await
        {
            Ok(invocation) => invocation,
            Err(error) => {
                return Some(HostResponseEnvelope::failure(
                    request.command_id.clone(),
                    HostError::new("host_internal_error", error.to_string(), true),
                ));
            }
        };
        invocation
            .filter(|invocation| invocation.execution_route != LocalAgentToolExecutionRoute::Platform)
            .map(|_| {
                HostResponseEnvelope::failure(
                    request.command_id.clone(),
                    HostError::new(
                        "reserved_command",
                        "shared-runtime tool results can only be committed by the built-in local worker",
                        false,
                    ),
                )
            })
    }

    async fn apply_hook_preflight_to_external_claim(
        &self,
        mut response: HostResponseEnvelope,
    ) -> HostResponseEnvelope {
        let Some(hooks) = self.hook_coordinator.as_ref() else {
            return response;
        };
        let invocation = match response.result.as_ref() {
            Some(HostResult::ToolClaim { claim: Some(claim) }) => claim.invocation.clone(),
            _ => return response,
        };
        let hook_preflight = match hooks.tool_preflight_outcome(&invocation).await {
            Ok(outcome) => outcome,
            Err(error) => Some(LocalAgentToolOutcome::Failed {
                error: "trusted tool Hook preflight failed closed".to_string(),
                detail: serde_json::json!({
                    "phase": "trusted_plugin_hook",
                    "error": error,
                }),
            }),
        };
        if let (Some(outcome), Some(HostResult::ToolClaim { claim: Some(claim) })) =
            (hook_preflight, response.result.as_mut())
        {
            claim.preflight_outcome = Some(outcome);
        }
        response
    }

    async fn health_response(&self, request: HostRequestEnvelope) -> HostResponseEnvelope {
        let mut response = self.runtime.handle(request.clone()).await;
        if !response.ok {
            return response;
        }
        let empty_tool_names: &[String] = &[];
        let (include_tool_names, exclude_tool_names, execution_route) = self
            .tool_scheduler
            .as_ref()
            .map(LocalToolScheduler::claim_filter)
            .unwrap_or((None, empty_tool_names, None));
        let queued_ready_work_items = match self
            .runtime
            .ready_host_work_count(
                &self.owner_user_id,
                include_tool_names,
                exclude_tool_names,
                execution_route,
            )
            .await
        {
            Ok(count) => count,
            Err(error) => {
                return HostResponseEnvelope::failure(
                    request.command_id,
                    HostError::new("host_snapshot_failed", error.to_string(), true),
                );
            }
        };
        let active_work_items = self.work_activity.active_count();
        let admission_in_flight_work_items = self.admission_activity.active_count();
        let active_memory_sync_items = self.memory_work_activity.active_count();
        let scheduler = LocalAgentHostSchedulerSnapshot {
            maximum_concurrent_work_items: self.maximum_concurrent_work_items,
            target_concurrent_work_items: *self.target_concurrent_work_items.borrow(),
            effective_maximum_concurrent_work_items: *self
                .effective_maximum_concurrent_work_items
                .borrow(),
            pressure_level: *self.pressure_level.borrow(),
            active_work_items,
            idle_workers: self
                .maximum_concurrent_work_items
                .saturating_sub(active_work_items),
            queued_ready_work_items,
            accepting_new_work: *self.accepting_new_work.borrow(),
            active_memory_sync_items,
            admission_in_flight_work_items,
        };
        if let Some(HostResult::Health {
            scheduler: result_scheduler,
            ..
        }) = response.result.as_mut()
        {
            *result_scheduler = Some(scheduler);
        }
        response
    }

    fn update_scheduler_pressure(
        &self,
        request: HostRequestEnvelope,
        pressure_level: LocalAgentResourcePressureLevel,
        target_concurrent_work_items: usize,
    ) -> HostResponseEnvelope {
        if target_concurrent_work_items > self.maximum_concurrent_work_items {
            return HostResponseEnvelope::failure(
                request.command_id,
                HostError::new(
                    "invalid_request",
                    "scheduler target exceeds the configured maximum",
                    false,
                ),
            );
        }
        let effective_maximum_concurrent_work_items =
            effective_worker_limit(target_concurrent_work_items, pressure_level);
        self.pressure_level.send_replace(pressure_level);
        self.target_concurrent_work_items
            .send_replace(target_concurrent_work_items);
        self.effective_maximum_concurrent_work_items
            .send_replace(effective_maximum_concurrent_work_items);
        HostResponseEnvelope::success(
            request.command_id,
            HostResult::SchedulerPressureUpdated {
                pressure_level,
                target_concurrent_work_items,
                effective_maximum_concurrent_work_items,
            },
        )
    }

    async fn update_scheduler_admission(
        &self,
        request: HostRequestEnvelope,
        accepting_new_work: bool,
    ) -> HostResponseEnvelope {
        let barrier = self.admission_barrier.write().await;
        self.accepting_new_work.send_replace(accepting_new_work);
        drop(barrier);
        if accepting_new_work {
            self.wake();
        }
        HostResponseEnvelope::success(
            request.command_id,
            HostResult::SchedulerAdmissionUpdated { accepting_new_work },
        )
    }
}
