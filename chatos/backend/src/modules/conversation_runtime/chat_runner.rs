// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tracing::warn;

use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::ai_settings::attachment_total_max_bytes_from_settings;
use crate::core::chat_stream::{
    enrich_chat_result_with_persisted_messages, handle_chat_result, send_tools_unavailable_event,
    ChatEventSink, ChatRealtimeStreamContext,
};
use crate::core::messages::set_task_runner_async_overall_status_for_session;
use crate::services::agent_runtime::message_manager::MessageManager;
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::ai_common::{
    build_ai_client_success_payload, build_user_message_metadata,
    TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
};
use crate::utils::abort_registry;
use crate::utils::attachments::Attachment;
use crate::utils::log_helpers::log_chat_begin;
use crate::utils::sse::SseSender;

use super::bootstrap::CommonChatBootstrap;
use super::chat_execution::{effective_codex_gateway_mcp_passthrough, prepare_mcp_execution};
use super::cloud_agent::{start_chatos_cloud_agent, StartChatosCloudAgent};
use super::guidance;
use super::runtime_context::{ResolvedConversationRuntimeContext, ToolMetadataMap};
use super::snapshot::{sync_chat_turn_snapshot, LiveRequestSnapshotContext};
use super::turn_lifecycle::ActiveConversationTurn;

pub struct PreparedChatExecution {
    pub sink: ChatEventSink,
    pub callbacks: AiClientCallbacks,
    pub chunk_sent: Arc<AtomicBool>,
    pub streamed_content: Arc<Mutex<String>>,
    pub mcp_tool_metadata: ToolMetadataMap,
}

pub struct ChatLifecycleConfig<'a> {
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub user_message_id: &'a str,
    pub model_runtime: &'a ResolvedChatModelConfig,
    pub use_tools: bool,
    pub unavailable_tools: &'a [Value],
    pub runtime_context: &'a ResolvedConversationRuntimeContext,
    pub tool_metadata: &'a ToolMetadataMap,
}

pub fn build_live_request_snapshot_context(
    config: &ChatLifecycleConfig<'_>,
) -> LiveRequestSnapshotContext {
    LiveRequestSnapshotContext {
        session_id: config.session_id.to_string(),
        turn_id: config.turn_id.to_string(),
        user_message_id: config.user_message_id.to_string(),
        model: config.model_runtime.model.clone(),
        provider: config.model_runtime.provider.clone(),
        tool_metadata: config.tool_metadata.clone(),
        unavailable_builtin_tools: config.unavailable_tools.to_vec(),
        runtime_context: config.runtime_context.clone(),
    }
}

pub struct BootstrappedChatInput<'a> {
    pub sender: Option<SseSender>,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub session_id: &'a str,
    pub content: &'a str,
    pub persisted_user_message_content: Option<String>,
    pub persisted_user_message_metadata: Option<Value>,
    pub cloud_agent_owner_context: Option<Value>,
    pub model_runtime: &'a ResolvedChatModelConfig,
    pub bootstrap: CommonChatBootstrap,
}

pub struct BootstrappedProjectPlanningInput<'a> {
    pub sender: Option<SseSender>,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub session_id: &'a str,
    pub content: &'a str,
    pub persisted_user_message_content: Option<String>,
    pub persisted_user_message_metadata: Option<Value>,
    pub model_runtime: &'a ResolvedChatModelConfig,
    pub bootstrap: CommonChatBootstrap,
}

pub fn build_chat_event_sink(
    sender: Option<SseSender>,
    user_id: Option<String>,
    session_id: &str,
    conversation_turn_id: Option<String>,
    project_id: Option<String>,
    user_message_id: Option<String>,
) -> ChatEventSink {
    ChatEventSink::new(
        sender,
        Some(ChatRealtimeStreamContext {
            user_id,
            conversation_id: Some(session_id.to_string()),
            conversation_turn_id,
            project_id,
            user_message_id,
        }),
    )
}

