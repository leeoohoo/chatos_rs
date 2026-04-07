use std::time::Duration;

use dashmap::DashSet;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::api::chat_stream_common::{
    build_prefixed_input_items, resolve_chat_stream_context, ChatStreamRequest,
};
use crate::config::Config;
use crate::core::ai_model_config::resolve_chat_model_config;
use crate::core::ai_settings::chat_max_tokens_from_settings;
use crate::core::messages::MessageOut;
use crate::models::ai_model_config::AiModelConfig;
use crate::models::message::Message;
use crate::services::builtin_mcp::{BuiltinMcpKind, TASK_EXECUTOR_SERVER_NAME};
use crate::services::contact_agent_model::{
    normalize_optional_model_id, resolve_effective_contact_agent_model_config_id,
};
use crate::services::mcp_loader::McpBuiltinServer;
use crate::services::memory_server_client::{self, TaskExecutionScopeBinding};
use crate::services::session_event_hub::session_event_hub;
use crate::services::task_service_client::{
    self, AckAllDoneRequestDto, SchedulerRequestDto, TaskExecutionScopeDto, TaskRecordDto,
    UpdateTaskRequestDto,
};
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v3::ai_server::{AiServer, ChatOptions};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::services::v3::message_manager::MessageManager;

static ACTIVE_SCOPE_JOBS: Lazy<DashSet<String>> = Lazy::new(DashSet::new);

pub fn start() {
    if !Config::get().task_scheduler_enabled {
        info!("[TASK-RUNNER] disabled by config");
        return;
    }

    tokio::spawn(async move {
        let interval_secs = Config::get().task_scheduler_interval_secs.max(1) as u64;
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            dispatch_tick().await;
        }
    });
}

async fn dispatch_tick() {
    let limit = Some(Config::get().task_scheduler_scope_limit.max(1));
    let scopes = match task_service_client::list_scheduler_scopes(None, limit).await {
        Ok(items) => items,
        Err(err) => {
            warn!("[TASK-RUNNER] list scopes failed: {}", err);
            return;
        }
    };

    for scope in scopes {
        if ACTIVE_SCOPE_JOBS.insert(scope.scope_key.clone()) {
            tokio::spawn(async move {
                let scope_key = scope.scope_key.clone();
                let result = memory_server_client::with_internal_scope(process_scope(scope)).await;
                if let Err(err) = result {
                    warn!("[TASK-RUNNER] scope processing failed: {}", err);
                }
                ACTIVE_SCOPE_JOBS.remove(&scope_key);
            });
        }
    }
}

async fn process_scope(scope: TaskExecutionScopeDto) -> Result<(), String> {
    let decision = task_service_client::scheduler_next(&SchedulerRequestDto {
        user_id: Some(scope.user_id.clone()),
        contact_agent_id: scope.contact_agent_id.clone(),
        project_id: scope.project_id.clone(),
    })
    .await?;

    match decision.decision.as_str() {
        "task" => {
            let task = decision
                .task
                .ok_or_else(|| format!("scope {} missing task payload", scope.scope_key))?;
            execute_task(scope, task).await
        }
        "all_done" => handle_all_done(scope).await,
        "pass" => Ok(()),
        other => Err(format!(
            "scope {} returned unsupported decision {}",
            scope.scope_key, other
        )),
    }
}

