// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::api::chat_stream_common::ChatStreamRequest;
use crate::config::Config;
use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::chat_context::maybe_spawn_session_title_rename;
use crate::core::chat_stream::{send_error_event, send_start_event};
use crate::services::ai_common::normalize_turn_id;
use crate::services::model_runtime_resolver::resolve_model_runtime_for_request;
use crate::utils::sse::SseSender;

use super::bootstrap::{load_common_chat_bootstrap, CommonChatBootstrapInput};
use super::chat_execution::init_chatos_stream_agent;
use super::chat_runner::{build_chat_event_sink, run_bootstrapped_chat, BootstrappedChatInput};
use super::guidance;

pub struct RunChatUsecaseInput {
    pub sender: Option<SseSender>,
    pub req: ChatStreamRequest,
}

pub async fn run_chat_usecase(input: RunChatUsecaseInput) {
    let RunChatUsecaseInput { sender, req } = input;
    let session_id = req.conversation_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let initial_turn_id = normalize_turn_id(req.turn_id.as_deref());
    let initial_sink = build_chat_event_sink(
        sender.clone(),
        req.user_id.clone(),
        &session_id,
        initial_turn_id.clone(),
        req.project_id.clone(),
        None,
    );
    if let Err(err) = Config::try_get() {
        send_error_event(
            &initial_sink,
            format!("服务配置未初始化: {err}").as_str(),
            None,
        );
        initial_sink.send_done();
        close_initial_turn(&session_id, initial_turn_id.as_deref());
        return;
    }

    send_start_event(&initial_sink, &session_id);
    maybe_spawn_session_title_rename(true, &session_id, &content, 30);

    let model_runtime = match resolve_chat_model_runtime(&req, "gpt-4o", true).await {
        Ok(runtime) => runtime,
        Err(err) => {
            send_error_event(
                &initial_sink,
                format!("解析模型配置失败: {err}").as_str(),
                None,
            );
            initial_sink.send_done();
            close_initial_turn(&session_id, initial_turn_id.as_deref());
            return;
        }
    };
    let bootstrap = load_common_chat_bootstrap(build_common_bootstrap_input(
        &req,
        &session_id,
        &content,
        &model_runtime,
    ))
    .await;
    let agent = init_chatos_stream_agent(&model_runtime, bootstrap.runtime_context.agent_profile);
    run_bootstrapped_chat(BootstrappedChatInput {
        sender: sender.clone(),
        user_id: req.user_id.clone(),
        project_id: req.project_id.clone(),
        session_id: &session_id,
        content: &content,
        model_runtime: &model_runtime,
        agent,
        bootstrap,
    })
    .await;
}

fn close_initial_turn(session_id: &str, turn_id: Option<&str>) {
    if let Some(turn_id) = turn_id {
        guidance::close_active_turn(session_id, turn_id);
    }
}

fn build_common_bootstrap_input(
    req: &ChatStreamRequest,
    session_id: &str,
    content: &str,
    model_runtime: &ResolvedChatModelConfig,
) -> CommonChatBootstrapInput {
    CommonChatBootstrapInput {
        session_id: session_id.to_string(),
        content: content.to_string(),
        user_id: req.user_id.clone(),
        contact_agent_id: req.contact_agent_id.clone(),
        project_id: req.project_id.clone(),
        project_root: req.project_root.clone(),
        workspace_root: req.workspace_root.clone(),
        remote_connection_id: req.remote_connection_id.clone(),
        plan_mode: req.plan_mode,
        project_requirement_execution_planner: req.project_requirement_execution_planner,
        turn_id: req.turn_id.clone(),
        user_message_id: req.user_message_id.clone(),
        attachments: req.attachments.clone(),
        default_system_prompt: model_runtime.system_prompt.clone(),
        use_active_system_context: model_runtime.use_active_system_context,
    }
}

async fn resolve_chat_model_runtime(
    req: &ChatStreamRequest,
    default_model: &str,
    respect_model_flags: bool,
) -> Result<ResolvedChatModelConfig, String> {
    resolve_model_runtime_for_request(
        req.model_config_id.as_deref(),
        req.ai_model_config.as_ref(),
        req.conversation_id.as_deref(),
        req.user_id.as_deref(),
        default_model,
        req.reasoning_enabled,
        respect_model_flags,
    )
    .await
}
