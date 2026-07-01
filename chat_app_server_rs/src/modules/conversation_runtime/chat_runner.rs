// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tracing::warn;

use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::ai_settings::attachment_total_max_bytes_from_settings;
use crate::core::chat_stream::{
    build_chat_stream_callbacks, enrich_chat_result_with_persisted_messages, handle_chat_result,
    send_tools_unavailable_event, ChatEventSink, ChatRealtimeStreamContext,
};
use crate::core::messages::set_task_runner_async_overall_status_for_session;
use crate::services::agent_runtime::ai_server::AiServer as AgentAiServer;
use crate::services::ai_client_common::AiClientCallbacks;
use crate::utils::abort_registry;
use crate::utils::attachments::Attachment;
use crate::utils::log_helpers::log_chat_begin;
use crate::utils::sse::SseSender;

use super::bootstrap::CommonChatBootstrap;
use super::chat_execution::{
    build_agent_chat_options, configure_agent_ai_server, prepare_mcp_execution, ChatExecutionInput,
};
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
    pub model_runtime: &'a ResolvedChatModelConfig,
    pub ai_server: AgentAiServer,
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
        model_runtime,
        ai_server,
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
        user_id,
        session_id,
        Some(resolved_turn_id.clone()),
        project_id,
        Some(user_message_id.clone()),
    );

    if let Some(runtime_error) = runtime_context.runtime_error.clone() {
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
            |err| crate::utils::log_helpers::log_chat_error(err),
        )
        .await;
        return;
    }

    if let Err(attachment_error) =
        validate_attachment_total_size(attachments.as_slice(), &effective_settings)
    {
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
            |err| crate::utils::log_helpers::log_chat_error(err),
        )
        .await;
        return;
    }

    let prepared_mcp = prepare_mcp_execution(
        session_id,
        resolved_turn_id.as_str(),
        &mut runtime_context,
        model_runtime.use_codex_gateway_mcp_passthrough,
    )
    .await;
    let mut callback_bundle = build_chat_stream_callbacks(&sink, session_id, false);
    callback_bundle.callbacks.on_chunk = None;
    callback_bundle.callbacks.on_thinking = None;
    callback_bundle.callbacks.on_tools_start = None;
    callback_bundle.callbacks.on_tools_stream = None;
    callback_bundle.callbacks.on_tools_end = None;
    let prepared = prepare_chat_execution(
        sink,
        prepared_mcp.unavailable_tools.as_slice(),
        prepared_mcp.tool_metadata.clone(),
        &runtime_context,
        callback_bundle.callbacks.clone(),
        callback_bundle.chunk_sent.clone(),
        callback_bundle.streamed_content.clone(),
        build_live_request_snapshot_context(&ChatLifecycleConfig {
            session_id,
            turn_id: resolved_turn_id.as_str(),
            user_message_id: user_message_id.as_str(),
            model_runtime,
            use_tools,
            unavailable_tools: prepared_mcp.unavailable_tools.as_slice(),
            runtime_context: &runtime_context,
            tool_metadata: &prepared_mcp.tool_metadata,
        }),
        "responses",
    );
    let mut ai_server = ai_server;
    configure_agent_ai_server(
        &mut ai_server,
        session_id,
        resolved_turn_id.as_str(),
        &runtime_context,
        &effective_settings,
        prepared_mcp.executor,
    );
    let unavailable_tools = prepared_mcp.unavailable_tools.clone();
    let chat_options = build_agent_chat_options(
        model_runtime,
        &runtime_context,
        prepared_mcp.prefixed_input_items,
        ChatExecutionInput {
            use_tools,
            max_tokens,
            attachments,
            callbacks: prepared.callbacks.clone(),
            turn_id: resolved_turn_id.clone(),
            user_message_id: user_message_id.clone(),
            message_source: model_runtime.model.clone(),
        },
    );
    let result = run_chat_lifecycle(
        ChatLifecycleConfig {
            session_id,
            turn_id: resolved_turn_id.as_str(),
            user_message_id: user_message_id.as_str(),
            model_runtime,
            use_tools,
            unavailable_tools: unavailable_tools.as_slice(),
            runtime_context: &runtime_context,
            tool_metadata: &prepared.mcp_tool_metadata,
        },
        ai_server.chat(session_id, content, chat_options),
    )
    .await;

    finalize_chat_result(
        &prepared.sink,
        session_id,
        resolved_turn_id.as_str(),
        user_message_id.as_str(),
        true,
        &prepared.chunk_sent,
        &prepared.streamed_content,
        result,
        false,
        || crate::utils::log_helpers::log_chat_cancelled(session_id),
        |err| crate::utils::log_helpers::log_chat_error(err),
    )
    .await;
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
        match set_task_runner_async_overall_status_for_session(
            session_id,
            user_message_id,
            "completed",
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::resolve_terminal_snapshot_status;
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
}