async fn execute_task(scope: TaskExecutionScopeDto, task: TaskRecordDto) -> Result<(), String> {
    info!(
        "[TASK-RUNNER] execute task start: scope={} task_id={} session_id={}",
        scope.scope_key,
        task.id,
        task.session_id.as_deref().unwrap_or("")
    );

    let mut task_runtime = match build_task_runtime(scope.clone(), Some(&task)).await {
        Ok(runtime) => runtime,
        Err(err) => return fail_task(scope, task, err.as_str()).await,
    };
    let result = task_runtime
        .ai_server
        .chat(
            task_runtime.runtime_session_key.as_str(),
            task.content.as_str(),
            task_runtime.chat_options(task.id.as_str(), Some(&task)),
        )
        .await;

    match result {
        Ok(payload) => {
            let final_text = payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let saved_notice = match save_task_notice_message(
                task.session_id.as_deref(),
                "task_execution_notice",
                "completed",
                &scope,
                Some(&task),
                if final_text.is_empty() {
                    format!("任务“{}”已完成。", task.title)
                } else {
                    format!("任务“{}”已完成。\n\n{}", task.title, final_text)
                },
            )
            .await
            {
                Ok(message) => message,
                Err(err) => {
                    warn!(
                        "[TASK-RUNNER] save completion notice failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, err
                    );
                    None
                }
            };

            let result_summary = compact_result_summary(
                if final_text.is_empty() {
                    format!("任务“{}”已完成", task.title)
                } else {
                    final_text.clone()
                }
                .as_str(),
            );
            let updated_task = match task_service_client::update_task_internal(
                task.id.as_str(),
                &UpdateTaskRequestDto {
                    status: Some("completed".to_string()),
                    result_summary: Some(Some(result_summary.clone())),
                    result_message_id: Some(
                        saved_notice
                            .as_ref()
                            .map(|m| Some(m.id.clone()))
                            .unwrap_or(None),
                    ),
                    last_error: Some(None),
                    ..UpdateTaskRequestDto::default()
                },
            )
            .await
            {
                Ok(task) => task,
                Err(err) => {
                    warn!(
                        "[TASK-RUNNER] update completed status failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, err
                    );
                    None
                }
            };
            if let Some(updated_task) = updated_task.as_ref() {
                if let Err(err) = sync_task_result_brief(
                    &scope,
                    updated_task,
                    "completed",
                    result_summary.as_str(),
                    saved_notice.as_ref().map(|item| item.id.as_str()),
                )
                .await
                {
                    warn!(
                        "[TASK-RUNNER] sync completion brief failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, err
                    );
                }
            } else {
                warn!(
                    "[TASK-RUNNER] update completed status failed: scope={} task_id={} error={}",
                    scope.scope_key, task.id, "task not returned after update"
                );
            }
            info!(
                "[TASK-RUNNER] execute task completed: scope={} task_id={}",
                scope.scope_key, task.id
            );
            Ok(())
        }
        Err(err) => {
            if let Err(notice_err) = save_task_notice_message(
                task.session_id.as_deref(),
                "task_execution_notice",
                "failed",
                &scope,
                Some(&task),
                format!("任务“{}”执行失败：{}", task.title, err),
            )
            .await
            {
                warn!(
                    "[TASK-RUNNER] save failure notice failed: scope={} task_id={} error={}",
                    scope.scope_key, task.id, notice_err
                );
            }
            let updated_task = match task_service_client::update_task_internal(
                task.id.as_str(),
                &UpdateTaskRequestDto {
                    status: Some("failed".to_string()),
                    result_summary: Some(Some(compact_result_summary(err.as_str()))),
                    result_message_id: Some(None),
                    last_error: Some(Some(err.clone())),
                    ..UpdateTaskRequestDto::default()
                },
            )
            .await
            {
                Ok(task) => task,
                Err(update_err) => {
                    warn!(
                        "[TASK-RUNNER] update failed status failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, update_err
                    );
                    None
                }
            };
            if let Some(updated_task) = updated_task.as_ref() {
                if let Err(bridge_err) =
                    sync_task_result_brief(&scope, updated_task, "failed", err.as_str(), None).await
                {
                    warn!(
                        "[TASK-RUNNER] sync failure brief failed: scope={} task_id={} error={}",
                        scope.scope_key, task.id, bridge_err
                    );
                }
            } else {
                warn!(
                    "[TASK-RUNNER] update failed status failed: scope={} task_id={} error={}",
                    scope.scope_key, task.id, "task not returned after update"
                );
            }
            Err(format!(
                "scope {} task {} execution failed: {}",
                scope.scope_key, task.id, err
            ))
        }
    }
}

