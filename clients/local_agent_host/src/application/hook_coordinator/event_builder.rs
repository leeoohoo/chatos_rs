impl LocalHookCoordinator {
    async fn events_for_claim(&self, claim: &LocalAgentRunClaim) -> Result<Vec<HookEvent>, String> {
        let run = &claim.run;
        let mut events = Vec::new();
        if run.owner_entity_type == "conversation_turn" {
            let conversation_id = required_input_string(run, "conversation_id")?;
            let turn_id = required_input_string(run, "turn_id")?;
            let prompt = run
                .input
                .get("message")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} Run is missing message", run.run_id))?;
            if let Some(marker) = latest_compaction_marker(run)? {
                events.push(HookEvent::new_for_owner_with_sequence(
                    &run.owner_user_id,
                    "PostCompact",
                    conversation_id,
                    stable_sequence(&["post_compact", conversation_id, &marker]),
                    "auto",
                    json!({
                        "session_id": conversation_id,
                        "hook_event_name": "PostCompact",
                        "trigger": "auto",
                        "compact_marker": marker,
                        "cwd": self.workspace_display(),
                    }),
                )?);
            }
            let session = self.session_for(run, conversation_id).await?;
            events.push(HookEvent::new_for_owner_with_sequence(
                &run.owner_user_id,
                "SessionStart",
                conversation_id,
                session.sequence,
                &session.source,
                json!({
                    "session_id": conversation_id,
                    "hook_event_name": "SessionStart",
                    "source": session.source,
                    "cwd": self.workspace_display(),
                }),
            )?);
            events.push(HookEvent::new(
                run,
                "UserPromptSubmit",
                turn_id,
                "",
                json!({
                    "session_id": conversation_id,
                    "hook_event_name": "UserPromptSubmit",
                    "prompt": prompt,
                    "cwd": self.workspace_display(),
                }),
            )?);
        } else if self.is_delegated_task(run).await? {
            let session_id = run
                .input
                .get("source_conversation_id")
                .and_then(Value::as_str)
                .unwrap_or(&run.owner_entity_id);
            self.touch_session(session_id)?;
            events.push(HookEvent::new(
                run,
                "SubagentStart",
                &run.run_id,
                "task_execution",
                json!({
                    "session_id": session_id,
                    "hook_event_name": "SubagentStart",
                    "agent_id": run.run_id,
                    "agent_type": "task_execution",
                    "cwd": self.workspace_display(),
                }),
            )?);
        }
        events.extend(self.post_tool_events(run)?);
        Ok(events)
    }

    fn post_tool_events(&self, run: &LocalAgentRunRecord) -> Result<Vec<HookEvent>, String> {
        let Some(invocations) = run
            .continuation_input
            .as_ref()
            .and_then(|value| value.get("invocations"))
            .and_then(Value::as_array)
        else {
            return Ok(Vec::new());
        };
        let mut events = Vec::with_capacity(invocations.len());
        for invocation in invocations {
            let Some(invocation_id) = invocation.get("invocation_id").and_then(Value::as_str)
            else {
                continue;
            };
            let Some(call_id) = invocation.get("call_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(tool_name) = invocation.get("tool_name").and_then(Value::as_str) else {
                continue;
            };
            let arguments = invocation.get("arguments").unwrap_or(&Value::Null);
            let response = invocation.get("result").cloned().unwrap_or_else(|| {
                json!({
                    "status": invocation.get("status"),
                    "error": invocation.get("error"),
                })
            });
            events.push(HookEvent::new(
                run,
                "PostToolUse",
                invocation_id,
                tool_name,
                tool_hook_payload(
                    run,
                    "PostToolUse",
                    call_id,
                    tool_name,
                    arguments,
                    Some(&response),
                    self.workspace_display(),
                ),
            )?);
        }
        Ok(events)
    }

    async fn session_for(
        &self,
        run: &LocalAgentRunRecord,
        conversation_id: &str,
    ) -> Result<ActiveHookSession, String> {
        let compact_marker = latest_compaction_marker(run)?;
        {
            let mut active_sessions = self
                .active_sessions
                .lock()
                .map_err(|_| "Hook session state lock is poisoned".to_string())?;
            if let Some(active) = active_sessions.get_mut(conversation_id) {
                if compact_marker.is_none() || compact_marker == active.compact_marker {
                    active.last_active_at = Instant::now();
                    return Ok(active.clone());
                }
            }
        }

        let selected = if let Some(marker) = compact_marker.clone() {
            let compact_sequence = stable_sequence(&["compact", conversation_id, &marker]);
            let existing = self
                .runtime
                .get_hook_event_for_host_worker(
                    &run.owner_user_id,
                    "SessionStart",
                    conversation_id,
                    compact_sequence,
                )
                .await
                .map_err(|error| error.to_string())?;
            if existing
                .as_ref()
                .is_none_or(|event| event.status != "completed")
            {
                ActiveHookSession {
                    sequence: compact_sequence,
                    source: "compact".to_string(),
                    compact_marker: Some(marker),
                    last_active_at: Instant::now(),
                }
            } else {
                ActiveHookSession {
                    sequence: self.session_sequence(conversation_id)?,
                    source: "resume".to_string(),
                    compact_marker: Some(marker),
                    last_active_at: Instant::now(),
                }
            }
        } else {
            let existed = self
                .runtime
                .has_hook_event_for_host_worker(&run.owner_user_id, "SessionStart", conversation_id)
                .await
                .map_err(|error| error.to_string())?;
            ActiveHookSession {
                sequence: self.session_sequence(conversation_id)?,
                source: if existed { "resume" } else { "startup" }.to_string(),
                compact_marker: None,
                last_active_at: Instant::now(),
            }
        };
        self.active_sessions
            .lock()
            .map_err(|_| "Hook session state lock is poisoned".to_string())?
            .insert(conversation_id.to_string(), selected.clone());
        Ok(selected)
    }

    fn session_sequence(&self, conversation_id: &str) -> Result<u64, String> {
        let generation = self
            .session_generations
            .lock()
            .map_err(|_| "Hook session generation lock is poisoned".to_string())?
            .get(conversation_id)
            .copied()
            .unwrap_or(0);
        if generation == 0 {
            return Ok(self.process_session_sequence);
        }
        Ok(stable_sequence(&[
            "process-session",
            &self.process_session_sequence.to_string(),
            conversation_id,
            &generation.to_string(),
        ]))
    }

    fn touch_session(&self, conversation_id: &str) -> Result<(), String> {
        if let Some(session) = self
            .active_sessions
            .lock()
            .map_err(|_| "Hook session state lock is poisoned".to_string())?
            .get_mut(conversation_id)
        {
            session.last_active_at = Instant::now();
        }
        Ok(())
    }

    async fn is_delegated_task(&self, run: &LocalAgentRunRecord) -> Result<bool, String> {
        if run.owner_entity_type != "task" || run.profile_key != TASK_EXECUTION_PROFILE_KEY {
            return Ok(false);
        }
        let Some(source_run_id) = run.input.get("source_run_id").and_then(Value::as_str) else {
            return Ok(false);
        };
        if source_run_id.is_empty() || source_run_id == run.run_id {
            return Ok(false);
        }
        let mut current_id = source_run_id.to_string();
        let mut visited = std::collections::HashSet::from([run.run_id.clone()]);
        for _ in 0..MAXIMUM_PARENT_DEPTH {
            if !visited.insert(current_id.clone()) {
                return Ok(false);
            }
            let Some(parent) = self
                .runtime
                .get_run_for_host_worker(&current_id)
                .await
                .map_err(|error| error.to_string())?
            else {
                return Ok(false);
            };
            if parent.owner_user_id != run.owner_user_id {
                return Ok(false);
            }
            let Some(next) = parent.input.get("source_run_id").and_then(Value::as_str) else {
                return Ok(true);
            };
            if next.is_empty() {
                return Ok(true);
            }
            current_id = next.to_string();
        }
        Ok(false)
    }
}