pub async fn run_bootstrapped_project_planning(input: BootstrappedProjectPlanningInput<'_>) {
    let BootstrappedProjectPlanningInput {
        sender,
        user_id,
        project_id,
        session_id,
        content,
        persisted_user_message_content,
        persisted_user_message_metadata,
        model_runtime,
        bootstrap,
    } = input;
    let CommonChatBootstrap {
        effective_settings,
        mut runtime_context,
        attachments,
        user_message_id,
        resolved_turn_id,
        ..
    } = bootstrap;
    let sink = build_chat_event_sink(
        sender,
        user_id,
        session_id,
        Some(resolved_turn_id.clone()),
        project_id,
        Some(user_message_id.clone()),
    );
    let empty_chunk_sent = Arc::new(AtomicBool::new(false));
    let empty_streamed_content = Arc::new(Mutex::new(String::new()));

    let user_message_metadata = merge_programmatic_planning_user_message_metadata(
        persisted_user_message_metadata,
        build_user_message_metadata(attachments.as_slice(), Some(resolved_turn_id.as_str())),
    );
    if let Err(error) = MessageManager::new()
        .save_user_message(
            session_id,
            persisted_user_message_content.as_deref().unwrap_or(content),
            Some(user_message_id.clone()),
            Some(TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE.to_string()),
            Some(model_runtime.model.clone()),
            user_message_metadata,
        )
        .await
    {
        close_mcp_management_runtime_session(
            &mut runtime_context,
            session_id,
            resolved_turn_id.as_str(),
        )
        .await;
        finalize_chat_result(
            &sink,
            session_id,
            resolved_turn_id.as_str(),
            user_message_id.as_str(),
            false,
            &empty_chunk_sent,
            &empty_streamed_content,
            Err(format!("保存规划源消息失败：{error}")),
            true,
            || crate::utils::log_helpers::log_chat_cancelled(session_id),
            crate::utils::log_helpers::log_chat_error,
        )
        .await;
        return;
    }

    if let Some(runtime_error) = runtime_context.runtime_error.clone() {
        close_mcp_management_runtime_session(
            &mut runtime_context,
            session_id,
            resolved_turn_id.as_str(),
        )
        .await;
        finalize_chat_result(
            &sink,
            session_id,
            resolved_turn_id.as_str(),
            user_message_id.as_str(),
            true,
            &empty_chunk_sent,
            &empty_streamed_content,
            Err(runtime_error),
            true,
            || crate::utils::log_helpers::log_chat_cancelled(session_id),
            crate::utils::log_helpers::log_chat_error,
        )
        .await;
        return;
    }

    if let Err(attachment_error) =
        validate_attachment_total_size(attachments.as_slice(), &effective_settings)
    {
        close_mcp_management_runtime_session(
            &mut runtime_context,
            session_id,
            resolved_turn_id.as_str(),
        )
        .await;
        finalize_chat_result(
            &sink,
            session_id,
            resolved_turn_id.as_str(),
            user_message_id.as_str(),
            true,
            &empty_chunk_sent,
            &empty_streamed_content,
            Err(attachment_error),
            true,
            || crate::utils::log_helpers::log_chat_cancelled(session_id),
            crate::utils::log_helpers::log_chat_error,
        )
        .await;
        return;
    }

    let prepared_mcp = match prepare_mcp_execution(
        session_id,
        resolved_turn_id.as_str(),
        &mut runtime_context,
        false,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => {
            close_mcp_management_runtime_session(
                &mut runtime_context,
                session_id,
                resolved_turn_id.as_str(),
            )
            .await;
            finalize_chat_result(
                &sink,
                session_id,
                resolved_turn_id.as_str(),
                user_message_id.as_str(),
                true,
                &empty_chunk_sent,
                &empty_streamed_content,
                Err(error),
                true,
                || crate::utils::log_helpers::log_chat_cancelled(session_id),
                crate::utils::log_helpers::log_chat_error,
            )
            .await;
            return;
        }
    };

    let create_task_tool_name =
        prepared_mcp
            .tool_metadata
            .iter()
            .find_map(|(public_name, info)| {
                (info.server_name == "mcp_management"
                    && info.original_name == "task_runner_service_create_task")
                    .then(|| public_name.clone())
            });
    let unavailable_tools = prepared_mcp.unavailable_tools.clone();
    let tool_metadata = prepared_mcp.tool_metadata.clone();
    let use_tools = runtime_context.use_tools;
    let output_is_english = runtime_context.user_output_locale.is_english();
    let task_title = project_planning_task_title(content, output_is_english);
    let task_objective = project_planning_task_objective(content, output_is_english);
    let attachment_context = project_planning_attachment_context(attachments.as_slice());
    let lifecycle_session_id = session_id.to_string();
    let lifecycle_turn_id = resolved_turn_id.clone();
    let lifecycle_user_message_id = user_message_id.clone();
    let executor = prepared_mcp.executor;

    let result = run_chat_lifecycle(
        ChatLifecycleConfig {
            session_id,
            turn_id: resolved_turn_id.as_str(),
            user_message_id: user_message_id.as_str(),
            model_runtime,
            use_tools,
            unavailable_tools: unavailable_tools.as_slice(),
            runtime_context: &runtime_context,
            tool_metadata: &tool_metadata,
        },
        async move {
            let create_task_tool_name = create_task_tool_name.ok_or_else(|| {
                "当前会话暂时无法创建规划任务。".to_string()
            })?;
            set_task_runner_async_overall_status_for_session(
                lifecycle_session_id.as_str(),
                lifecycle_user_message_id.as_str(),
                "processing",
            )
            .await?;
            let arguments = project_planning_task_arguments(
                task_title,
                task_objective,
                content,
                attachment_context,
            );
            let tool_calls = vec![json!({
                "id": format!("project-planning-{}", lifecycle_user_message_id),
                "function": {
                    "name": create_task_tool_name,
                    "arguments": serde_json::to_string(&arguments)
                        .map_err(|error| format!("序列化规划任务失败: {error}"))?,
                }
            })];
            let tool_results = executor
                .execute_tools_stream(
                    tool_calls.as_slice(),
                    Some(lifecycle_session_id.as_str()),
                    Some(lifecycle_turn_id.as_str()),
                    None,
                    None,
                    None,
                    None,
                )
                .await;
            MessageManager::new()
                .save_tool_results(lifecycle_session_id.as_str(), tool_results.as_slice())
                .await;
            let tool_result = tool_results
                .into_iter()
                .next()
                .ok_or_else(|| "规划服务没有返回任务创建结果。".to_string())?;
            if !tool_result.success || tool_result.is_error {
                return Err(if tool_result.content.trim().is_empty() {
                    "创建规划任务失败。".to_string()
                } else {
                    tool_result.content
                });
            }
            let structured_result = tool_result.result.unwrap_or(Value::Null);
            let task_id = structured_result
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let acknowledgement = if output_is_english {
                "Planning has started. The project is being analyzed to produce requirements, technical documentation, implementation tasks, and their dependencies."
            } else {
                "规划已开始。正在分析项目并整理需求、技术文档、实施任务和依赖关系。"
            };
            let metadata = json!({
                "task_runner_async": {
                    "mode": "contact_async",
                    "message_kind": "project_planning_submitted",
                    "source_user_message_id": lifecycle_user_message_id,
                },
                "project_planning": {
                    "task_id": task_id,
                    "programmatic_submission": true,
                }
            });
            MessageManager::new()
                .save_assistant_response_message(
                    lifecycle_session_id.as_str(),
                    acknowledgement,
                    None,
                    Some("project_planning_submission".to_string()),
                    Some("task_runner".to_string()),
                    Some(metadata),
                    None,
                    None,
                    Some(lifecycle_turn_id.as_str()),
                    Some("completed"),
                )
                .await?;
            Ok(build_ai_client_success_payload(
                acknowledgement.to_string(),
                None,
                Some("stop".to_string()),
                0,
            ))
        },
    )
    .await;

    close_mcp_management_runtime_session(
        &mut runtime_context,
        session_id,
        resolved_turn_id.as_str(),
    )
    .await;
    let mark_task_runner_async_terminal = result.is_err();
    finalize_chat_result(
        &sink,
        session_id,
        resolved_turn_id.as_str(),
        user_message_id.as_str(),
        mark_task_runner_async_terminal,
        &empty_chunk_sent,
        &empty_streamed_content,
        result,
        true,
        || crate::utils::log_helpers::log_chat_cancelled(session_id),
        crate::utils::log_helpers::log_chat_error,
    )
    .await;
}