async fn fail_task(
    scope: TaskExecutionScopeDto,
    task: TaskRecordDto,
    err: &str,
) -> Result<(), String> {
    if let Err(notice_err) = save_task_notice_message(
        task.session_id.as_deref(),
        "task_execution_notice",
        "failed",
        &scope,
        Some(&task),
        format!("任务“{}”执行失败：{}", task.title, err),
    )
    .await
    {
        warn!(
            "[TASK-RUNNER] save setup-failure notice failed: scope={} task_id={} error={}",
            scope.scope_key, task.id, notice_err
        );
    }
    let updated_task = match task_service_client::update_task_internal(
        task.id.as_str(),
        &UpdateTaskRequestDto {
            status: Some("failed".to_string()),
            result_summary: Some(Some(compact_result_summary(err))),
            result_message_id: Some(None),
            last_error: Some(Some(err.to_string())),
            ..UpdateTaskRequestDto::default()
        },
    )
    .await
    {
        Ok(task) => task,
        Err(update_err) => {
            warn!(
                "[TASK-RUNNER] update setup-failure status failed: scope={} task_id={} error={}",
                scope.scope_key, task.id, update_err
            );
            None
        }
    };
    if let Some(updated_task) = updated_task.as_ref() {
        if let Err(bridge_err) =
            sync_task_result_brief(&scope, updated_task, "failed", err, None).await
        {
            warn!(
                "[TASK-RUNNER] sync setup-failure brief failed: scope={} task_id={} error={}",
                scope.scope_key, task.id, bridge_err
            );
        }
    } else {
        warn!(
            "[TASK-RUNNER] update setup-failure status failed: scope={} task_id={} error={}",
            scope.scope_key, task.id, "task not returned after update"
        );
    }
    Err(format!(
        "scope {} task {} execution failed: {}",
        scope.scope_key, task.id, err
    ))
}

async fn handle_all_done(scope: TaskExecutionScopeDto) -> Result<(), String> {
    let Some(session_id) = scope
        .latest_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
    else {
        task_service_client::ack_all_done(&AckAllDoneRequestDto {
            user_id: Some(scope.user_id.clone()),
            contact_agent_id: scope.contact_agent_id.clone(),
            project_id: scope.project_id.clone(),
            ack_at: None,
        })
        .await?;
        return Ok(());
    };

    let mut task_runtime = build_task_runtime(scope.clone(), None).await?;
    let summary_prompt = "当前这个联系人的后台任务都已经执行完成。请基于已有任务执行记录，给用户一段简短、自然的结语：说明任务已全部完成，并概括最终结果；不要输出过程推理，不要编造未完成事项。";
    let result = task_runtime
        .ai_server
        .chat(
            task_runtime.runtime_session_key.as_str(),
            summary_prompt,
            task_runtime.chat_options("all_done", None),
        )
        .await?;
    let final_text = result
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("后台任务已全部执行完成。")
        .trim()
        .to_string();

    if let Err(err) = save_task_notice_message(
        Some(session_id.as_str()),
        "task_execution_notice",
        "all_done",
        &scope,
        None,
        if final_text.is_empty() {
            "后台任务已全部执行完成。".to_string()
        } else {
            final_text
        },
    )
    .await
    {
        warn!(
            "[TASK-RUNNER] save all-done notice failed: scope={} error={}",
            scope.scope_key, err
        );
    }

    task_service_client::ack_all_done(&AckAllDoneRequestDto {
        user_id: Some(scope.user_id.clone()),
        contact_agent_id: scope.contact_agent_id.clone(),
        project_id: scope.project_id.clone(),
        ack_at: None,
    })
    .await?;
    Ok(())
}

struct PreparedTaskRuntime {
    ai_server: AiServer,
    model_runtime: crate::core::ai_model_config::ResolvedChatModelConfig,
    runtime_context: crate::api::chat_stream_common::ResolvedChatStreamContext,
    runtime_session_key: String,
    max_tokens: Option<i64>,
}

