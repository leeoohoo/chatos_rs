// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tracing::warn;

use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::ai_settings::attachment_total_max_bytes_from_settings;
use crate::core::chat_stream::{
    enrich_chat_result_with_persisted_messages, handle_chat_result, send_tools_unavailable_event,
    ChatEventSink, ChatRealtimeStreamContext,
};
use crate::core::messages::set_task_runner_async_overall_status_for_session;
use crate::services::agent_runtime::message_manager::MessageManager;
use crate::services::ai_common::{
    build_user_message_metadata, TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE,
};
use crate::utils::abort_registry;
use crate::utils::attachments::Attachment;
use crate::utils::log_helpers::log_chat_begin;
use crate::utils::sse::SseSender;

use super::bootstrap::CommonChatBootstrap;
use super::chat_execution::{
    effective_codex_gateway_mcp_passthrough, merge_user_record_metadata, prepare_mcp_execution,
};
use super::cloud_agent::{start_chatos_cloud_agent, StartChatosCloudAgent};
use super::guidance;
use super::runtime_context::{ResolvedConversationRuntimeContext, ToolMetadataMap};
use super::snapshot::sync_chat_turn_snapshot;

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

    let persisted_content = persisted_user_message_content.as_deref().unwrap_or(content);
    let persisted_metadata = merge_user_record_metadata(
        persisted_user_message_metadata.clone(),
        build_user_message_metadata(attachments.as_slice(), Some(resolved_turn_id.as_str())),
    );
    if let Err(error) = MessageManager::new()
        .save_user_message(
            session_id,
            persisted_content,
            Some(user_message_id.clone()),
            Some(TASK_RUNNER_ASYNC_PLAN_MESSAGE_MODE.to_string()),
            Some(model_runtime.model.clone()),
            persisted_metadata,
        )
        .await
    {
        let empty_chunk_sent = Arc::new(AtomicBool::new(false));
        let empty_streamed_content = Arc::new(Mutex::new(String::new()));
        finalize_chat_result(
            &sink,
            session_id,
            resolved_turn_id.as_str(),
            user_message_id.as_str(),
            false,
            None,
            &empty_chunk_sent,
            &empty_streamed_content,
            Err(format!("保存用户消息失败: {error}")),
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
        let empty_chunk_sent = Arc::new(AtomicBool::new(false));
        let empty_streamed_content = Arc::new(Mutex::new(String::new()));
        finalize_chat_result(
            &sink,
            session_id,
            resolved_turn_id.as_str(),
            user_message_id.as_str(),
            false,
            None,
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
            None,
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
                None,
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
            None,
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
    task_runner_async_success_status: Option<&str>,
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
        let overall_status = task_runner_async_status_for_result(
            session_id,
            &result,
            task_runner_async_success_status,
        );
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

fn task_runner_async_status_for_result(
    session_id: &str,
    result: &Result<Value, String>,
    success_status: Option<&str>,
) -> &'static str {
    if abort_registry::is_aborted(session_id)
        || matches!(result, Err(err) if err.trim().eq_ignore_ascii_case("aborted"))
    {
        return "cancelled";
    }

    if result.is_ok() {
        if success_status.is_some_and(|status| status.eq_ignore_ascii_case("processing")) {
            "processing"
        } else {
            "completed"
        }
    } else {
        "failed"
    }
}