fn project_planning_task_title(content: &str, english: bool) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let summary = normalized.chars().take(72).collect::<String>();
    if summary.is_empty() {
        return if english {
            "Plan the current project".to_string()
        } else {
            "规划当前项目".to_string()
        };
    }
    if english {
        format!("Project planning: {summary}")
    } else {
        format!("项目规划：{summary}")
    }
}

fn project_planning_task_objective(content: &str, english: bool) -> String {
    if english {
        format!(
            "Plan the current project for the user's goal below. Use project facts as read-only evidence and turn the goal into traceable requirements, a non-empty technical document, implementation tasks, and a valid dependency graph in the project workspace. Re-read the saved artifacts and verify coverage before finishing; engineering implementation follows in the execution stage.\n\nUser goal:\n{}",
            content.trim()
        )
    } else {
        format!(
            "根据下面的用户目标规划当前项目。以只读方式了解项目事实，并在项目空间中形成可追踪的需求、非空技术文档、实施任务和有效依赖图；完成后重新读取已保存的产物并核对覆盖范围，工程实现由后续执行阶段承接。\n\n用户目标：\n{}",
            content.trim()
        )
    }
}

fn project_planning_task_arguments(
    title: String,
    objective: String,
    user_goal: &str,
    attachment_context: Vec<Value>,
) -> Value {
    json!({
        "title": title,
        "objective": objective,
        "input_payload": {
            "kind": "project_planning",
            "user_goal": user_goal,
            "attachments": attachment_context,
        },
        "tags": ["project-planning"],
        "requires_execution": false,
        "is_planning_task": true,
    })
}

