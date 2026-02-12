use super::super::*;
use super::agent_resolver::resolve_command_cwd;

pub(crate) fn execute_job(
    execution: JobExecutionContext,
    cancel_flag: Option<&AtomicBool>,
) -> Result<(String, Value), String> {
    if let Some(flag) = cancel_flag {
        if flag.load(Ordering::Relaxed) {
            append_job_event(
                execution.job_id.as_str(),
                "execute_cancelled_precheck",
                Some(json!({ "reason": "cancel_flag" })),
                execution.session_id.as_str(),
                execution.run_id.as_str(),
            );
            return Ok((
                "cancelled".to_string(),
                json!({
                    "status": "cancelled",
                    "job_id": execution.job_id,
                    "agent_id": execution.resolved.agent.id,
                    "agent_name": execution.resolved.agent.name,
                    "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
                    "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                    "reason": execution.resolved.reason,
                    "error": "cancelled"
                }),
            ));
        }
    }

    let requested_model = optional_trimmed_string(&execution.args, "caller_model")
        .or_else(|| optional_trimmed_string(&execution.args, "model"));
    let allow_policy = resolve_allow_prefixes(execution.args.get("mcp_allow_prefixes"));
    append_job_event(
        execution.job_id.as_str(),
        "execute_prepare",
        Some(json!({
            "agent_id": execution.resolved.agent.id,
            "agent_name": execution.resolved.agent.name,
            "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
            "skills": execution
                .resolved
                .used_skills
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>(),
            "requested_model": requested_model.clone(),
            "allow_prefixes": allow_policy.prefixes.clone(),
            "query": optional_trimmed_string(&execution.args, "query").unwrap_or_default(),
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    let run_env = build_env(
        execution.task.as_str(),
        &execution.resolved.agent,
        execution.resolved.command.as_ref(),
        &execution.resolved.used_skills,
        execution.session_id.as_str(),
        execution.run_id.as_str(),
        optional_trimmed_string(&execution.args, "query").as_deref(),
        optional_trimmed_string(&execution.args, "model").as_deref(),
        optional_trimmed_string(&execution.args, "caller_model").as_deref(),
        &allow_policy.prefixes,
        execution.ctx.project_id.as_deref(),
    );
    append_job_event(
        execution.job_id.as_str(),
        "env_ready",
        Some(json!({
            "entries": run_env.len(),
            "keys": run_env.keys().cloned().collect::<Vec<_>>(),
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    if let Some(cmd) = execution
        .resolved
        .command
        .clone()
        .and_then(|command| command.exec)
    {
        append_job_event(
            execution.job_id.as_str(),
            "execute_mode_selected",
            Some(json!({
                "mode": "command",
            })),
            execution.session_id.as_str(),
            execution.run_id.as_str(),
        );
        let cwd = resolve_command_cwd(
            execution.ctx.workspace_root.as_path(),
            execution
                .resolved
                .command
                .as_ref()
                .and_then(|command| command.cwd.as_deref()),
        );

        append_job_event(
            execution.job_id.as_str(),
            "command_start",
            Some(json!({
                "command": cmd.clone(),
                "cwd": cwd,
                "timeout_ms": execution.ctx.timeout_ms,
            })),
            execution.session_id.as_str(),
            execution.run_id.as_str(),
        );

        let result = run_command(
            &cmd,
            &run_env,
            cwd.as_deref(),
            execution.ctx.timeout_ms,
            execution.ctx.max_output_bytes,
            None,
            cancel_flag,
        )?;

        let status = if matches!(result.error.as_deref(), Some("cancelled")) {
            "cancelled".to_string()
        } else if result.exit_code.unwrap_or(0) == 0 && !result.timed_out {
            "ok".to_string()
        } else {
            "error".to_string()
        };

        let payload = json!({
            "status": status,
            "job_id": execution.job_id,
            "agent_id": execution.resolved.agent.id,
            "agent_name": execution.resolved.agent.name,
            "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
            "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
            "reason": execution.resolved.reason,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.exit_code,
            "signal": result.signal,
            "duration_ms": result.duration_ms,
            "started_at": result.started_at,
            "finished_at": result.finished_at,
            "stdout_truncated": result.stdout_truncated,
            "stderr_truncated": result.stderr_truncated,
            "error": result.error,
            "timed_out": result.timed_out,
        });

        append_job_event(
            execution.job_id.as_str(),
            "command_finish",
            Some(json!({
                "status": payload.get("status").cloned().unwrap_or(Value::String("error".to_string())),
                "exit_code": result.exit_code,
                "signal": result.signal,
                "duration_ms": result.duration_ms,
                "timed_out": result.timed_out,
                "error": result.error,
                "stdout_preview": truncate_for_event(result.stdout.as_str(), 2000),
                "stderr_preview": truncate_for_event(result.stderr.as_str(), 2000),
                "stdout_truncated": result.stdout_truncated,
                "stderr_truncated": result.stderr_truncated,
            })),
            execution.session_id.as_str(),
            execution.run_id.as_str(),
        );

        return Ok((
            payload
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("error")
                .to_string(),
            payload,
        ));
    }

    append_job_event(
        execution.job_id.as_str(),
        "execute_mode_selected",
        Some(json!({
            "mode": "ai",
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    let system_prompt = {
        let mut guard = execution
            .ctx
            .catalog
            .lock()
            .map_err(|_| "catalog lock poisoned".to_string())?;
        build_system_prompt(
            &execution.resolved.agent,
            &execution.resolved.used_skills,
            execution.resolved.command.as_ref(),
            &mut guard,
            &allow_policy,
            execution.ctx.workspace_root.as_path(),
        )
    };

    append_job_event(
        execution.job_id.as_str(),
        "system_prompt_ready",
        Some(json!({
            "chars": system_prompt.chars().count(),
            "preview": truncate_for_event(system_prompt.as_str(), 2_000),
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    append_job_event(
        execution.job_id.as_str(),
        "ai_start",
        Some(json!({
            "requested_model": requested_model.clone(),
            "timeout_ms": execution.ctx.ai_timeout_ms,
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    let ai = {
        let ctx = execution.ctx.clone();
        let task = execution.task.clone();
        let requested = requested_model
            .as_deref()
            .map(|value| value.trim().to_string());
        let prompt = system_prompt.clone();
        let allow_policy = allow_policy.clone();
        let job_id = execution.job_id.clone();
        let session_id = execution.session_id.clone();
        let run_id = execution.run_id.clone();

        block_on_result(async move {
            let model = resolve_model_config(ctx.user_id.clone(), requested).await?;
            append_job_event(
                job_id.as_str(),
                "ai_model_resolved",
                Some(json!({
                    "model_id": model.id.clone(),
                    "model_name": model.name.clone(),
                    "provider": model.provider.clone(),
                    "model": model.model.clone(),
                    "supports_responses": model.supports_responses,
                })),
                session_id.as_str(),
                run_id.as_str(),
            );
            if model.api_key.trim().is_empty() {
                append_job_event(
                    job_id.as_str(),
                    "ai_model_missing_key",
                    None,
                    session_id.as_str(),
                    run_id.as_str(),
                );
                return Err(
                    "No usable AI API key found in model configs or OPENAI_API_KEY".to_string(),
                );
            }

            let mcp_selection = resolve_effective_mcp_selection(ctx.user_id.clone())
                .await
                .unwrap_or(EffectiveMcpSelection {
                    configured: false,
                    ids: Vec::new(),
                });
            append_job_event(
                job_id.as_str(),
                "ai_mcp_selection",
                Some(json!({
                    "configured": mcp_selection.configured,
                    "enabled_mcp_ids": mcp_selection.ids,
                })),
                session_id.as_str(),
                run_id.as_str(),
            );

            let workspace_dir = ctx.workspace_root.to_string_lossy().to_string();
            let mcp_ids = mcp_selection.ids.clone();

            let (http_servers, mut stdio_servers, builtin_servers) =
                if mcp_selection.configured && mcp_ids.is_empty() {
                    (Vec::new(), Vec::new(), Vec::new())
                } else {
                    load_mcp_configs_for_user(
                        ctx.user_id.clone(),
                        if mcp_ids.is_empty() {
                            None
                        } else {
                            Some(mcp_ids.clone())
                        },
                        if workspace_dir.trim().is_empty() {
                            None
                        } else {
                            Some(workspace_dir.as_str())
                        },
                        ctx.project_id.as_deref(),
                    )
                    .await
                    .unwrap_or((Vec::new(), Vec::new(), Vec::new()))
                };
            append_job_event(
                job_id.as_str(),
                "ai_mcp_configs_loaded",
                Some(json!({
                    "http": http_servers.len(),
                    "stdio": stdio_servers.len(),
                    "builtin": builtin_servers.len(),
                    "workspace_dir": workspace_dir,
                })),
                session_id.as_str(),
                run_id.as_str(),
            );

            if !workspace_dir.trim().is_empty() {
                for server in stdio_servers.iter_mut() {
                    if server
                        .cwd
                        .as_ref()
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                    {
                        server.cwd = Some(workspace_dir.clone());
                    }
                }
            }

            let effective_settings = get_effective_user_settings(ctx.user_id.clone())
                .await
                .unwrap_or_else(|_| json!({}));
            let max_tokens = effective_settings
                .get("CHAT_MAX_TOKENS")
                .and_then(|value| value.as_i64())
                .filter(|value| *value > 0);
            let setting_keys = effective_settings
                .as_object()
                .map(|map| map.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            append_job_event(
                job_id.as_str(),
                "ai_effective_settings",
                Some(json!({
                    "max_tokens": max_tokens,
                    "setting_keys": setting_keys,
                })),
                session_id.as_str(),
                run_id.as_str(),
            );

            let chunk_buffer = Arc::new(Mutex::new(String::new()));
            let thinking_buffer = Arc::new(Mutex::new(String::new()));

            let on_chunk = {
                let chunk_buffer = chunk_buffer.clone();
                Arc::new(move |chunk: String| {
                    if chunk.trim().is_empty() {
                        return;
                    }
                    if let Ok(mut guard) = chunk_buffer.lock() {
                        guard.push_str(chunk.as_str());
                        if guard.chars().count() > 24_000 {
                            let trimmed = guard
                                .chars()
                                .rev()
                                .take(24_000)
                                .collect::<String>()
                                .chars()
                                .rev()
                                .collect::<String>();
                            *guard = trimmed;
                        }
                    }
                })
            };

            let on_thinking = {
                let thinking_buffer = thinking_buffer.clone();
                Arc::new(move |chunk: String| {
                    if chunk.trim().is_empty() {
                        return;
                    }
                    if let Ok(mut guard) = thinking_buffer.lock() {
                        guard.push_str(chunk.as_str());
                        if guard.chars().count() > 24_000 {
                            let trimmed = guard
                                .chars()
                                .rev()
                                .take(24_000)
                                .collect::<String>()
                                .chars()
                                .rev()
                                .collect::<String>();
                            *guard = trimmed;
                        }
                    }
                })
            };

            let on_tools_start = {
                let job_id = job_id.clone();
                let session_id = session_id.clone();
                let run_id = run_id.clone();
                Arc::new(move |tool_calls: Value| {
                    append_job_event(
                        job_id.as_str(),
                        "ai_tools_start",
                        Some(json!({
                            "tool_calls": summarize_tool_calls_for_event(&tool_calls),
                        })),
                        session_id.as_str(),
                        run_id.as_str(),
                    );
                })
            };

            let on_tools_stream = {
                let job_id = job_id.clone();
                let session_id = session_id.clone();
                let run_id = run_id.clone();
                Arc::new(move |result: Value| {
                    append_job_event(
                        job_id.as_str(),
                        "ai_tools_stream",
                        Some(summarize_single_tool_result_for_event(&result)),
                        session_id.as_str(),
                        run_id.as_str(),
                    );
                })
            };

            let on_tools_end = {
                let job_id = job_id.clone();
                let session_id = session_id.clone();
                let run_id = run_id.clone();
                Arc::new(move |result: Value| {
                    append_job_event(
                        job_id.as_str(),
                        "ai_tools_end",
                        Some(summarize_tool_results_for_event(&result)),
                        session_id.as_str(),
                        run_id.as_str(),
                    );
                })
            };

            let api_mode = if model.supports_responses {
                "responses"
            } else {
                "chat_completions"
            };
            append_job_event(
                job_id.as_str(),
                "ai_api_mode_selected",
                Some(json!({
                    "api_mode": api_mode,
                    "supports_responses": model.supports_responses,
                })),
                session_id.as_str(),
                run_id.as_str(),
            );

            let response = if model.supports_responses {
                let mut mcp_execute = McpToolExecute::new(
                    http_servers.clone(),
                    stdio_servers.clone(),
                    builtin_servers.clone(),
                );

                if !http_servers.is_empty()
                    || !stdio_servers.is_empty()
                    || !builtin_servers.is_empty()
                {
                    if let Err(err) = mcp_execute.init().await {
                        append_job_event(
                            job_id.as_str(),
                            "ai_mcp_init_error",
                            Some(json!({ "error": err, "api_mode": api_mode })),
                            session_id.as_str(),
                            run_id.as_str(),
                        );
                    }
                }

                let (tools_before_filter, tools_after_filter) = if allow_policy.configured {
                    filter_tools_by_prefixes(&mut mcp_execute, &allow_policy.prefixes)
                } else {
                    let count = mcp_execute.tools.len();
                    (count, count)
                };

                append_job_event(
                    job_id.as_str(),
                    "ai_mcp_ready",
                    Some(json!({
                        "api_mode": api_mode,
                        "supports_responses": model.supports_responses,
                        "configured": mcp_selection.configured,
                        "enabled_mcp_ids": mcp_ids,
                        "allow_prefixes": allow_policy.prefixes,
                        "servers": {
                            "http": http_servers.len(),
                            "stdio": stdio_servers.len(),
                            "builtin": builtin_servers.len(),
                        },
                        "tools_before_filter": tools_before_filter,
                        "tools_after_filter": tools_after_filter,
                    })),
                    session_id.as_str(),
                    run_id.as_str(),
                );

                let message_manager = MessageManager::new();
                let handler = AiRequestHandler::new(
                    model.api_key.clone(),
                    model.base_url.clone(),
                    message_manager.clone(),
                );
                let mut ai_client = AiClient::new(handler, mcp_execute, message_manager);
                apply_settings_to_ai_client(&mut ai_client, &effective_settings);

                let messages = vec![json!({
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": task }
                    ]
                })];

                let req = ai_client.process_request(
                    messages,
                    Some(session_id.clone()),
                    ProcessOptions {
                        model: Some(model.model.clone()),
                        provider: Some(model.provider.clone()),
                        thinking_level: model.thinking_level.clone(),
                        temperature: Some(0.7),
                        max_tokens,
                        reasoning_enabled: Some(true),
                        system_prompt: Some(prompt.clone()),
                        history_limit: None,
                        purpose: Some("sub_agent_router".to_string()),
                        callbacks: Some(AiClientCallbacks {
                            on_chunk: Some(on_chunk.clone()),
                            on_thinking: Some(on_thinking.clone()),
                            on_tools_start: Some(on_tools_start.clone()),
                            on_tools_stream: Some(on_tools_stream.clone()),
                            on_tools_end: Some(on_tools_end.clone()),
                        }),
                    },
                );
                append_job_event(
                    job_id.as_str(),
                    "ai_request_dispatch",
                    Some(json!({
                        "api_mode": api_mode,
                        "provider": model.provider.clone(),
                        "model": model.model.clone(),
                    })),
                    session_id.as_str(),
                    run_id.as_str(),
                );

                match timeout(Duration::from_millis(ctx.ai_timeout_ms as u64), req).await {
                    Ok(result) => result?,
                    Err(_) => {
                        append_job_event(
                            job_id.as_str(),
                            "ai_timeout",
                            Some(json!({
                                "api_mode": api_mode,
                                "timeout_ms": ctx.ai_timeout_ms,
                            })),
                            session_id.as_str(),
                            run_id.as_str(),
                        );
                        return Err(format!("AI timeout after {} ms", ctx.ai_timeout_ms));
                    }
                }
            } else {
                let mut mcp_execute = LegacyMcpToolExecute::new(
                    http_servers.clone(),
                    stdio_servers.clone(),
                    builtin_servers.clone(),
                );

                if !http_servers.is_empty()
                    || !stdio_servers.is_empty()
                    || !builtin_servers.is_empty()
                {
                    if let Err(err) = mcp_execute.init().await {
                        append_job_event(
                            job_id.as_str(),
                            "ai_mcp_init_error",
                            Some(json!({ "error": err, "api_mode": api_mode })),
                            session_id.as_str(),
                            run_id.as_str(),
                        );
                    }
                }

                let (tools_before_filter, tools_after_filter) = if allow_policy.configured {
                    filter_legacy_tools_by_prefixes(&mut mcp_execute, &allow_policy.prefixes)
                } else {
                    let count = mcp_execute.tools.len();
                    (count, count)
                };
                let use_tools = !mcp_execute.tools.is_empty();

                append_job_event(
                    job_id.as_str(),
                    "ai_mcp_ready",
                    Some(json!({
                        "api_mode": api_mode,
                        "supports_responses": model.supports_responses,
                        "configured": mcp_selection.configured,
                        "enabled_mcp_ids": mcp_ids,
                        "allow_prefixes": allow_policy.prefixes,
                        "servers": {
                            "http": http_servers.len(),
                            "stdio": stdio_servers.len(),
                            "builtin": builtin_servers.len(),
                        },
                        "tools_before_filter": tools_before_filter,
                        "tools_after_filter": tools_after_filter,
                    })),
                    session_id.as_str(),
                    run_id.as_str(),
                );

                let message_manager = LegacyMessageManager::new();
                let handler = LegacyAiRequestHandler::new(
                    model.api_key.clone(),
                    model.base_url.clone(),
                    message_manager.clone(),
                );
                let mut ai_client = LegacyAiClient::new(handler, mcp_execute, message_manager);
                apply_settings_to_ai_client(&mut ai_client, &effective_settings);
                ai_client.set_system_prompt(Some(prompt.clone()));

                let messages = vec![json!({
                    "role": "user",
                    "content": task,
                })];

                let req = ai_client.process_request(
                    messages,
                    Some(session_id.clone()),
                    model.model.clone(),
                    0.7,
                    max_tokens,
                    use_tools,
                    LegacyAiClientCallbacks {
                        on_chunk: Some(on_chunk.clone()),
                        on_thinking: Some(on_thinking.clone()),
                        on_tools_start: Some(on_tools_start.clone()),
                        on_tools_stream: Some(on_tools_stream.clone()),
                        on_tools_end: Some(on_tools_end.clone()),
                        on_context_summarized_start: None,
                        on_context_summarized_stream: None,
                        on_context_summarized_end: None,
                    },
                    true,
                    Some(model.provider.clone()),
                    model.thinking_level.clone(),
                );
                append_job_event(
                    job_id.as_str(),
                    "ai_request_dispatch",
                    Some(json!({
                        "api_mode": api_mode,
                        "provider": model.provider.clone(),
                        "model": model.model.clone(),
                        "use_tools": use_tools,
                    })),
                    session_id.as_str(),
                    run_id.as_str(),
                );

                match timeout(Duration::from_millis(ctx.ai_timeout_ms as u64), req).await {
                    Ok(result) => result?,
                    Err(_) => {
                        append_job_event(
                            job_id.as_str(),
                            "ai_timeout",
                            Some(json!({
                                "api_mode": api_mode,
                                "timeout_ms": ctx.ai_timeout_ms,
                            })),
                            session_id.as_str(),
                            run_id.as_str(),
                        );
                        return Err(format!("AI timeout after {} ms", ctx.ai_timeout_ms));
                    }
                }
            };

            append_job_event(
                job_id.as_str(),
                "ai_response_received",
                Some(json!({
                    "api_mode": api_mode,
                    "finish_reason": response
                        .get("finish_reason")
                        .and_then(|value| value.as_str()),
                    "content_preview": truncate_for_event(
                        response
                            .get("content")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default(),
                        2_000,
                    ),
                    "reasoning_preview": truncate_for_event(
                        response
                            .get("reasoning")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default(),
                        2_000,
                    ),
                })),
                session_id.as_str(),
                run_id.as_str(),
            );

            let mut content = response
                .get("content")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_default();
            let mut content_source = "response".to_string();

            if content.is_empty() {
                if let Ok(guard) = chunk_buffer.lock() {
                    let fallback = guard.trim();
                    if !fallback.is_empty() {
                        content = fallback.to_string();
                        content_source = "chunk_buffer".to_string();
                    }
                }
            }

            if content.is_empty() {
                content = "(empty)".to_string();
                content_source = "empty_placeholder".to_string();
            }
            append_job_event(
                job_id.as_str(),
                "ai_content_ready",
                Some(json!({
                    "source": content_source,
                    "chars": content.chars().count(),
                    "preview": truncate_for_event(content.as_str(), 2_000),
                })),
                session_id.as_str(),
                run_id.as_str(),
            );

            let mut reasoning = response
                .get("reasoning")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let mut reasoning_source = "response".to_string();

            if reasoning.is_none() {
                if let Ok(guard) = thinking_buffer.lock() {
                    let fallback = guard.trim();
                    if !fallback.is_empty() {
                        reasoning = Some(fallback.to_string());
                        reasoning_source = "thinking_buffer".to_string();
                    }
                }
            }
            append_job_event(
                job_id.as_str(),
                "ai_reasoning_ready",
                Some(json!({
                    "source": reasoning_source,
                    "chars": reasoning.as_ref().map(|value| value.chars().count()).unwrap_or(0),
                    "preview": truncate_for_event(
                        reasoning.as_deref().unwrap_or_default(),
                        2_000,
                    ),
                })),
                session_id.as_str(),
                run_id.as_str(),
            );

            let finish_reason = response
                .get("finish_reason")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());

            Ok(AiTaskResult {
                response: content,
                reasoning,
                finish_reason,
                model_id: model.id,
                model_name: model.name,
                provider: model.provider,
                model: model.model,
            })
        })
    }?;

    append_job_event(
        execution.job_id.as_str(),
        "ai_finish",
        Some(json!({
            "model_id": ai.model_id,
            "model_name": ai.model_name,
            "provider": ai.provider,
            "model": ai.model,
            "finish_reason": ai.finish_reason,
            "reasoning": truncate_for_event(ai.reasoning.as_deref().unwrap_or(""), 12000),
            "response_preview": truncate_for_event(ai.response.as_str(), 6000),
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    let payload = json!({
        "status": "ok",
        "job_id": execution.job_id,
        "agent_id": execution.resolved.agent.id,
        "agent_name": execution.resolved.agent.name,
        "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
        "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        "reason": execution.resolved.reason,
        "response": ai.response,
        "reasoning": ai.reasoning,
        "finish_reason": ai.finish_reason,
        "model_id": ai.model_id,
        "model_name": ai.model_name,
        "provider": ai.provider,
        "model": ai.model,
    });

    Ok(("ok".to_string(), payload))
}
