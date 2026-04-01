use std::collections::HashSet;
use std::time::Duration;

use axum::http::StatusCode;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[path = "agent_builder_create.rs"]
mod create_support;
#[path = "agent_builder_flow.rs"]
mod flow_support;
#[path = "agent_builder_request.rs"]
mod request_support;
#[path = "agent_builder_runtime.rs"]
mod runtime_support;
#[path = "agent_builder_stream.rs"]
mod stream_support;
#[path = "agent_builder_support.rs"]
mod support;
#[path = "agent_builder_tools.rs"]
mod tool_support;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::MemoryAgent;
use flow_support::run_agent_builder;
use runtime_support::resolve_model_runtime;
use support::{
    normalize_optional_string_array, normalize_optional_text, normalize_required_text,
    resolve_visible_user_ids,
};

#[derive(Debug, Clone, Deserialize)]
pub struct AiCreateAgentRequest {
    pub user_id: Option<String>,
    pub model_config_id: Option<String>,
    pub requirement: Option<String>,
    pub name: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub role_definition: Option<String>,
    pub plugin_sources: Option<Vec<String>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub skill_prompts: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub mcp_enabled: Option<bool>,
    pub enabled_mcp_ids: Option<Vec<String>>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub ai_model_config: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiCreateAgentResult {
    pub created: bool,
    pub agent: MemoryAgent,
    pub source: String,
    pub model: String,
    pub provider: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone)]
struct NormalizedRequest {
    scope_user_id: String,
    model_config_id: Option<String>,
    requirement: String,
    name: Option<String>,
    category: Option<String>,
    description: Option<String>,
    role_definition: Option<String>,
    plugin_sources: Option<Vec<String>>,
    skill_ids: Option<Vec<String>>,
    default_skill_ids: Option<Vec<String>>,
    skill_prompts: Option<Vec<String>>,
    enabled: Option<bool>,
    mcp_enabled: Option<bool>,
    enabled_mcp_ids: Option<Vec<String>>,
    project_id: Option<String>,
    project_root: Option<String>,
    ai_model_config: Option<Value>,
}

#[derive(Debug, Clone)]
struct ModelRuntime {
    provider: String,
    model: String,
    base_url: String,
    api_key: String,
    temperature: f64,
    request_timeout_secs: u64,
    supports_responses: bool,
}

#[derive(Debug, Default)]
struct ToolState {
    listed_skills: bool,
    created_once: bool,
}

struct ToolContext<'a> {
    db: &'a Db,
    request: &'a NormalizedRequest,
    visible_user_ids: Vec<String>,
    state: ToolState,
}

#[derive(Debug, Clone)]
struct ToolCall {
    id: String,
    name: String,
    arguments: Value,
    raw: Value,
}

#[derive(Debug, Clone)]
struct ToolExecution {
    payload: Value,
    created_agent: Option<MemoryAgent>,
}

#[derive(Debug, Clone)]
struct ToolLoopOutcome {
    created_agent: Option<MemoryAgent>,
    final_content: Option<String>,
}

#[derive(Debug, Clone)]
struct VisibleSkillCatalog {
    items: Vec<crate::models::MemorySkill>,
    ids: HashSet<String>,
}

pub async fn ai_create_agent(
    db: &Db,
    config: &AppConfig,
    scope_user_id: String,
    req: AiCreateAgentRequest,
) -> Result<AiCreateAgentResult, (StatusCode, String)> {
    let request = NormalizedRequest::from_request(scope_user_id, req)?;
    let runtime = resolve_model_runtime(db, config, &request).await?;
    let http = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        // In WSL deployments we've seen stale pooled sockets cause follow-up
        // calls to fail intermittently. Use short-lived connections here.
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|err| internal_error(format!("init agent builder http client failed: {err}")))?;

    let mut context = ToolContext {
        db,
        request: &request,
        visible_user_ids: resolve_visible_user_ids(request.scope_user_id.as_str()),
        state: ToolState::default(),
    };

    let outcome = run_agent_builder(&http, &runtime, &mut context).await?;
    let Some(agent) = outcome.created_agent else {
        return Err(bad_gateway_error("AI 未返回可创建的智能体配置"));
    };

    Ok(AiCreateAgentResult {
        created: true,
        agent,
        source: "memory_llm_agent_builder".to_string(),
        model: runtime.model,
        provider: runtime.provider,
        content: outcome.final_content,
    })
}

impl NormalizedRequest {
    fn from_request(
        scope_user_id: String,
        req: AiCreateAgentRequest,
    ) -> Result<Self, (StatusCode, String)> {
        let requirement = normalize_required_text(req.requirement, "requirement")?;

        Ok(Self {
            scope_user_id,
            model_config_id: normalize_optional_text(req.model_config_id),
            requirement,
            name: normalize_optional_text(req.name),
            category: normalize_optional_text(req.category),
            description: normalize_optional_text(req.description),
            role_definition: normalize_optional_text(req.role_definition),
            plugin_sources: normalize_optional_string_array(req.plugin_sources),
            skill_ids: normalize_optional_string_array(req.skill_ids),
            default_skill_ids: normalize_optional_string_array(req.default_skill_ids),
            skill_prompts: normalize_optional_string_array(req.skill_prompts),
            enabled: req.enabled,
            mcp_enabled: req.mcp_enabled,
            enabled_mcp_ids: normalize_optional_string_array(req.enabled_mcp_ids),
            project_id: normalize_optional_text(req.project_id),
            project_root: normalize_optional_text(req.project_root),
            ai_model_config: req.ai_model_config,
        })
    }
}

fn bad_request_error(message: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message.into())
}

fn bad_gateway_error(message: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::BAD_GATEWAY, message.into())
}

fn internal_error(message: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, message.into())
}