fn merge_programmatic_planning_user_message_metadata(
    persisted: Option<Value>,
    generated: Option<Value>,
) -> Option<Value> {
    match (persisted, generated) {
        (Some(Value::Object(mut persisted)), Some(Value::Object(generated))) => {
            persisted.extend(generated);
            Some(Value::Object(persisted))
        }
        (Some(persisted), None) => Some(persisted),
        (None, generated) => generated,
        (Some(_), Some(generated)) => Some(generated),
    }
}

fn project_planning_attachment_context(attachments: &[Attachment]) -> Vec<Value> {
    attachments
        .iter()
        .map(|attachment| {
            json!({
                "id": attachment.id,
                "name": attachment.name,
                "mime_type": attachment.mime_type,
                "size": attachment.size,
                "url": attachment.url,
                "view_url": attachment.view_url,
                "text": attachment.text.as_deref().map(|text| text.chars().take(20_000).collect::<String>()),
            })
        })
        .collect()
}

fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / MB)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / KB)
    } else {
        format!("{bytes} B")
    }
}

fn validate_attachment_total_size(
    attachments: &[Attachment],
    effective_settings: &Value,
) -> Result<(), String> {
    let total_bytes = attachments.iter().fold(0u64, |total, attachment| {
        total.saturating_add(attachment.size.unwrap_or(0))
    });
    let max_bytes = attachment_total_max_bytes_from_settings(effective_settings).max(1) as u64;
    if total_bytes <= max_bytes {
        return Ok(());
    }

    Err(format!(
        "附件总大小为 {}，超过 {} 限制，请减少文件数量或换更小的文件重试。",
        format_bytes(total_bytes),
        format_bytes(max_bytes)
    ))
}