impl PreparedTaskRuntime {
    fn chat_options(&self, turn_suffix: &str, task: Option<&TaskRecordDto>) -> ChatOptions {
        ChatOptions {
            model: Some(self.model_runtime.model.clone()),
            provider: Some(self.model_runtime.provider.clone()),
            thinking_level: self.model_runtime.thinking_level.clone(),
            supports_responses: Some(self.model_runtime.supports_responses),
            temperature: Some(self.model_runtime.temperature),
            max_tokens: self.max_tokens,
            use_tools: Some(self.runtime_context.use_tools),
            attachments: Some(Vec::new()),
            supports_images: Some(self.model_runtime.supports_images),
            reasoning_enabled: Some(self.model_runtime.effective_reasoning),
            callbacks: None,
            turn_id: Some(format!("task-exec-{}", turn_suffix)),
            user_message_id: None,
            message_mode: Some("model".to_string()),
            message_source: Some(self.model_runtime.model.clone()),
            prefixed_input_items: build_task_execution_prefixed_input_items(
                &self.runtime_context,
                task,
            ),
            request_cwd: if self.model_runtime.use_codex_gateway_mcp_passthrough {
                self.runtime_context.resolved_project_root.clone()
            } else {
                None
            },
            use_codex_gateway_mcp_passthrough: Some(
                self.model_runtime.use_codex_gateway_mcp_passthrough,
            ),
        }
    }
}

async fn build_task_runtime(
    scope: TaskExecutionScopeDto,
    task: Option<&TaskRecordDto>,
) -> Result<PreparedTaskRuntime, String> {
    validate_task_execution_grants(scope.clone(), task).await?;
    let model_config = resolve_execution_model(scope.clone(), task).await?;
    let model_config_json = json!({
        "model_name": model_config.model,
        "provider": model_config.provider,
        "thinking_level": model_config.thinking_level,
        "api_key": model_config.api_key,
        "base_url": model_config.base_url,
        "supports_images": model_config.supports_images,
        "supports_reasoning": model_config.supports_reasoning,
        "supports_responses": model_config.supports_responses,
    });
    let cfg = Config::get();
    let model_runtime = resolve_chat_model_config(
        &model_config_json,
        "gpt-4o",
        &cfg.openai_api_key,
        &cfg.openai_base_url,
        None,
        true,
    );

    let binding = TaskExecutionScopeBinding {
        user_id: scope.user_id.clone(),
        contact_agent_id: scope.contact_agent_id.clone(),
        project_id: scope.project_id.clone(),
        task_id: task.map(|item| item.id.clone()),
        source_session_id: task
            .and_then(|item| item.session_id.clone())
            .or_else(|| scope.latest_session_id.clone()),
    };
    let mut ai_server = AiServer::new_with_message_manager(
        model_runtime.api_key.clone(),
        model_runtime.base_url.clone(),
        model_runtime.model.clone(),
        model_runtime.temperature,
        McpToolExecute::new(Vec::new(), Vec::new(), Vec::new()),
        MessageManager::new_task_execution(binding),
    );

    let request = ChatStreamRequest {
        session_id: task
            .and_then(|item| item.session_id.clone())
            .or_else(|| scope.latest_session_id.clone()),
        content: Some(task.map(|item| item.content.clone()).unwrap_or_default()),
        ai_model_config: None,
        user_id: Some(scope.user_id.clone()),
        attachments: None,
        reasoning_enabled: Some(model_runtime.effective_reasoning),
        turn_id: None,
        contact_agent_id: Some(scope.contact_agent_id.clone()),
        project_id: Some(scope.project_id.clone()),
        project_root: task.and_then(|item| item.project_root.clone()),
        remote_connection_id: task.and_then(|item| item.remote_connection_id.clone()),
        mcp_enabled: Some(true),
        enabled_mcp_ids: Some(
            task.map(|item| item.planned_builtin_mcp_ids.clone())
                .unwrap_or_default(),
        ),
        execution_context: Some(true),
    };
    let runtime_context = resolve_chat_stream_context(
        request.session_id.as_deref().unwrap_or(""),
        request.content.as_deref().unwrap_or(""),
        &request,
        model_runtime.system_prompt.clone(),
        model_runtime.use_active_system_context,
    )
    .await;
    if runtime_context.base_system_prompt.is_some() {
        ai_server.set_system_prompt(runtime_context.base_system_prompt.clone());
    }

    let execution_asset_context =
        build_task_execution_asset_context(scope.contact_agent_id.as_str(), task).await?;

    let (http_servers, stdio_servers, mut builtin_servers) =
        runtime_context.mcp_server_bundle.clone();
    if let Some(task) = task {
        builtin_servers.push(McpBuiltinServer {
            name: TASK_EXECUTOR_SERVER_NAME.to_string(),
            kind: BuiltinMcpKind::TaskExecutor,
            workspace_dir: String::new(),
            user_id: Some(scope.user_id.clone()),
            project_id: Some(scope.project_id.clone()),
            remote_connection_id: None,
            contact_agent_id: Some(scope.contact_agent_id.clone()),
            current_task_id: Some(task.id.clone()),
            allow_writes: false,
            max_file_bytes: 0,
            max_write_bytes: 0,
            search_limit: 0,
        });
    }
    let mut mcp_exec = McpToolExecute::new(
        http_servers.clone(),
        stdio_servers.clone(),
        builtin_servers.clone(),
    );
    if runtime_context.use_tools {
        let init_result = if model_runtime.use_codex_gateway_mcp_passthrough {
            mcp_exec.init_builtin_only().await
        } else {
            mcp_exec.init().await
        };
        if let Err(err) = init_result {
            warn!(
                "[TASK-RUNNER] init tools failed for scope={}: {}",
                scope.scope_key, err
            );
        }
    }
    ai_server.set_mcp_tool_execute(mcp_exec);

    let effective_settings = get_effective_user_settings(Some(scope.user_id.clone()))
        .await
        .unwrap_or_else(|_| json!({}));
    apply_settings_to_ai_client(&mut ai_server.ai_client, &effective_settings);
    let max_tokens = chat_max_tokens_from_settings(&effective_settings);

    Ok(PreparedTaskRuntime {
        ai_server,
        model_runtime,
        runtime_context: crate::api::chat_stream_common::ResolvedChatStreamContext {
            contact_system_prompt: merge_execution_prompts(
                runtime_context.contact_system_prompt.as_deref(),
                execution_asset_context.as_deref(),
            ),
            ..runtime_context
        },
        runtime_session_key: format!("task-exec-scope:{}", scope.scope_key),
        max_tokens,
    })
}

