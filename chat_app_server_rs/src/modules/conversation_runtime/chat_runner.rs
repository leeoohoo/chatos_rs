use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tracing::warn;

use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::chat_stream::{
    build_v2_callbacks, build_v3_callbacks, enrich_chat_result_with_persisted_messages,
    handle_chat_result, send_tools_unavailable_event, ChatEventSink, ChatRealtimeStreamContext,
};
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::v2::ai_server::AiServer as AiServerV2;
use crate::services::v3::ai_server::AiServer as AiServerV3;
use crate::utils::log_helpers::log_chat_begin;
use crate::utils::sse::SseSender;

use super::bootstrap::CommonChatBootstrap;
use super::chat_execution::{
    build_chat_options_v2, build_chat_options_v3, configure_ai_server_v2, configure_ai_server_v3,
    prepare_mcp_execution_v2, prepare_mcp_execution_v3, ChatExecutionInput,
};
use super::runtime_context::{ResolvedConversationRuntimeContext, ToolMetadataMap};
use super::snapshot::{sync_chat_turn_snapshot, wire_implicit_command_tracking, LiveRequestSnapshotContext};
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

pub struct BootstrappedChatV2Input<'a> {
    pub sender: Option<SseSender>,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub session_id: &'a str,
    pub content: &'a str,
    pub model_runtime: &'a ResolvedChatModelConfig,
    pub ai_server: AiServerV2,
    pub bootstrap: CommonChatBootstrap,
    pub always_send_done: bool,
}

pub struct BootstrappedChatV3Input<'a> {
    pub sender: Option<SseSender>,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub session_id: &'a str,
    pub content: &'a str,
    pub model_runtime: &'a ResolvedChatModelConfig,
    pub ai_server: AiServerV3,
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

pub fn prepare_chat_execution(
    sink: ChatEventSink,
    unavailable_tools: &[Value],
    mcp_tool_metadata: ToolMetadataMap,
    runtime_context: &ResolvedConversationRuntimeContext,
    mut callbacks: AiClientCallbacks,
    chunk_sent: Arc<AtomicBool>,
    streamed_content: Arc<Mutex<String>>,
    live_request_snapshot: LiveRequestSnapshotContext,
    actual_context_mode: &'static str,
) -> PreparedChatExecution {
    send_tools_unavailable_event(&sink, unavailable_tools);
    wire_implicit_command_tracking(
        &mut callbacks,
        runtime_context.selected_commands_for_snapshot.clone(),
    );
    callbacks.on_before_model_request = Some(Arc::new(move |request_input, previous_response_id, override_context| {
        let snapshot_context = override_context.unwrap_or_else(|| live_request_snapshot.clone());
        let mode = actual_context_mode.to_string();
        tokio::spawn(async move {
            let actual_request = crate::modules::conversation_runtime::snapshot::ActualTurnRequestContext {
                context_mode: Some(mode.clone()),
                previous_response_id,
                items: if mode == "v3" {
                    crate::modules::conversation_runtime::snapshot::actual_context_items_from_v3_input(&request_input)
                } else {
                    crate::modules::conversation_runtime::snapshot::actual_context_items_from_v2_messages(
                        request_input.as_array().map(|items| items.as_slice()).unwrap_or(&[]),
                    )
                },
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

pub async fn run_bootstrapped_chat_v2(input: BootstrappedChatV2Input<'_>) {
    let BootstrappedChatV2Input {
        sender,
        user_id,
        project_id,
        session_id,
        content,
        model_runtime,
        ai_server,
        bootstrap,
        always_send_done,
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
    let prepared_mcp =
        prepare_mcp_execution_v2(session_id, resolved_turn_id.as_str(), &mut runtime_context).await;
    let sink = build_chat_event_sink(
        sender,
        user_id,
        session_id,
        Some(resolved_turn_id.clone()),
        project_id,
        Some(user_message_id.clone()),
    );
    let callback_bundle = build_v2_callbacks(&sink, session_id);
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
        "v2",
    );
    let mut ai_server = ai_server;
    configure_ai_server_v2(
        &mut ai_server,
        session_id,
        resolved_turn_id.as_str(),
        &runtime_context,
        &effective_settings,
        prepared_mcp.executor,
    );
    let unavailable_tools = prepared_mcp.unavailable_tools.clone();
    let chat_options = build_chat_options_v2(
        model_runtime,
        prepared_mcp.prefixed_messages,
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
        &prepared.chunk_sent,
        &prepared.streamed_content,
        result,
        always_send_done,
        || crate::utils::log_helpers::log_chat_cancelled(session_id),
        |err| crate::utils::log_helpers::log_chat_error(err),
    )
    .await;
}

pub async fn run_bootstrapped_chat_v3(input: BootstrappedChatV3Input<'_>) {
    let BootstrappedChatV3Input {
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
    let prepared_mcp = prepare_mcp_execution_v3(
        session_id,
        resolved_turn_id.as_str(),
        &mut runtime_context,
        model_runtime.use_codex_gateway_mcp_passthrough,
    )
    .await;
    let sink = build_chat_event_sink(
        sender,
        user_id,
        session_id,
        Some(resolved_turn_id.clone()),
        project_id,
        Some(user_message_id.clone()),
    );
    let callback_bundle = build_v3_callbacks(&sink, session_id, true);
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
        "v3",
    );
    let mut ai_server = ai_server;
    configure_ai_server_v3(
        &mut ai_server,
        session_id,
        resolved_turn_id.as_str(),
        &runtime_context,
        &effective_settings,
        prepared_mcp.executor,
    );
    let unavailable_tools = prepared_mcp.unavailable_tools.clone();
    let chat_options = build_chat_options_v3(
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

    let should_send_done = handle_chat_result(
        sink,
        session_id,
        Some(turn_id),
        Some(user_message_id),
        Some(chunk_sent),
        Some(streamed_content),
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

    sync_execution_snapshot(
        config.session_id,
        config.turn_id,
        if result.is_ok() {
            "completed"
        } else {
            "failed"
        },
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