pub fn prepare_chat_execution(
    sink: ChatEventSink,
    unavailable_tools: &[Value],
    mcp_tool_metadata: ToolMetadataMap,
    _runtime_context: &ResolvedConversationRuntimeContext,
    mut callbacks: AiClientCallbacks,
    chunk_sent: Arc<AtomicBool>,
    streamed_content: Arc<Mutex<String>>,
    live_request_snapshot: LiveRequestSnapshotContext,
    actual_context_mode: &'static str,
) -> PreparedChatExecution {
    send_tools_unavailable_event(&sink, unavailable_tools);
    let live_request_snapshot_for_context = live_request_snapshot.clone();
    callbacks.on_before_model_request =
        Some(Arc::new(move |request_input, _, override_context| {
            let snapshot_context =
                override_context.unwrap_or_else(|| live_request_snapshot_for_context.clone());
            let mode = actual_context_mode.to_string();
            let items =
                crate::modules::conversation_runtime::snapshot::actual_context_items_from_v3_input(
                    request_input,
                );
            tokio::spawn(async move {
                let actual_request =
                    crate::modules::conversation_runtime::snapshot::ActualTurnRequestContext {
                        context_mode: Some(mode.clone()),
                        items,
                        model_request_payload: None,
                    };
                let _ = crate::modules::conversation_runtime::snapshot::sync_live_request_snapshot(
                    &snapshot_context,
                    &actual_request,
                )
                .await;
            });
        }));
    let live_request_snapshot_for_payload = live_request_snapshot.clone();
    callbacks.on_before_send_model_request = Some(Arc::new(move |payload| {
        let snapshot_context = live_request_snapshot_for_payload.clone();
        let mode = actual_context_mode.to_string();
        tokio::spawn(async move {
            let actual_request =
                crate::modules::conversation_runtime::snapshot::ActualTurnRequestContext {
                    context_mode: Some(mode.clone()),
                    items: crate::modules::conversation_runtime::snapshot::actual_context_items_from_v3_input(
                        payload
                            .get("input")
                            .unwrap_or(&Value::Null),
                    ),
                    model_request_payload: Some(payload),
                };
            let _ = crate::modules::conversation_runtime::snapshot::sync_live_request_snapshot(
                &snapshot_context,
                &actual_request,
            )
            .await;
        });
    }));

    PreparedChatExecution {
        sink,
        callbacks,
        chunk_sent,
        streamed_content,
        mcp_tool_metadata,
    }
}