async fn validate_task_execution_grants(
    scope: TaskExecutionScopeDto,
    task: Option<&TaskRecordDto>,
) -> Result<(), String> {
    let Some(task) = task else {
        return Ok(());
    };

    let contacts =
        memory_server_client::list_memory_contacts(Some(scope.user_id.as_str()), Some(500), 0)
            .await?;
    let authorized_builtin_mcp_ids = contacts
        .into_iter()
        .find(|contact| contact.agent_id.trim() == scope.contact_agent_id.trim())
        .map(|contact| contact.authorized_builtin_mcp_ids)
        .unwrap_or_default();

    let unauthorized = task
        .planned_builtin_mcp_ids
        .iter()
        .filter(|item| {
            !authorized_builtin_mcp_ids
                .iter()
                .any(|allowed| allowed == *item)
        })
        .cloned()
        .collect::<Vec<_>>();

    if unauthorized.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "task {} contains builtin MCP ids no longer authorized for contact {}: {}",
            task.id,
            scope.contact_agent_id,
            unauthorized.join(", ")
        ))
    }
}

fn build_task_execution_prefixed_input_items(
    runtime_context: &crate::api::chat_stream_common::ResolvedChatStreamContext,
    task: Option<&TaskRecordDto>,
) -> Option<Vec<Value>> {
    let mut items = build_prefixed_input_items(
        runtime_context.contact_system_prompt.as_deref(),
        runtime_context.command_system_prompt.as_deref(),
    )
    .unwrap_or_default();

    if let Some(summary) = runtime_context
        .memory_summary_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        items.push(system_input_item(
            format!("历史上下文总结：\n{}", summary).as_str(),
        ));
    }

    if let Some(task) = task {
        let mut lines = vec![
            "当前处于后台任务执行阶段。".to_string(),
            format!("task_id={}", task.id),
            format!("任务标题={}", task.title),
            format!("任务状态={}", task.status),
        ];
        if let Some(snapshot) = task.planning_snapshot.as_ref() {
            if let Some(source_goal) = snapshot
                .source_user_goal_summary
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                lines.push("来源用户目标摘要:".to_string());
                lines.push(source_goal.to_string());
            }
            if let Some(source_constraints) = snapshot
                .source_constraints_summary
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                lines.push("来源约束摘要:".to_string());
                lines.push(source_constraints.to_string());
            }
        }
        if !task.planned_builtin_mcp_ids.is_empty() {
            lines.push(format!(
                "本次任务允许使用的内置 MCP={}",
                task.planned_builtin_mcp_ids.join(", ")
            ));
        }
        if let Some(contract) = task.execution_result_contract.as_ref() {
            lines.push(format!(
                "结果必填={}",
                if contract.result_required {
                    "true"
                } else {
                    "false"
                }
            ));
            if let Some(format) = contract
                .preferred_format
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                lines.push(format!("结果格式偏好={}", format));
            }
        }
        lines.push("执行完成后，应输出明确结果；如失败，也必须说明失败结果。".to_string());
        items.push(system_input_item(lines.join("\n").as_str()));
    }

    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

