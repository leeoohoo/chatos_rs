impl LocalHookCoordinator {
    async fn execute_event(&self, event: HookEvent) -> Result<PersistedEventOutcome, String> {
        self.execute_event_at(event, self.workspace_root.clone())
            .await
    }

    async fn execute_event_at(
        &self,
        event: HookEvent,
        workspace_root: Option<PathBuf>,
    ) -> Result<PersistedEventOutcome, String> {
        let event_lock = {
            let mut inflight = self
                .inflight_events
                .lock()
                .map_err(|_| "Hook event lock registry is poisoned".to_string())?;
            inflight.retain(|_, event_lock| event_lock.strong_count() != 0);
            if let Some(event_lock) = inflight.get(&event.spec.event_id).and_then(Weak::upgrade) {
                event_lock
            } else {
                let event_lock = Arc::new(AsyncMutex::new(()));
                inflight.insert(event.spec.event_id.clone(), Arc::downgrade(&event_lock));
                event_lock
            }
        };
        let _event_guard = event_lock.lock().await;
        self.execute_event_serial(event, workspace_root).await
    }

    async fn execute_event_serial(
        &self,
        mut event: HookEvent,
        workspace_root: Option<PathBuf>,
    ) -> Result<PersistedEventOutcome, String> {
        let prepared = self.persist_event_snapshot(&mut event).await?;
        if prepared.status == "completed" {
            return decode_persisted_outcome(&event.event_type, prepared.additional_context);
        }

        let bundles = self
            .runtime
            .plugin_hook_bundles_for_host_worker(
                &event.spec.owner_user_id,
                &event.spec.active_bundle_ids,
            )
            .await
            .map_err(|error| error.to_string())?;
        let mut matches = matching_hooks(bundles, &event)?;
        matches.sort_by(|left, right| {
            (&left.0.spec.bundle_id, &left.1.hook_id)
                .cmp(&(&right.0.spec.bundle_id, &right.1.hook_id))
        });

        let mut invocations = Vec::with_capacity(matches.len());
        for (bundle, hook) in matches {
            let spec = LocalHookInvocationSpec {
                invocation_id: stable_id(
                    "hook-invocation",
                    &[&event.spec.event_id, &bundle.spec.bundle_id, &hook.hook_id],
                ),
                event_id: event.spec.event_id.clone(),
                bundle_id: bundle.spec.bundle_id.clone(),
                hook_id: hook.hook_id.clone(),
                is_async: hook.is_async,
            };
            let record = self
                .runtime
                .begin_hook_invocation_for_host_worker(&spec)
                .await
                .map_err(|error| error.to_string())?;
            invocations.push((bundle, hook, record));
        }

        let mut synchronous = Vec::new();
        let mut asynchronous = Vec::new();
        for invocation in invocations {
            if invocation.1.is_async {
                asynchronous.push(invocation);
            } else {
                synchronous.push(invocation);
            }
        }
        let results = join_all(synchronous.into_iter().map(|(bundle, hook, record)| {
            Self::execute_invocation_with(
                Arc::clone(&self.runtime),
                Arc::clone(&self.runner),
                Arc::clone(&self.hook_execution_permits),
                workspace_root.clone(),
                &event,
                bundle,
                hook,
                record,
            )
        }))
        .await;
        let mut outcomes = Vec::with_capacity(results.len());
        for outcome in results {
            outcomes.push(outcome?);
        }
        let outcome = merge_outcomes(&event.event_type, outcomes)?;
        let encoded = serde_json::to_string(&outcome)
            .map_err(|error| format!("encode Hook event outcome failed: {error}"))?;
        self.runtime
            .complete_hook_event_for_host_worker(&event.spec.event_id, Some(&encoded))
            .await
            .map_err(|error| error.to_string())?;
        for (bundle, hook, record) in asynchronous {
            if record.status != "running" {
                continue;
            }
            let runtime = Arc::clone(&self.runtime);
            let runner = Arc::clone(&self.runner);
            let hook_execution_permits = Arc::clone(&self.hook_execution_permits);
            let workspace_root = workspace_root.clone();
            let event = event.clone();
            tokio::spawn(async move {
                let _ = Self::execute_invocation_with(
                    runtime,
                    runner,
                    hook_execution_permits,
                    workspace_root,
                    &event,
                    bundle,
                    hook,
                    record,
                )
                .await;
            });
        }
        Ok(outcome)
    }

    async fn persist_event_snapshot(
        &self,
        event: &mut HookEvent,
    ) -> Result<LocalHookEventRecord, String> {
        if let Some(existing) = self
            .runtime
            .get_hook_event_for_host_worker(
                &event.spec.owner_user_id,
                &event.event_type,
                &event.spec.subject_id,
                event.spec.sequence,
            )
            .await
            .map_err(|error| error.to_string())?
        {
            if existing.spec.payload_json.is_empty() && existing.status != "completed" {
                return Err(format!(
                    "pending Hook event predates recoverable payload storage: {}",
                    existing.spec.event_id
                ));
            }
            if !existing.spec.payload_json.is_empty() {
                *event = HookEvent::from_record(&existing)?;
            }
            return Ok(existing);
        }
        let bundles = self
            .runtime
            .active_plugin_hook_bundles_for_host_worker(&event.spec.owner_user_id)
            .await
            .map_err(|error| error.to_string())?;
        let mut bundle_ids = bundles
            .into_iter()
            .map(|bundle| bundle.spec.bundle_id)
            .collect::<Vec<_>>();
        bundle_ids.sort();
        bundle_ids.dedup();
        event.spec.active_bundle_ids = bundle_ids;
        self.runtime
            .prepare_hook_event_for_host_worker(&event.spec)
            .await
            .map_err(|error| error.to_string())
    }

    async fn execute_invocation_with(
        runtime: Arc<LocalAgentRuntime>,
        runner: Arc<dyn LocalHookRunner>,
        hook_execution_permits: Arc<Semaphore>,
        workspace_root: Option<PathBuf>,
        event: &HookEvent,
        bundle: LocalPluginHookBundleRecord,
        hook: LocalPluginHookDefinition,
        record: LocalHookInvocationRecord,
    ) -> Result<InvocationOutcome, String> {
        if record.status != "running" {
            let protocol_output = decode_invocation_output(record.additional_context.as_deref());
            return Ok(InvocationOutcome {
                bundle_id: bundle.spec.bundle_id,
                plugin_id: bundle.spec.plugin_id,
                hook_id: hook.hook_id,
                block_reason: (record.status == "blocked")
                    .then_some(record.stderr_summary)
                    .flatten(),
                status: record.status,
                additional_context: protocol_output.additional_context,
                permission_decision: protocol_output.permission_decision,
                decision_reason: protocol_output.decision_reason,
                updated_input: protocol_output.updated_input,
            });
        }
        let _permit = hook_execution_permits
            .acquire_owned()
            .await
            .map_err(|_| "Hook execution limiter is closed".to_string())?;
        let result = runner
            .run(LocalHookInvocation {
                bundle: bundle.spec.clone(),
                hook: hook.clone(),
                stdin_json: event.stdin_json.clone(),
                workspace_root,
            })
            .await;
        let completion = match result {
            Ok(result) => completion_from_result(&event.event_type, &hook, &result),
            Err(error) => LocalHookInvocationCompletion {
                status: "failed".to_string(),
                exit_code: None,
                timed_out: false,
                duration_ms: 0,
                stdout_sha256: None,
                stdout_bytes: 0,
                stdout_overflow: false,
                stderr_sha256: None,
                stderr_bytes: 0,
                stderr_overflow: false,
                stderr_summary: Some(safe_summary(error.as_bytes())),
                additional_context: None,
                stdout_spill: None,
                stderr_spill: None,
            },
        };
        let completed = runtime
            .complete_hook_invocation_for_host_worker(
                &record.spec.invocation_id,
                record.attempt,
                &completion,
            )
            .await
            .map_err(|error| error.to_string())?;
        let protocol_output = decode_invocation_output(completed.additional_context.as_deref());
        Ok(InvocationOutcome {
            bundle_id: bundle.spec.bundle_id,
            plugin_id: bundle.spec.plugin_id,
            hook_id: hook.hook_id,
            status: completed.status,
            additional_context: protocol_output.additional_context,
            block_reason: if completion.status == "blocked" {
                completion.stderr_summary
            } else {
                None
            },
            permission_decision: protocol_output.permission_decision,
            decision_reason: protocol_output.decision_reason,
            updated_input: protocol_output.updated_input,
        })
    }

    fn workspace_display(&self) -> String {
        self.workspace_root
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    }
}