pub async fn run_bootstrapped_chat(input: BootstrappedChatInput<'_>) {
    let BootstrappedChatInput {
        sender,
        user_id,
        project_id,
        session_id,
        content,
        persisted_user_message_content,
        persisted_user_message_metadata,
        cloud_agent_owner_context,
        model_runtime,
        bootstrap,
    } = input;
    let CommonChatBootstrap {
        effective_settings,
        mut runtime_context,
        attachments,
        user_message_id,
        resolved_turn_id,
        max_tokens,
    } = bootstrap;

    let use_tools = runtime_context.use_tools;
    let sink = build_chat_event_sink(
        sender,
        user_id.clone(),
        session_id,
        Some(resolved_turn_id.clone()),
        project_id.clone(),
        Some(user_message_id.clone()),
    );

    if let Some(runtime_error) = runtime_context.runtime_error.clone() {
        close_mcp_management_runtime_session(
            &mut runtime_context,
            session_id,
            resolved_turn_id.as_str(),
        )
        .await;
        let empty_chunk_sent = Arc::new(AtomicBool::new(false));
        let empty_streamed_content = Arc::new(Mutex::new(String::new()));
        finalize_chat_result(
            &sink,
            session_id,
            resolved_turn_id.as_str(),
            user_message_id.as_str(),
            false,
            &empty_chunk_sent,
            &empty_streamed_content,
            Err(runtime_error),
            true,
            || crate::utils::log_helpers::log_chat_cancelled(session_id),
            crate::utils::log_helpers::log_chat_error,
        )
        .await;
        return;
    }

    if let Err(attachment_error) =
        validate_attachment_total_size(attachments.as_slice(), &effective_settings)
    {
        close_mcp_management_runtime_session(
            &mut runtime_context,
            session_id,
            resolved_turn_id.as_str(),
        )
        .await;
        let empty_chunk_sent = Arc::new(AtomicBool::new(false));
        let empty_streamed_content = Arc::new(Mutex::new(String::new()));
        finalize_chat_result(
            &sink,
            session_id,
            resolved_turn_id.as_str(),
            user_message_id.as_str(),
            false,
            &empty_chunk_sent,
            &empty_streamed_content,
            Err(attachment_error),
            true,
            || crate::utils::log_helpers::log_chat_cancelled(session_id),
            crate::utils::log_helpers::log_chat_error,
        )
        .await;
        return;
    }

    let use_codex_gateway_mcp_passthrough =
        effective_codex_gateway_mcp_passthrough(model_runtime, &runtime_context);
    let prepared_mcp = match prepare_mcp_execution(
        session_id,
        resolved_turn_id.as_str(),
        &mut runtime_context,
        use_codex_gateway_mcp_passthrough,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => {
            close_mcp_management_runtime_session(
                &mut runtime_context,
                session_id,
                resolved_turn_id.as_str(),
            )
            .await;
            let empty_chunk_sent = Arc::new(AtomicBool::new(false));
            let empty_streamed_content = Arc::new(Mutex::new(String::new()));
            finalize_chat_result(
                &sink,
                session_id,
                resolved_turn_id.as_str(),
                user_message_id.as_str(),
                false,
                &empty_chunk_sent,
                &empty_streamed_content,
                Err(error),
                true,
                || crate::utils::log_helpers::log_chat_cancelled(session_id),
                crate::utils::log_helpers::log_chat_error,
            )
            .await;
            return;
        }
    };
    send_tools_unavailable_event(&sink, prepared_mcp.unavailable_tools.as_slice());
    log_chat_begin(
        session_id,
        &model_runtime.model,
        &model_runtime.base_url,
        use_tools,
        runtime_context.mcp_server_bundle.0.len(),
        runtime_context.mcp_server_bundle.1.len() + runtime_context.mcp_server_bundle.2.len(),
        !model_runtime.api_key.is_empty(),
    );
    guidance::register_active_turn(session_id, resolved_turn_id.as_str());
    sync_execution_snapshot(
        session_id,
        resolved_turn_id.as_str(),
        "running",
        user_message_id.as_str(),
        model_runtime.model.as_str(),
        model_runtime.provider.as_str(),
        &prepared_mcp.tool_metadata,
        prepared_mcp.unavailable_tools.as_slice(),
        &runtime_context,
    )
    .await;
    let start_result = start_chatos_cloud_agent(StartChatosCloudAgent {
        user_id,
        project_id,
        session_id,
        turn_id: resolved_turn_id.as_str(),
        user_message_id: user_message_id.as_str(),
        content,
        persisted_user_message_content,
        persisted_user_message_metadata,
        attachments,
        model_runtime,
        effective_settings,
        max_tokens,
        runtime_context: &runtime_context,
        prepared_mcp,
        owner_context: cloud_agent_owner_context,
    })
    .await;
    if let Err(error) = start_result {
        close_mcp_management_runtime_session(
            &mut runtime_context,
            session_id,
            resolved_turn_id.as_str(),
        )
        .await;
        guidance::close_active_turn(session_id, resolved_turn_id.as_str());
        let chunk_sent = Arc::new(AtomicBool::new(false));
        let streamed_content = Arc::new(Mutex::new(String::new()));
        finalize_chat_result(
            &sink,
            session_id,
            resolved_turn_id.as_str(),
            user_message_id.as_str(),
            true,
            &chunk_sent,
            &streamed_content,
            Err(error),
            true,
            || crate::utils::log_helpers::log_chat_cancelled(session_id),
            crate::utils::log_helpers::log_chat_error,
        )
        .await;
        return;
    }
    sink.send_done();
}

