use crate::api::chat_stream_common::ChatStreamRequest;
use crate::config::Config;
use crate::core::ai_model_config::ResolvedChatModelConfig;
use crate::core::chat_context::maybe_spawn_session_title_rename;
use crate::core::chat_stream::{send_error_event, send_start_event};
use crate::services::ai_common::normalize_turn_id;
use crate::services::model_runtime_resolver::resolve_model_runtime_for_request;
use crate::utils::sse::SseSender;

use super::bootstrap::{load_common_chat_bootstrap, CommonChatBootstrapInput};
use super::chat_execution::{init_ai_server_v2, init_ai_server_v3};
use super::chat_runner::{
    build_chat_event_sink, run_bootstrapped_chat_v2, run_bootstrapped_chat_v3,
    BootstrappedChatV2Input, BootstrappedChatV3Input,
};

pub struct RunChatV2UsecaseInput {
    pub sender: Option<SseSender>,
    pub req: ChatStreamRequest,
    pub always_send_done: bool,
    pub rename_session: bool,
    pub respect_model_flags: bool,
}

pub struct RunChatV3UsecaseInput {
    pub sender: Option<SseSender>,
    pub req: ChatStreamRequest,
}

pub async fn run_chat_v2_usecase(input: RunChatV2UsecaseInput) {
    let RunChatV2UsecaseInput {
        sender,
        req,
        always_send_done,
        rename_session,
        respect_model_flags,
    } = input;
    let session_id = req.conversation_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let initial_turn_id = normalize_turn_id(req.turn_id.as_deref());
    let initial_sink = build_chat_event_sink(
        sender.clone(),
        req.user_id.clone(),
        &session_id,
        initial_turn_id,
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
        return;
    }

    send_start_event(&initial_sink, &session_id);
    maybe_spawn_session_title_rename(rename_session, &session_id, &content, 30);

    let model_runtime = match resolve_chat_model_runtime(&req, "gpt-4", respect_model_flags).await {
        Ok(runtime) => runtime,
        Err(err) => {
            send_error_event(
                &initial_sink,
                format!("解析模型配置失败: {err}").as_str(),
                None,
            );
            initial_sink.send_done();
            return;
        }
    };

    let ai_server = match init_ai_server_v2(&model_runtime) {
        Ok(ai_server) => ai_server,
        Err(err) => {
            send_error_event(
                &initial_sink,
                format!("初始化 AI 服务失败: {err}").as_str(),
                None,
            );
            initial_sink.send_done();
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
    run_bootstrapped_chat_v2(BootstrappedChatV2Input {
        sender: sender.clone(),
        user_id: req.user_id.clone(),
        project_id: req.project_id.clone(),
        session_id: &session_id,
        content: &content,
        model_runtime: &model_runtime,
        ai_server,
        bootstrap,
        always_send_done,
    })
    .await;
}

pub async fn run_chat_v3_usecase(input: RunChatV3UsecaseInput) {
    let RunChatV3UsecaseInput { sender, req } = input;
    let session_id = req.conversation_id.clone().unwrap_or_default();
    let content = req.content.clone().unwrap_or_default();
    let initial_turn_id = normalize_turn_id(req.turn_id.as_deref());
    let initial_sink = build_chat_event_sink(
        sender.clone(),
        req.user_id.clone(),
        &session_id,
        initial_turn_id,
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
            return;
        }
    };
    if !model_runtime.supports_responses {
        send_error_event(&initial_sink, "当前模型未启用 Responses API", None);
        initial_sink.send_done();
        return;
    }

    let ai_server = init_ai_server_v3(&model_runtime);
    let bootstrap = load_common_chat_bootstrap(build_common_bootstrap_input(
        &req,
        &session_id,
        &content,
        &model_runtime,
    ))
    .await;
    run_bootstrapped_chat_v3(BootstrappedChatV3Input {
        sender: sender.clone(),
        user_id: req.user_id.clone(),
        project_id: req.project_id.clone(),
        session_id: &session_id,
        content: &content,
        model_runtime: &model_runtime,
        ai_server,
        bootstrap,
    })
    .await;
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
        remote_connection_id: req.remote_connection_id.clone(),
        mcp_enabled: req.mcp_enabled,
        enabled_mcp_ids: req.enabled_mcp_ids.clone(),
        skills_enabled: req.skills_enabled,
        selected_skill_ids: req.selected_skill_ids.clone(),
        turn_id: req.turn_id.clone(),
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
