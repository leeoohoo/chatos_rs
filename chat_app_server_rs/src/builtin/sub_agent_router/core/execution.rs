use super::super::*;
use super::agent_resolver::resolve_agent_and_command;
use super::job_executor::execute_job;

pub(crate) fn run_sub_agent_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task": { "type": "string" },
            "agent_id": {
                "anyOf": [
                    { "type": "string" },
                    { "type": "null" }
                ]
            },
            "skills": {
                "anyOf": [
                    { "type": "array", "items": { "type": "string" } },
                    { "type": "null" }
                ]
            },
            "query": {
                "anyOf": [
                    { "type": "string" },
                    { "type": "null" }
                ]
            },
            "command_id": {
                "anyOf": [
                    { "type": "string" },
                    { "type": "null" }
                ]
            }
        },
        "additionalProperties": false,
        "required": ["task", "agent_id"]
    })
}

pub(crate) fn run_sub_agent_sync(
    ctx: BoundContext,
    args: Value,
    tool_ctx: &ToolContext,
) -> Result<Value, String> {
    let task = required_trimmed_string(&args, "task")?;
    trace_router_node(
        "run_sub_agent",
        "resolve_start",
        None,
        Some(tool_ctx.session_id),
        Some(tool_ctx.run_id),
        Some(json!({
            "task": truncate_for_event(task.as_str(), 2_000),
            "agent_id": optional_trimmed_string(&args, "agent_id"),
            "command_id": optional_trimmed_string(&args, "command_id"),
            "query": optional_trimmed_string(&args, "query"),
            "skills": parse_string_array(args.get("skills")).unwrap_or_default(),
        })),
    );
    let resolved = match resolve_agent_and_command(&ctx, task.as_str(), &args) {
        Ok(value) => value,
        Err(err) => {
            trace_router_node(
                "run_sub_agent",
                "resolve_error",
                None,
                Some(tool_ctx.session_id),
                Some(tool_ctx.run_id),
                Some(json!({ "error": err })),
            );
            return Err(err);
        }
    };

    let job = create_job(
        task.as_str(),
        Some(resolved.agent.id.clone()),
        resolved.command.as_ref().map(|c| c.id.clone()),
        Some(args.clone()),
        tool_ctx.session_id,
        tool_ctx.run_id,
    );
    if let Some(on_stream_chunk) = tool_ctx.on_stream_chunk.clone() {
        set_job_stream_sink(job.id.as_str(), on_stream_chunk);
    }
    let _ = update_job_status(job.id.as_str(), "running", None, None);
    append_job_event(
        job.id.as_str(),
        "start",
        Some(json!({
            "agent_id": resolved.agent.id,
            "command_id": resolved.command.as_ref().map(|c| c.id.clone()).unwrap_or_default()
        })),
        tool_ctx.session_id,
        tool_ctx.run_id,
    );
    append_job_event(
        job.id.as_str(),
        "resolve_finish",
        Some(json!({
            "agent_id": resolved.agent.id,
            "agent_name": resolved.agent.name,
            "command_id": resolved.command.as_ref().map(|c| c.id.clone()),
            "skills": resolved
                .used_skills
                .iter()
                .map(|skill| skill.id.clone())
                .collect::<Vec<_>>(),
            "reason": resolved.reason,
        })),
        tool_ctx.session_id,
        tool_ctx.run_id,
    );

    let execution = JobExecutionContext {
        ctx: ctx.clone(),
        task,
        args,
        resolved: resolved.clone(),
        session_id: tool_ctx.session_id.to_string(),
        run_id: tool_ctx.run_id.to_string(),
        conversation_turn_id: tool_ctx.conversation_turn_id.to_string(),
        job_id: job.id.clone(),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    set_cancel_flag(job.id.as_str(), cancel_flag.clone());
    append_job_event(
        job.id.as_str(),
        "cancel_flag_registered",
        Some(json!({
            "initial": false,
        })),
        tool_ctx.session_id,
        tool_ctx.run_id,
    );

    if abort_registry::is_aborted(tool_ctx.session_id) {
        cancel_flag.store(true, Ordering::Relaxed);
        append_job_event(
            job.id.as_str(),
            "cancel_flag_pre_aborted",
            None,
            tool_ctx.session_id,
            tool_ctx.run_id,
        );
    }

    let session_id = tool_ctx.session_id.to_string();
    let run_id = tool_ctx.run_id.to_string();
    let job_id_for_watcher = job.id.clone();
    let watcher_done = Arc::new(AtomicBool::new(false));
    let watcher_done_flag = watcher_done.clone();
    let cancel_flag_for_watcher = cancel_flag.clone();
    let cancel_watcher = if session_id.trim().is_empty() {
        append_job_event(
            job.id.as_str(),
            "cancel_watcher_disabled",
            Some(json!({ "reason": "empty_session_id" })),
            tool_ctx.session_id,
            tool_ctx.run_id,
        );
        None
    } else {
        append_job_event(
            job.id.as_str(),
            "cancel_watcher_started",
            Some(json!({ "poll_interval_ms": 100 })),
            tool_ctx.session_id,
            tool_ctx.run_id,
        );
        Some(thread::spawn(move || {
            while !watcher_done_flag.load(Ordering::Relaxed)
                && !cancel_flag_for_watcher.load(Ordering::Relaxed)
            {
                if abort_registry::is_aborted(session_id.as_str()) {
                    cancel_flag_for_watcher.store(true, Ordering::Relaxed);
                    trace_router_node(
                        "run_sub_agent",
                        "cancel_watcher_abort_detected",
                        Some(job_id_for_watcher.as_str()),
                        Some(session_id.as_str()),
                        Some(run_id.as_str()),
                        None,
                    );
                    break;
                }
                thread::sleep(StdDuration::from_millis(100));
            }
            trace_router_node(
                "run_sub_agent",
                "cancel_watcher_exit",
                Some(job_id_for_watcher.as_str()),
                Some(session_id.as_str()),
                Some(run_id.as_str()),
                Some(json!({
                    "cancelled": cancel_flag_for_watcher.load(Ordering::Relaxed),
                    "done": watcher_done_flag.load(Ordering::Relaxed),
                })),
            );
        }))
    };

    let (status, payload, error_text) = match execute_job(
        execution.clone(),
        Some(cancel_flag.as_ref()),
    ) {
        Ok((status, payload)) => (status, payload, None),
        Err(err) => {
            let cancelled =
                err.eq_ignore_ascii_case("aborted") || err.eq_ignore_ascii_case("cancelled");
            let status = if cancelled { "cancelled" } else { "error" };
            append_job_event(
                job.id.as_str(),
                "execute_error",
                Some(json!({
                    "status": status,
                    "error": err,
                })),
                tool_ctx.session_id,
                tool_ctx.run_id,
            );
            (
                status.to_string(),
                json!({
                    "status": status,
                    "job_id": execution.job_id,
                    "agent_id": execution.resolved.agent.id,
                    "agent_name": execution.resolved.agent.name,
                    "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
                    "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                    "reason": execution.resolved.reason,
                    "error": err,
                }),
                Some(err),
            )
        }
    };

    watcher_done.store(true, Ordering::Relaxed);
    append_job_event(
        job.id.as_str(),
        "cancel_watcher_stop_signal",
        None,
        tool_ctx.session_id,
        tool_ctx.run_id,
    );
    if let Some(handle) = cancel_watcher {
        let _ = handle.join();
        append_job_event(
            job.id.as_str(),
            "cancel_watcher_joined",
            None,
            tool_ctx.session_id,
            tool_ctx.run_id,
        );
    }
    remove_cancel_flag(job.id.as_str());
    append_job_event(
        job.id.as_str(),
        "cancel_flag_removed",
        None,
        tool_ctx.session_id,
        tool_ctx.run_id,
    );
    let final_status = map_status_to_job_state(status.as_str());
    let _ = update_job_status(
        job.id.as_str(),
        final_status,
        Some(payload.to_string()),
        error_text,
    );
    append_job_event(
        job.id.as_str(),
        "finish",
        Some(json!({
            "status": final_status,
        })),
        tool_ctx.session_id,
        tool_ctx.run_id,
    );

    let mut response_payload = payload;
    if let Value::Object(ref mut map) = response_payload {
        map.insert(
            "job_events".to_string(),
            serde_json::to_value(list_job_events(job.id.as_str())).unwrap_or_else(|_| json!([])),
        );
        map.insert(
            "trace_log_path".to_string(),
            trace_log_path_string()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
    }

    remove_job_stream_sink(job.id.as_str());

    Ok(text_result(with_chatos(
        ctx.server_name.as_str(),
        "run_sub_agent",
        response_payload,
        status.as_str(),
    )))
}