fn system_input_item(text: &str) -> Value {
    json!({
        "type": "message",
        "role": "system",
        "content": [{ "type": "input_text", "text": text }],
    })
}

fn merge_execution_prompts(base: Option<&str>, extra: Option<&str>) -> Option<String> {
    match (
        base.map(str::trim).filter(|value| !value.is_empty()),
        extra.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(base), Some(extra)) => Some(format!("{}\n\n{}", base, extra)),
        (Some(base), None) => Some(base.to_string()),
        (None, Some(extra)) => Some(extra.to_string()),
        (None, None) => None,
    }
}

async fn build_task_execution_asset_context(
    contact_agent_id: &str,
    task: Option<&TaskRecordDto>,
) -> Result<Option<String>, String> {
    let Some(task) = task else {
        return Ok(None);
    };
    if task.planned_context_assets.is_empty() {
        return Ok(None);
    }

    let runtime_context = memory_server_client::get_memory_agent_runtime_context(contact_agent_id)
        .await?
        .ok_or_else(|| format!("agent runtime context not found: {}", contact_agent_id))?;

    let mut sections = Vec::new();
    for asset in &task.planned_context_assets {
        match asset.asset_type.trim().to_ascii_lowercase().as_str() {
            "skill" => {
                let Some(skill) =
                    memory_server_client::get_memory_skill(asset.asset_id.as_str()).await?
                else {
                    continue;
                };
                sections.push(format!(
                    "[技能] {} ({})\n{}",
                    skill.name,
                    skill.id,
                    skill.content.trim()
                ));
            }
            "plugin" => {
                let Some(plugin) =
                    memory_server_client::get_memory_skill_plugin(asset.asset_id.as_str()).await?
                else {
                    continue;
                };
                let mut text = format!(
                    "[插件] {} ({})\n{}",
                    plugin.name,
                    plugin.source,
                    plugin.content.as_deref().map(str::trim).unwrap_or("")
                );
                if !plugin.commands.is_empty() {
                    text.push_str("\n\n插件命令：");
                    for command in &plugin.commands {
                        text.push_str(
                            format!(
                                "\n- {} [{}]\n{}",
                                command.name,
                                command.source_path,
                                command.content.trim()
                            )
                            .as_str(),
                        );
                    }
                }
                sections.push(text);
            }
            "common" => {
                let Some(command) = runtime_context.runtime_commands.iter().find(|item| {
                    item.command_ref.trim() == asset.asset_id.trim()
                        || item.source_path.trim() == asset.asset_id.trim()
                }) else {
                    continue;
                };
                sections.push(format!(
                    "[Common] {} ({})\n{}",
                    command.name,
                    command.command_ref,
                    command.content.trim()
                ));
            }
            _ => {}
        }
    }

    if sections.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!(
            "以下是本次任务明确选中的技能 / 插件 / commons 全文，请严格基于这些内容执行：\n\n{}",
            sections.join("\n\n---\n\n")
        )))
    }
}