async fn close_mcp_management_runtime_session(
    runtime_context: &mut ResolvedConversationRuntimeContext,
    source_session_id: &str,
    turn_id: &str,
) {
    let Some(runtime_session) = runtime_context.mcp_management_runtime_session.take() else {
        return;
    };
    let mcp_session_id = runtime_session.session_id().to_string();
    if let Err(error) = runtime_session.close().await {
        warn!(
            source_session_id,
            turn_id,
            mcp_session_id,
            error = %error,
            "close ChatOS MCP Management runtime session failed"
        );
    }
}

pub async fn sync_execution_snapshot(
    session_id: &str,
    turn_id: &str,
    status: &str,
    user_message_id: &str,
    model: &str,
    provider: &str,
    tool_metadata: &ToolMetadataMap,
    unavailable_tools: &[Value],
    runtime_context: &ResolvedConversationRuntimeContext,
) {
    if let Err(err) = sync_chat_turn_snapshot(
        session_id,
        turn_id,
        status,
        Some(user_message_id.to_string()),
        model,
        provider,
        tool_metadata,
        unavailable_tools,
        runtime_context,
        None,
    )
    .await
    {
        warn!(
            "sync {} turn snapshot failed: session_id={}, turn_id={}, detail={}",
            status, session_id, turn_id, err
        );
    }
}

pub async fn finalize_chat_result<FC, FE>(
    sink: &ChatEventSink,
    session_id: &str,
    turn_id: &str,
    user_message_id: &str,
    mark_task_runner_async_completed: bool,
    chunk_sent: &Arc<AtomicBool>,
    streamed_content: &Arc<Mutex<String>>,
    result: Result<Value, String>,
    always_send_done: bool,
    on_cancelled: FC,
    on_error: FE,
) where
    FC: FnMut(),
    FE: FnMut(&str),
{
    if mark_task_runner_async_completed {
        let overall_status = task_runner_async_status_for_result(session_id, &result);
        match set_task_runner_async_overall_status_for_session(
            session_id,
            user_message_id,
            overall_status,
        )
        .await
        {
            Ok(Some(_)) => {}
            Ok(None) => warn!(
                session_id,
                user_message_id,
                "task runner async completed status was not persisted: message not found"
            ),
            Err(err) => warn!(
                session_id,
                user_message_id,
                error = err.as_str(),
                "task runner async completed status persist failed"
            ),
        }
    }
    let result = match result {
        Ok(value) => Ok(enrich_chat_result_with_persisted_messages(
            session_id,
            Some(turn_id),
            Some(user_message_id),
            value,
        )
        .await),
        Err(error) => Err(error),
    };

    let chunk_sent_for_result = if mark_task_runner_async_completed {
        None
    } else {
        Some(chunk_sent)
    };
    let streamed_content_for_result = if mark_task_runner_async_completed {
        None
    } else {
        Some(streamed_content)
    };
    let should_send_done = handle_chat_result(
        sink,
        session_id,
        Some(turn_id),
        Some(user_message_id),
        chunk_sent_for_result,
        streamed_content_for_result,
        result,
        on_cancelled,
        on_error,
    )
    .await;

    if always_send_done || should_send_done {
        sink.send_done();
    }
}

pub async fn run_chat_lifecycle<Fut>(
    config: ChatLifecycleConfig<'_>,
    execute_chat: Fut,
) -> Result<Value, String>
where
    Fut: std::future::Future<Output = Result<Value, String>>,
{
    log_chat_begin(
        config.session_id,
        &config.model_runtime.model,
        &config.model_runtime.base_url,
        config.use_tools,
        config.runtime_context.mcp_server_bundle.0.len(),
        config.runtime_context.mcp_server_bundle.1.len()
            + config.runtime_context.mcp_server_bundle.2.len(),
        !config.model_runtime.api_key.is_empty(),
    );

    let _active_turn = ActiveConversationTurn::start(config.session_id, config.turn_id);
    sync_execution_snapshot(
        config.session_id,
        config.turn_id,
        "running",
        config.user_message_id,
        config.model_runtime.model.as_str(),
        config.model_runtime.provider.as_str(),
        config.tool_metadata,
        config.unavailable_tools,
        config.runtime_context,
    )
    .await;

    let result = execute_chat.await;
    let terminal_status = resolve_terminal_snapshot_status(config.session_id, &result);

    sync_execution_snapshot(
        config.session_id,
        config.turn_id,
        terminal_status,
        config.user_message_id,
        config.model_runtime.model.as_str(),
        config.model_runtime.provider.as_str(),
        config.tool_metadata,
        config.unavailable_tools,
        config.runtime_context,
    )
    .await;

    result
}

