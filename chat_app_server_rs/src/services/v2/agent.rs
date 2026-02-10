use serde_json::{json, Value};

use crate::config::Config;
use crate::repositories::{agents, ai_model_configs, system_contexts};
use crate::services::mcp_loader::load_mcp_configs_for_user;
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v2::ai_client::AiClientCallbacks;
use crate::services::v2::ai_server::{AiServer, ChatOptions};
use crate::utils::attachments::Attachment;

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub model_name: String,
    pub provider: String,
    pub thinking_level: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
    pub system_prompt: Option<String>,
    pub use_active_system_context: bool,
    pub user_id: Option<String>,
    pub mcp_config_ids: Vec<String>,
    pub project_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub lock_mcp: bool,
}

pub async fn load_model_config_for_agent(agent_id: &str) -> Result<ModelConfig, String> {
    let agent = agents::get_agent_by_id(agent_id)
        .await?
        .ok_or("智能体不存在或未启用")?;
    if agent.enabled == false {
        return Err("智能体不存在或未启用".to_string());
    }

    let model_cfg = ai_model_configs::get_ai_model_config_by_id(&agent.ai_model_config_id)
        .await?
        .ok_or("模型配置不可用或未启用")?;
    if model_cfg.enabled == false {
        return Err("模型配置不可用或未启用".to_string());
    }

    let mut system_prompt = None;
    if let Some(ctx_id) = agent.system_context_id.clone() {
        if let Ok(Some(ctx)) = system_contexts::get_system_context_by_id(&ctx_id).await {
            if ctx.is_active {
                system_prompt = ctx.content;
            }
        }
    }

    Ok(ModelConfig {
        model_name: model_cfg.model,
        provider: model_cfg.provider,
        thinking_level: model_cfg.thinking_level,
        api_key: model_cfg.api_key,
        base_url: model_cfg.base_url,
        supports_images: model_cfg.supports_images,
        supports_reasoning: model_cfg.supports_reasoning,
        temperature: 0.7,
        max_tokens: None,
        system_prompt,
        use_active_system_context: false,
        user_id: agent.user_id,
        mcp_config_ids: agent.mcp_config_ids,
        project_id: agent.project_id,
        workspace_dir: agent.workspace_dir,
        lock_mcp: true,
    })
}

pub async fn run_chat(
    session_id: &str,
    content: &str,
    model_config: &ModelConfig,
    user_id: Option<String>,
    attachments: Vec<Attachment>,
    reasoning_enabled: Option<bool>,
    callbacks: AiClientCallbacks,
) -> Result<Value, String> {
    let cfg = Config::get();
    let effective_user_id = user_id.or(model_config.user_id.clone());
    let filtered_ids: Vec<String> = model_config
        .mcp_config_ids
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();
    let has_mcp = !filtered_ids.is_empty();
    let workspace_dir =
        crate::utils::workspace::resolve_workspace_dir(model_config.workspace_dir.as_deref());
    let workspace_dir_opt = if workspace_dir.trim().is_empty() {
        None
    } else {
        Some(workspace_dir.as_str())
    };
    let (http_servers, mut stdio_servers, builtin_servers) = if model_config.lock_mcp {
        if has_mcp {
            load_mcp_configs_for_user(
                effective_user_id.clone(),
                Some(filtered_ids),
                workspace_dir_opt,
                model_config.project_id.as_deref(),
            )
            .await
            .unwrap_or((Vec::new(), Vec::new(), Vec::new()))
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        }
    } else {
        load_mcp_configs_for_user(
            effective_user_id.clone(),
            None,
            workspace_dir_opt,
            model_config.project_id.as_deref(),
        )
        .await
        .unwrap_or((Vec::new(), Vec::new(), Vec::new()))
    };
    if !workspace_dir.trim().is_empty() {
        for server in stdio_servers.iter_mut() {
            if server
                .cwd
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                server.cwd = Some(workspace_dir.clone());
            }
        }
    }

    let mut mcp_tool_execute = crate::services::v2::mcp_tool_execute::McpToolExecute::new(
        http_servers,
        stdio_servers,
        builtin_servers,
    );
    if has_mcp
        && (!mcp_tool_execute.mcp_servers.is_empty()
            || !mcp_tool_execute.stdio_mcp_servers.is_empty()
            || !mcp_tool_execute.builtin_mcp_servers.is_empty())
    {
        let _ = mcp_tool_execute.init().await;
    }

    let api_key = model_config
        .api_key
        .clone()
        .unwrap_or_else(|| cfg.openai_api_key.clone());
    let base_url = model_config
        .base_url
        .clone()
        .unwrap_or_else(|| cfg.openai_base_url.clone());

    let mut ai_server = AiServer::new(
        api_key,
        base_url.clone(),
        model_config.model_name.clone(),
        model_config.temperature,
        mcp_tool_execute,
    );

    if let Some(prompt) = model_config.system_prompt.clone() {
        ai_server.set_system_prompt(Some(prompt));
    } else if model_config.use_active_system_context {
        if let Some(uid) = effective_user_id.clone() {
            if let Ok(Some(ctx)) = system_contexts::get_active_system_context(&uid).await {
                if let Some(content) = ctx.content {
                    ai_server.set_system_prompt(Some(content));
                }
            }
        }
    }

    let effective_settings = get_effective_user_settings(effective_user_id.clone())
        .await
        .unwrap_or_else(|_| json!({}));
    apply_settings_to_ai_client(&mut ai_server.ai_client, &effective_settings);
    let max_tokens = effective_settings
        .get("CHAT_MAX_TOKENS")
        .and_then(|v| v.as_i64())
        .filter(|v| *v > 0);

    let effective_reasoning = (model_config.supports_reasoning
        || model_config.thinking_level.is_some())
        && reasoning_enabled.unwrap_or(true);

    let options = ChatOptions {
        model: Some(model_config.model_name.clone()),
        provider: Some(model_config.provider.clone()),
        thinking_level: model_config.thinking_level.clone(),
        temperature: Some(model_config.temperature),
        max_tokens,
        use_tools: Some(has_mcp),
        attachments: Some(attachments),
        supports_images: Some(model_config.supports_images),
        reasoning_enabled: Some(effective_reasoning),
        callbacks: Some(callbacks),
    };

    ai_server.chat(session_id, content, options).await
}