async fn resolve_execution_model(
    scope: TaskExecutionScopeDto,
    task: Option<&TaskRecordDto>,
) -> Result<AiModelConfig, String> {
    let agent_model_id =
        resolve_effective_contact_agent_model_config_id(scope.contact_agent_id.as_str()).await?;
    let model_id = normalize_optional_model_id(task.and_then(|item| item.model_config_id.clone()))
        .or(agent_model_id)
        .ok_or_else(|| {
            format!(
                "scope {} missing model_config_id for contact {}",
                scope.scope_key, scope.contact_agent_id
            )
        })?;
    let config = memory_server_client::get_memory_model_config(model_id.as_str())
        .await?
        .ok_or_else(|| format!("model config not found: {}", model_id))?;
    Ok(AiModelConfig {
        id: config.id,
        name: config.name,
        provider: if config.provider.trim().eq_ignore_ascii_case("openai") {
            "gpt".to_string()
        } else {
            config.provider
        },
        model: config.model,
        thinking_level: config.thinking_level,
        api_key: config.api_key,
        base_url: config.base_url,
        user_id: Some(config.user_id),
        enabled: config.enabled == 1,
        supports_images: config.supports_images == 1,
        supports_reasoning: config.supports_reasoning == 1,
        supports_responses: config.supports_responses == 1,
        created_at: config.created_at,
        updated_at: config.updated_at,
    })
}

async fn save_task_notice_message(
    session_id: Option<&str>,
    notice_type: &str,
    event: &str,
    scope: &TaskExecutionScopeDto,
    task: Option<&TaskRecordDto>,
    content: String,
) -> Result<Option<Message>, String> {
    let Some(session_id) = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
    else {
        return Ok(None);
    };

    let message = Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "assistant".to_string(),
        content,
        message_mode: Some("task_notice".to_string()),
        message_source: Some("task_execution_runner".to_string()),
        summary: None,
        tool_calls: None,
        tool_call_id: None,
        reasoning: None,
        metadata: Some(json!({
            "type": notice_type,
            "hidden_from_model": true,
            "task_execution": {
                "event": event,
                "scope_key": scope.scope_key,
                "user_id": scope.user_id,
                "contact_agent_id": scope.contact_agent_id,
                "project_id": scope.project_id,
                "task_id": task.map(|item| item.id.clone()),
                "task_title": task.map(|item| item.title.clone()),
            }
        })),
        created_at: crate::core::time::now_rfc3339(),
    };
    let saved = memory_server_client::upsert_message(&message).await?;
    let payload = json!({
        "type": "task_execution.notice",
        "timestamp": crate::core::time::now_rfc3339(),
        "event": event,
        "session_id": session_id,
        "message": serde_json::to_value(MessageOut::from(saved.clone())).unwrap_or(Value::Null),
        "task": task,
        "scope": scope,
    });
    session_event_hub().publish(session_id.as_str(), payload);
    Ok(Some(saved))
}

async fn sync_task_result_brief(
    scope: &TaskExecutionScopeDto,
    task: &TaskRecordDto,
    task_status: &str,
    result_summary: &str,
    result_message_id: Option<&str>,
) -> Result<(), String> {
    let result_summary = result_summary.trim();
    if result_summary.is_empty() {
        return Ok(());
    }

    memory_server_client::upsert_task_result_brief(
        &memory_server_client::UpsertTaskResultBriefRequestDto {
            task_id: task.id.clone(),
            user_id: scope.user_id.clone(),
            contact_agent_id: scope.contact_agent_id.clone(),
            project_id: scope.project_id.clone(),
            source_session_id: task.session_id.clone(),
            source_turn_id: task.conversation_turn_id.clone(),
            task_title: task.title.clone(),
            task_status: task_status.trim().to_string(),
            result_summary: compact_result_summary(result_summary),
            result_format: task
                .execution_result_contract
                .as_ref()
                .and_then(|item| item.preferred_format.clone()),
            result_message_id: result_message_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string()),
            finished_at: task.finished_at.clone(),
        },
    )
    .await?;
    Ok(())
}

fn compact_result_summary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= 500 {
        return trimmed.to_string();
    }
    let compact: String = trimmed.chars().take(500).collect();
    format!("{}...", compact)
}