fn resolve_terminal_snapshot_status(
    session_id: &str,
    result: &Result<Value, String>,
) -> &'static str {
    if abort_registry::is_aborted(session_id)
        || matches!(result, Err(err) if err.trim().eq_ignore_ascii_case("aborted"))
    {
        "cancelled"
    } else if result.is_ok() {
        "completed"
    } else {
        "failed"
    }
}

fn task_runner_async_status_for_result(
    session_id: &str,
    result: &Result<Value, String>,
) -> &'static str {
    if abort_registry::is_aborted(session_id)
        || matches!(result, Err(err) if err.trim().eq_ignore_ascii_case("aborted"))
    {
        return "cancelled";
    }

    if result.is_ok() {
        "completed"
    } else {
        "failed"
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        merge_programmatic_planning_user_message_metadata, project_planning_task_arguments,
        resolve_terminal_snapshot_status,
    };
    use crate::utils::abort_registry;

    #[test]
    fn resolve_terminal_snapshot_status_marks_aborted_error_as_cancelled() {
        let status = resolve_terminal_snapshot_status(
            "session_abort_status_err",
            &Err("aborted".to_string()),
        );
        assert_eq!(status, "cancelled");
    }

    #[test]
    fn resolve_terminal_snapshot_status_marks_aborted_registry_as_cancelled() {
        let session_id = "session_abort_status_registry";
        abort_registry::clear(session_id);
        assert!(abort_registry::abort(session_id));
        let status = resolve_terminal_snapshot_status(session_id, &Ok(json!({"ok": true})));
        assert_eq!(status, "cancelled");
        abort_registry::clear(session_id);
    }

    #[test]
    fn resolve_terminal_snapshot_status_preserves_normal_results() {
        assert_eq!(
            resolve_terminal_snapshot_status("session_abort_status_ok", &Ok(json!({"ok": true}))),
            "completed"
        );
        assert_eq!(
            resolve_terminal_snapshot_status("session_abort_status_fail", &Err("boom".to_string())),
            "failed"
        );
    }

    #[test]
    fn project_planning_submission_is_a_fixed_non_executing_task() {
        let arguments = project_planning_task_arguments(
            "规划多人刷宝游戏".to_string(),
            "读取项目并生成可追踪规划".to_string(),
            "支持注册、天梯榜和后端服务",
            Vec::new(),
        );

        assert_eq!(arguments["requires_execution"], false);
        assert_eq!(arguments["is_planning_task"], true);
        assert_eq!(arguments["tags"], json!(["project-planning"]));
        assert_eq!(arguments["input_payload"]["kind"], "project_planning");
        assert_eq!(
            arguments["input_payload"]["user_goal"],
            "支持注册、天梯榜和后端服务"
        );
        for program_bound_field in [
            "task_profile",
            "project_id",
            "owner_user_id",
            "source_session_id",
            "source_user_message_id",
            "workspace_dir",
            "runtime_provider",
        ] {
            assert!(arguments.get(program_bound_field).is_none());
        }
    }

    #[test]
    fn programmatic_planning_user_message_keeps_persisted_and_turn_metadata() {
        let metadata = merge_programmatic_planning_user_message_metadata(
            Some(json!({"hidden": false, "client": "web"})),
            Some(json!({"conversation_turn_id": "turn-1"})),
        )
        .expect("merged metadata");

        assert_eq!(metadata["hidden"], false);
        assert_eq!(metadata["client"], "web");
        assert_eq!(metadata["conversation_turn_id"], "turn-1");
    }
}
