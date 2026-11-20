impl LocalHookCoordinator {
    pub(crate) async fn enqueue_interrupt(
        self: &Arc<Self>,
        run: &LocalAgentRunRecord,
        reason: &str,
    ) -> Result<(), String> {
        let session_id = run
            .input
            .get("conversation_id")
            .or_else(|| run.input.get("source_conversation_id"))
            .and_then(Value::as_str)
            .unwrap_or(&run.owner_entity_id);
        let mut event = HookEvent::new_for_owner_with_sequence(
            &run.owner_user_id,
            "Interrupt",
            &run.run_id,
            run.version,
            "",
            json!({
                "session_id": session_id,
                "hook_event_name": "Interrupt",
                "run_id": run.run_id,
                "reason": reason,
                "cwd": self.workspace_display(),
            }),
        )?;
        let prepared = self.persist_event_snapshot(&mut event).await?;
        if prepared.status == "completed" {
            return Ok(());
        }
        let coordinator = Arc::clone(self);
        tokio::spawn(async move {
            // Interrupt is notification-only. Cancellation is already durable,
            // and no Hook output may restore or otherwise mutate the Run.
            let _ = coordinator.execute_event(event).await;
        });
        Ok(())
    }

    pub(crate) async fn enqueue_session_end(
        self: &Arc<Self>,
        owner_user_id: &str,
        reason: &str,
    ) -> Result<usize, String> {
        let sessions = self
            .active_sessions
            .lock()
            .map_err(|_| "Hook session state lock is poisoned".to_string())?
            .iter()
            .map(|(session_id, session)| (session_id.clone(), session.sequence))
            .collect::<Vec<_>>();
        self.enqueue_session_end_for(owner_user_id, reason, sessions)
            .await
    }

    pub(crate) async fn expire_idle_sessions(
        self: &Arc<Self>,
        owner_user_id: &str,
        now: Instant,
        idle_timeout: Duration,
    ) -> Result<usize, String> {
        if self.connected_clients.load(Ordering::Acquire) != 0 {
            return Ok(0);
        }
        let expired = {
            let mut active = self
                .active_sessions
                .lock()
                .map_err(|_| "Hook session state lock is poisoned".to_string())?;
            let expired_ids = active
                .iter()
                .filter(|(_, session)| {
                    now.checked_duration_since(session.last_active_at)
                        .is_some_and(|idle| idle >= idle_timeout)
                })
                .map(|(session_id, _)| session_id.clone())
                .collect::<Vec<_>>();
            expired_ids
                .into_iter()
                .filter_map(|session_id| {
                    active
                        .remove(&session_id)
                        .map(|session| (session_id, session.sequence))
                })
                .collect::<Vec<_>>()
        };
        if expired.is_empty() {
            return Ok(0);
        }
        {
            let mut generations = self
                .session_generations
                .lock()
                .map_err(|_| "Hook session generation lock is poisoned".to_string())?;
            for (session_id, _) in &expired {
                let generation = generations.entry(session_id.clone()).or_default();
                *generation = generation.saturating_add(1);
            }
        }
        self.enqueue_session_end_for(owner_user_id, "idle_timeout", expired)
            .await
    }

    async fn enqueue_session_end_for(
        self: &Arc<Self>,
        owner_user_id: &str,
        reason: &str,
        sessions: Vec<(String, u64)>,
    ) -> Result<usize, String> {
        let mut enqueued = 0;
        for (session_id, sequence) in sessions {
            let mut event = HookEvent::new_for_owner_with_sequence(
                owner_user_id,
                "SessionEnd",
                &session_id,
                sequence,
                reason,
                json!({
                    "session_id": session_id,
                    "hook_event_name": "SessionEnd",
                    "reason": reason,
                    "cwd": self.workspace_display(),
                }),
            )?;
            let prepared = self.persist_event_snapshot(&mut event).await?;
            if prepared.status == "completed" {
                continue;
            }
            enqueued += 1;
            let coordinator = Arc::clone(self);
            tokio::spawn(async move {
                // SessionEnd is notification-only. Persist it before returning
                // from shutdown, but never make process exit wait on a plugin.
                let _ = coordinator.execute_event(event).await;
            });
        }
        Ok(enqueued)
    }

    pub(crate) async fn recover_pending_events(
        self: &Arc<Self>,
        owner_user_id: &str,
    ) -> Result<usize, String> {
        let pending = self
            .runtime
            .list_pending_hook_events_for_host_worker(owner_user_id, 256)
            .await
            .map_err(|error| error.to_string())?;
        let mut recovered = 0;
        for record in pending {
            let event = match HookEvent::from_record(&record) {
                Ok(event) => event,
                Err(_) => continue,
            };
            recovered += 1;
            let coordinator = Arc::clone(self);
            tokio::spawn(async move {
                let _ = coordinator.execute_event(event).await;
            });
        }
        Ok(recovered)
    }

    pub(crate) async fn abandon_interrupted_async_invocations(
        &self,
        owner_user_id: &str,
    ) -> Result<u64, String> {
        self.runtime
            .abandon_running_async_hook_invocations_for_host_worker(owner_user_id)
            .await
            .map_err(|error| error.to_string())
    }
}
