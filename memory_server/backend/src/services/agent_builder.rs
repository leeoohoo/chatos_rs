use std::collections::HashSet;
use std::error::Error as StdError;
use std::time::Duration;

use axum::http::StatusCode;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tracing::warn;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    AiModelConfig, CreateMemoryAgentRequest, MemoryAgent, MemoryAgentSkill, MemorySkill,
    MemorySkillPlugin,
};
use crate::repositories::{
    agents as agents_repo, auth::ADMIN_USER_ID, configs, skills as skills_repo,
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
        .timeout(Duration::from_secs(config.ai_request_timeout_secs))
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

async fn run_agent_builder(
    http: &Client,
    runtime: &ModelRuntime,
    context: &mut ToolContext<'_>,
) -> Result<ToolLoopOutcome, (StatusCode, String)> {
    if runtime.supports_responses {
        return run_plain_json_fallback(http, runtime, context).await;
    }

    match run_tool_loop(http, runtime, context).await {
        Ok(outcome) => Ok(outcome),
        Err((status, detail))
            if status == StatusCode::BAD_GATEWAY && is_tooling_unsupported(detail.as_str()) =>
        {
            run_plain_json_fallback(http, runtime, context).await
        }
        Err(err) => Err(err),
    }
}

async fn run_tool_loop(
    http: &Client,
    runtime: &ModelRuntime,
    context: &mut ToolContext<'_>,
) -> Result<ToolLoopOutcome, (StatusCode, String)> {
    let visible_skills = skills_repo::list_skills(
        context.db,
        context.visible_user_ids.as_slice(),
        None,
        None,
        1000,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load skills for tool loop failed: {err}")))?;
    let visible_agents = agents_repo::list_agents(
        context.db,
        context.visible_user_ids.as_slice(),
        Some(true),
        200,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load agents for tool loop failed: {err}")))?;
    let visible_plugins = skills_repo::list_plugins_by_user_ids(
        context.db,
        context.visible_user_ids.as_slice(),
        300,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load skill plugins for tool loop failed: {err}")))?;

    let mut messages = vec![
        json!({"role": "system", "content": build_tool_loop_system_prompt()}),
        json!({
            "role": "user",
            "content": build_tool_loop_user_prompt(
                context.request,
                visible_skills.as_slice(),
                visible_agents.as_slice(),
                visible_plugins.as_slice(),
            )
        }),
    ];
    let tools = build_agent_builder_tools();
    let mut created_agent = None;
    let mut final_content = None;

    for _ in 0..8 {
        let response =
            request_chat_completion(http, runtime, messages.as_slice(), Some(tools.as_slice()))
                .await?;
        let choice = response
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .ok_or_else(|| {
                bad_gateway_error(format!(
                    "agent builder response missing choices: {}",
                    response
                ))
            })?;
        let message = choice.get("message").cloned().ok_or_else(|| {
            bad_gateway_error(format!(
                "agent builder response missing message: {}",
                response
            ))
        })?;
        let message_content = extract_message_text(message.get("content"));
        let tool_calls = parse_tool_calls(message.get("tool_calls"));

        if !tool_calls.is_empty() {
            messages.push(json!({
                "role": "assistant",
                "content": message.get("content").cloned().unwrap_or(Value::Null),
                "tool_calls": tool_calls.iter().map(|item| item.raw.clone()).collect::<Vec<_>>(),
            }));

            for tool_call in tool_calls {
                let execution = execute_tool_call(context, &tool_call).await;
                let (success, payload, agent) = match execution {
                    Ok(result) => (true, result.payload, result.created_agent),
                    Err(err) => (false, json!({"error": err}), None),
                };
                if created_agent.is_none() {
                    created_agent = agent;
                }

                let tool_payload = json!({
                    "success": success,
                    "name": tool_call.name,
                    "data": payload,
                });
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call.id,
                    "content": serde_json::to_string(&tool_payload)
                        .unwrap_or_else(|_| tool_payload.to_string()),
                }));
            }
            continue;
        }

        if message_content
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            final_content = message_content;
            break;
        }

        if choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .is_some_and(|reason| reason == "stop")
            && created_agent.is_some()
        {
            break;
        }
    }

    if created_agent.is_none() {
        if let Some(content) = final_content.as_deref() {
            created_agent = create_agent_from_final_response(context, content).await?;
        }
    }

    Ok(ToolLoopOutcome {
        created_agent,
        final_content,
    })
}

async fn run_plain_json_fallback(
    http: &Client,
    runtime: &ModelRuntime,
    context: &mut ToolContext<'_>,
) -> Result<ToolLoopOutcome, (StatusCode, String)> {
    let visible_skills = skills_repo::list_skills(
        context.db,
        context.visible_user_ids.as_slice(),
        None,
        None,
        1000,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load skills for fallback failed: {err}")))?;
    let visible_agents = agents_repo::list_agents(
        context.db,
        context.visible_user_ids.as_slice(),
        Some(true),
        200,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load agents for fallback failed: {err}")))?;
    let visible_plugins = skills_repo::list_plugins_by_user_ids(
        context.db,
        context.visible_user_ids.as_slice(),
        300,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load skill plugins for fallback failed: {err}")))?;
    context.state.listed_skills = true;

    let response = request_chat_completion(
        http,
        runtime,
        &[
            json!({"role": "system", "content": build_plain_system_prompt()}),
            json!({
                "role": "user",
                "content": build_plain_user_prompt(
                    context.request,
                    visible_skills.as_slice(),
                    visible_agents.as_slice(),
                    visible_plugins.as_slice(),
                ),
            }),
        ],
        None,
    )
    .await?;

    let content = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| extract_message_text(message.get("content")))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            bad_gateway_error(format!(
                "agent builder fallback returned empty content: {}",
                response
            ))
        })?;

    let created_agent = create_agent_from_final_response(context, content.as_str()).await?;
    Ok(ToolLoopOutcome {
        created_agent,
        final_content: Some(content),
    })
}

async fn create_agent_from_final_response(
    context: &mut ToolContext<'_>,
    raw: &str,
) -> Result<Option<MemoryAgent>, (StatusCode, String)> {
    let Some(parsed) = parse_json_candidate(raw) else {
        return Ok(None);
    };

    if let Some(agent_id) = parsed
        .get("created_agent_id")
        .or_else(|| parsed.get("agent_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let existing = agents_repo::get_agent_by_id(context.db, agent_id)
            .await
            .map_err(|err| internal_error(format!("load created agent failed: {err}")))?;
        return Ok(existing);
    }

    let payload = parsed
        .get("create_memory_agent")
        .cloned()
        .or_else(|| parsed.get("agent").cloned())
        .unwrap_or(parsed);
    let Some(object) = payload.as_object() else {
        return Ok(None);
    };

    let create_req = build_create_agent_request(context, object, false).await?;
    let created = agents_repo::create_agent(context.db, create_req)
        .await
        .map_err(|err| {
            internal_error(format!("create agent from fallback response failed: {err}"))
        })?;
    Ok(Some(created))
}

async fn execute_tool_call(
    context: &mut ToolContext<'_>,
    tool_call: &ToolCall,
) -> Result<ToolExecution, String> {
    match tool_call.name.as_str() {
        "list_available_skills" => list_available_skills(context, &tool_call.arguments).await,
        "list_existing_agents" => list_existing_agents(context, &tool_call.arguments).await,
        "create_memory_agent" => create_memory_agent(context, &tool_call.arguments).await,
        other => Err(format!("unknown tool: {other}")),
    }
}

async fn list_available_skills(
    context: &mut ToolContext<'_>,
    arguments: &Value,
) -> Result<ToolExecution, String> {
    let query = optional_string(arguments, "query");
    let plugin_source = optional_string(arguments, "plugin_source");
    let limit = optional_i64(arguments, "limit")
        .unwrap_or(300)
        .clamp(1, 1000);

    let skills = skills_repo::list_skills(
        context.db,
        context.visible_user_ids.as_slice(),
        plugin_source.as_deref(),
        query.as_deref(),
        limit,
        0,
    )
    .await
    .map_err(|err| err.to_string())?;
    context.state.listed_skills = true;

    let items = skills
        .into_iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "description": skill.description,
                "plugin_source": skill.plugin_source,
                "source_path": skill.source_path,
                "version": skill.version,
                "content_preview": truncate_text(skill.content.as_str(), 500),
                "updated_at": skill.updated_at,
            })
        })
        .collect::<Vec<_>>();

    Ok(ToolExecution {
        payload: json!({
            "items": items,
            "count": items.len(),
        }),
        created_agent: None,
    })
}

async fn list_existing_agents(
    context: &mut ToolContext<'_>,
    arguments: &Value,
) -> Result<ToolExecution, String> {
    let enabled = arguments
        .get("enabled")
        .and_then(Value::as_bool)
        .or(Some(true));
    let query = optional_string(arguments, "query");
    let category = optional_string(arguments, "category");
    let limit = optional_i64(arguments, "limit").unwrap_or(40).clamp(1, 100);

    let mut agents = agents_repo::list_agents(
        context.db,
        context.visible_user_ids.as_slice(),
        enabled,
        limit,
        0,
    )
    .await
    .map_err(|err| err.to_string())?;

    if let Some(query_text) = query.as_deref() {
        let needle = query_text.to_lowercase();
        agents.retain(|agent| {
            [
                agent.name.as_str(),
                agent.description.as_deref().unwrap_or(""),
                agent.category.as_deref().unwrap_or(""),
                agent.role_definition.as_str(),
            ]
            .iter()
            .any(|field| field.to_lowercase().contains(needle.as_str()))
        });
    }

    if let Some(category_name) = category.as_deref() {
        agents.retain(|agent| {
            agent
                .category
                .as_deref()
                .map(|value| value.eq_ignore_ascii_case(category_name))
                .unwrap_or(false)
        });
    }

    let items = agents
        .into_iter()
        .map(|agent| {
            json!({
                "id": agent.id,
                "name": agent.name,
                "description": agent.description,
                "category": agent.category,
                "plugin_sources": agent.plugin_sources,
                "role_definition_preview": truncate_text(agent.role_definition.as_str(), 320),
                "skill_ids": agent.skill_ids,
                "default_skill_ids": agent.default_skill_ids,
                "enabled": agent.enabled,
                "updated_at": agent.updated_at,
            })
        })
        .collect::<Vec<_>>();

    Ok(ToolExecution {
        payload: json!({
            "items": items,
            "count": items.len(),
        }),
        created_agent: None,
    })
}

async fn create_memory_agent(
    context: &mut ToolContext<'_>,
    arguments: &Value,
) -> Result<ToolExecution, String> {
    if !context.state.listed_skills {
        return Err("must call list_available_skills before create_memory_agent".to_string());
    }
    if context.state.created_once {
        return Err("create_memory_agent can only succeed once".to_string());
    }

    let object = arguments
        .as_object()
        .ok_or_else(|| "create_memory_agent arguments must be an object".to_string())?;
    let create_req = build_create_agent_request(context, object, true)
        .await
        .map_err(|(_, err)| err)?;
    let created = agents_repo::create_agent(context.db, create_req)
        .await
        .map_err(|err| err.to_string())?;
    context.state.created_once = true;

    Ok(ToolExecution {
        payload: json!({
            "created": true,
            "agent": {
                "id": created.id,
                "name": created.name,
                "description": created.description,
                "category": created.category,
                "plugin_sources": created.plugin_sources,
                "skill_ids": created.skill_ids,
                "default_skill_ids": created.default_skill_ids,
                "enabled": created.enabled,
                "updated_at": created.updated_at,
            }
        }),
        created_agent: Some(created),
    })
}

async fn build_create_agent_request(
    context: &ToolContext<'_>,
    payload: &Map<String, Value>,
    enforce_skill_lookup: bool,
) -> Result<CreateMemoryAgentRequest, (StatusCode, String)> {
    if enforce_skill_lookup && !context.state.listed_skills {
        return Err(bad_request_error(
            "must call list_available_skills before create_memory_agent",
        ));
    }

    let name = context
        .request
        .name
        .clone()
        .or_else(|| payload_optional_string(payload, "name"))
        .unwrap_or_else(|| default_agent_name(context.request.requirement.as_str()));
    if name.trim().is_empty() {
        return Err(bad_request_error("name is required"));
    }

    let role_definition = context
        .request
        .role_definition
        .clone()
        .or_else(|| payload_optional_string(payload, "role_definition"))
        .unwrap_or_else(|| {
            default_role_definition(name.as_str(), context.request.requirement.as_str())
        });
    if role_definition.trim().is_empty() {
        return Err(bad_request_error("role_definition is required"));
    }

    let description = context
        .request
        .description
        .clone()
        .or_else(|| payload_optional_string(payload, "description"))
        .or_else(|| {
            Some(format!(
                "根据需求“{}”生成的智能体。",
                truncate_text(context.request.requirement.as_str(), 120)
            ))
        });

    let category = context
        .request
        .category
        .clone()
        .or_else(|| payload_optional_string(payload, "category"))
        .or_else(|| Some(infer_agent_category(context.request.requirement.as_str()).to_string()));

    let visible_skills = load_visible_skill_catalog(context).await?;
    let requested_inline_skills = payload
        .get("skills")
        .and_then(parse_skill_objects_from_value)
        .unwrap_or_default();
    let prompt_inline_skills =
        build_inline_skills_from_prompts(context.request.skill_prompts.as_deref())
            .unwrap_or_default();
    let allow_inline_skills = !prompt_inline_skills.is_empty() || visible_skills.items.is_empty();
    if !allow_inline_skills && !requested_inline_skills.is_empty() {
        return Err(bad_request_error(
            "当前技能中心已有可用技能，AI 创建智能体时禁止内联 skills，请改用 skill_ids",
        ));
    }

    let mut inline_skills = if allow_inline_skills {
        if !requested_inline_skills.is_empty() {
            requested_inline_skills
        } else {
            prompt_inline_skills
        }
    } else {
        Vec::new()
    };
    dedupe_skills(&mut inline_skills);

    let mut skill_ids = context
        .request
        .skill_ids
        .clone()
        .or_else(|| {
            payload
                .get("skill_ids")
                .and_then(parse_string_array_from_value)
        })
        .unwrap_or_default();
    dedupe_strings(&mut skill_ids);

    let mut default_skill_ids = context
        .request
        .default_skill_ids
        .clone()
        .or_else(|| {
            payload
                .get("default_skill_ids")
                .and_then(parse_string_array_from_value)
        })
        .unwrap_or_default();
    dedupe_strings(&mut default_skill_ids);

    let inline_skill_ids = inline_skills
        .iter()
        .map(|skill| skill.id.clone())
        .collect::<Vec<_>>();
    if skill_ids.is_empty() && !inline_skill_ids.is_empty() {
        skill_ids = inline_skill_ids.clone();
    }
    if default_skill_ids.is_empty() {
        default_skill_ids = if !skill_ids.is_empty() {
            skill_ids.clone()
        } else {
            inline_skill_ids.clone()
        };
    }

    let mut plugin_sources = context
        .request
        .plugin_sources
        .clone()
        .or_else(|| {
            payload
                .get("plugin_sources")
                .and_then(parse_string_array_from_value)
        })
        .unwrap_or_default();
    dedupe_strings(&mut plugin_sources);

    for skill in visible_skills
        .items
        .iter()
        .filter(|skill| skill_ids.iter().any(|item| item == &skill.id))
    {
        if plugin_sources
            .iter()
            .any(|item| item == &skill.plugin_source)
        {
            continue;
        }
        plugin_sources.push(skill.plugin_source.clone());
    }

    validate_skill_ids(
        &visible_skills,
        skill_ids.as_slice(),
        default_skill_ids.as_slice(),
        inline_skill_ids.as_slice(),
    )
    .await?;

    let enabled = context.request.enabled.unwrap_or_else(|| {
        payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    });

    let mcp_policy = resolve_mcp_policy(context.request, payload);
    let project_policy = resolve_project_policy(context.request, payload);

    Ok(CreateMemoryAgentRequest {
        user_id: context.request.scope_user_id.clone(),
        name,
        description,
        category,
        role_definition,
        plugin_sources: if plugin_sources.is_empty() {
            None
        } else {
            Some(plugin_sources)
        },
        skills: if inline_skills.is_empty() {
            None
        } else {
            Some(inline_skills)
        },
        skill_ids: if skill_ids.is_empty() {
            None
        } else {
            Some(skill_ids)
        },
        default_skill_ids: if default_skill_ids.is_empty() {
            None
        } else {
            Some(default_skill_ids)
        },
        mcp_policy,
        project_policy,
        enabled: Some(enabled),
    })
}

async fn validate_skill_ids(
    visible_skills: &VisibleSkillCatalog,
    skill_ids: &[String],
    default_skill_ids: &[String],
    inline_skill_ids: &[String],
) -> Result<(), (StatusCode, String)> {
    let inline_ids = inline_skill_ids
        .iter()
        .map(|item| item.as_str())
        .collect::<HashSet<_>>();
    let mut missing = Vec::new();
    for skill_id in skill_ids.iter().chain(default_skill_ids.iter()) {
        if visible_skills.ids.contains(skill_id) || inline_ids.contains(skill_id.as_str()) {
            continue;
        }
        if missing.iter().any(|existing: &String| existing == skill_id) {
            continue;
        }
        missing.push(skill_id.clone());
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(bad_request_error(format!(
            "存在未安装的 skill_id: {}",
            missing.join(", ")
        )))
    }
}

async fn resolve_model_runtime(
    db: &Db,
    config: &AppConfig,
    request: &NormalizedRequest,
) -> Result<ModelRuntime, (StatusCode, String)> {
    if let Some(model_config_id) = request.model_config_id.as_deref() {
        let item = configs::get_model_config_by_id(db, model_config_id)
            .await
            .map_err(|err| internal_error(format!("load selected model config failed: {err}")))?;
        let Some(item) = item else {
            return Err(bad_request_error("所选创建模型不存在"));
        };
        if item.user_id != request.scope_user_id {
            return Err(bad_request_error("所选创建模型不属于当前作用域账号"));
        }
        if item.enabled != 1 {
            return Err(bad_request_error("所选创建模型未启用"));
        }
        return model_runtime_from_model_config(config, &item);
    }

    if let Some(value) = request.ai_model_config.as_ref() {
        return model_runtime_from_value(config, value);
    }

    let enabled_items = configs::list_model_configs(db, request.scope_user_id.as_str())
        .await
        .map_err(|err| internal_error(format!("load model configs failed: {err}")))?
        .into_iter()
        .filter(|item| item.enabled == 1)
        .collect::<Vec<_>>();
    if enabled_items.len() == 1 {
        return model_runtime_from_model_config(config, &enabled_items[0]);
    }
    if enabled_items.len() > 1 {
        return Err(bad_request_error(
            "已配置多个启用模型，请传 model_config_id 指定用于创建智能体的模型",
        ));
    }

    let api_key = config
        .openai_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            bad_request_error(
                "请先在 Memory 的模型配置中启用一个可用模型，或配置 MEMORY_SERVER_OPENAI_API_KEY",
            )
        })?;

    Ok(ModelRuntime {
        provider: "gpt".to_string(),
        model: normalize_model_name(config.openai_model.as_str()),
        base_url: normalize_base_url(config.openai_base_url.as_str()),
        api_key: api_key.to_string(),
        temperature: config.openai_temperature.clamp(0.0, 2.0),
        request_timeout_secs: config.ai_request_timeout_secs,
        supports_responses: false,
    })
}

fn model_runtime_from_model_config(
    config: &AppConfig,
    item: &AiModelConfig,
) -> Result<ModelRuntime, (StatusCode, String)> {
    let api_key = item
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(config
            .openai_api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()))
        .ok_or_else(|| bad_request_error(format!("模型 {} 未配置 api_key", item.name)))?;

    Ok(ModelRuntime {
        provider: normalize_provider(item.provider.as_str()),
        model: normalize_model_name(item.model.as_str()),
        base_url: item
            .base_url
            .as_deref()
            .map(normalize_base_url)
            .unwrap_or_else(|| normalize_base_url(config.openai_base_url.as_str())),
        api_key: api_key.to_string(),
        temperature: item
            .temperature
            .unwrap_or(config.openai_temperature)
            .clamp(0.0, 2.0),
        request_timeout_secs: config.ai_request_timeout_secs,
        supports_responses: item.supports_responses == 1,
    })
}

fn model_runtime_from_value(
    config: &AppConfig,
    value: &Value,
) -> Result<ModelRuntime, (StatusCode, String)> {
    let provider = value
        .get("provider")
        .and_then(Value::as_str)
        .map(normalize_provider)
        .unwrap_or_else(|| "gpt".to_string());
    let model = value
        .get("model")
        .or_else(|| value.get("model_name"))
        .and_then(Value::as_str)
        .map(normalize_model_name)
        .unwrap_or_else(|| normalize_model_name(config.openai_model.as_str()));
    let base_url = value
        .get("base_url")
        .and_then(Value::as_str)
        .map(normalize_base_url)
        .unwrap_or_else(|| normalize_base_url(config.openai_base_url.as_str()));
    let api_key = value
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .or(config
            .openai_api_key
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty()))
        .ok_or_else(|| bad_request_error("显式模型配置缺少 api_key"))?;
    let temperature = value
        .get("temperature")
        .and_then(Value::as_f64)
        .unwrap_or(config.openai_temperature)
        .clamp(0.0, 2.0);
    let supports_responses = value
        .get("supports_responses")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Ok(ModelRuntime {
        provider,
        model,
        base_url,
        api_key: api_key.to_string(),
        temperature,
        request_timeout_secs: config.ai_request_timeout_secs,
        supports_responses,
    })
}

async fn request_chat_completion(
    http: &Client,
    runtime: &ModelRuntime,
    messages: &[Value],
    tools: Option<&[Value]>,
) -> Result<Value, (StatusCode, String)> {
    if runtime.supports_responses {
        return request_responses_completion(http, runtime, messages, tools).await;
    }

    request_chat_completions(http, runtime, messages, tools).await
}

async fn request_chat_completions(
    http: &Client,
    runtime: &ModelRuntime,
    messages: &[Value],
    tools: Option<&[Value]>,
) -> Result<Value, (StatusCode, String)> {
    let mut body = json!({
        "model": runtime.model,
        "temperature": runtime.temperature,
        "max_tokens": 2400,
        "stream": true,
        "stream_options": {"include_usage": true},
        "messages": messages,
    });

    if let Some(tool_items) = tools {
        body["tools"] = Value::Array(tool_items.to_vec());
        body["tool_choice"] = Value::String("auto".to_string());
    }

    let endpoint = build_chat_completion_endpoint(runtime.base_url.as_str());
    let response = http
        .post(endpoint.as_str())
        .bearer_auth(runtime.api_key.as_str())
        .header("Content-Type", "application/json")
        .header("Connection", "close")
        .json(&body)
        .send()
        .await
        .map_err(|err| bad_gateway_error(format_transport_error(runtime, endpoint.as_str(), &err)))?;

    if !response.status().is_success() {
        let status = response.status();
        let payload = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "agent builder ai request status={} provider={} model={} endpoint={} body={}",
                status,
                runtime.provider,
                runtime.model,
                endpoint,
                payload
            ),
        ));
    }

    let events = read_sse_json_events(response).await?;
    aggregate_chat_completions_stream(events.as_slice())
}

async fn request_responses_completion(
    http: &Client,
    runtime: &ModelRuntime,
    messages: &[Value],
    tools: Option<&[Value]>,
) -> Result<Value, (StatusCode, String)> {
    let mut body = json!({
        "model": runtime.model,
        "temperature": runtime.temperature,
        "max_output_tokens": 2400,
        "stream": true,
        "input": build_responses_input_from_messages(messages),
    });

    if let Some(tool_items) = tools {
        body["tools"] = Value::Array(tool_items.to_vec());
        body["tool_choice"] = Value::String("auto".to_string());
    }

    let endpoint = build_responses_endpoint(runtime.base_url.as_str());
    let response = http
        .post(endpoint.as_str())
        .bearer_auth(runtime.api_key.as_str())
        .header("Content-Type", "application/json")
        .header("Connection", "close")
        .json(&body)
        .send()
        .await
        .map_err(|err| bad_gateway_error(format_transport_error(runtime, endpoint.as_str(), &err)))?;

    if !response.status().is_success() {
        let status = response.status();
        let payload = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "agent builder ai request status={} provider={} model={} endpoint={} body={}",
                status,
                runtime.provider,
                runtime.model,
                endpoint,
                payload
            ),
        ));
    }

    let events = read_sse_json_events(response).await?;
    let payload = aggregate_responses_stream(events.as_slice())?;

    Ok(adapt_responses_to_chat_completion(payload))
}

async fn read_sse_json_events(
    mut response: reqwest::Response,
) -> Result<Vec<Value>, (StatusCode, String)> {
    let mut buffer = String::new();
    let mut events: Vec<Value> = Vec::new();

    while let Some(bytes) = response
        .chunk()
        .await
        .map_err(|err| bad_gateway_error(format!("agent builder ai stream read failed: {err}")))?
    {
        let text = String::from_utf8_lossy(&bytes).to_string();
        buffer.push_str(text.as_str());
        events.extend(drain_sse_json_events(&mut buffer));
    }

    flush_sse_tail_events(&mut buffer, &mut events);

    if events.is_empty() {
        return Err(bad_gateway_error(
            "agent builder ai stream parse failed: no JSON events found",
        ));
    }

    Ok(events)
}

fn drain_sse_json_events(buffer: &mut String) -> Vec<Value> {
    let mut events = Vec::new();

    while let Some(idx) = buffer.find("\n\n") {
        let packet = buffer[..idx].to_string();
        *buffer = buffer[idx + 2..].to_string();

        for line in packet.lines() {
            let normalized = line.trim();
            if !normalized.starts_with("data:") {
                continue;
            }
            let data = normalized.trim_start_matches("data:").trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(data) {
                events.push(value);
            }
        }
    }

    events
}

fn flush_sse_tail_events(buffer: &mut String, events: &mut Vec<Value>) {
    if buffer.trim().is_empty() {
        return;
    }

    if buffer.contains("data:") {
        if !buffer.ends_with("\n\n") {
            buffer.push_str("\n\n");
        }
        events.extend(drain_sse_json_events(buffer));
    }

    let tail = buffer.trim();
    if tail.is_empty() {
        return;
    }

    if let Ok(value) = serde_json::from_str::<Value>(tail) {
        emit_tail_json_value(value, events);
    }
    buffer.clear();
}

fn emit_tail_json_value(value: Value, events: &mut Vec<Value>) {
    if let Some(items) = value.as_array() {
        for item in items {
            if item.is_object() {
                events.push(item.clone());
            }
        }
        return;
    }
    if value.is_object() {
        events.push(value);
    }
}

fn aggregate_chat_completions_stream(events: &[Value]) -> Result<Value, (StatusCode, String)> {
    #[derive(Default, Clone)]
    struct ToolCallAccumulator {
        id: Option<String>,
        name: Option<String>,
        arguments: String,
    }

    let mut content = String::new();
    let mut finish_reason: Option<String> = None;
    let mut usage: Option<Value> = None;
    let mut tool_calls: Vec<ToolCallAccumulator> = Vec::new();

    for event in events {
        if let Some(value_usage) = event.get("usage") {
            usage = Some(value_usage.clone());
        }

        let Some(choices) = event.get("choices").and_then(Value::as_array) else {
            continue;
        };

        for choice in choices {
            if let Some(reason) = choice
                .get("finish_reason")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                finish_reason = Some(reason.to_string());
            }

            if let Some(delta) = choice.get("delta") {
                if let Some(text) = delta.get("content").and_then(Value::as_str) {
                    content.push_str(text);
                } else if let Some(parts) = delta.get("content").and_then(Value::as_array) {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(Value::as_str) {
                            content.push_str(text);
                        }
                    }
                }

                if let Some(items) = delta.get("tool_calls").and_then(Value::as_array) {
                    for item in items {
                        let index = item
                            .get("index")
                            .and_then(Value::as_u64)
                            .map(|value| value as usize)
                            .unwrap_or(tool_calls.len());
                        while tool_calls.len() <= index {
                            tool_calls.push(ToolCallAccumulator::default());
                        }
                        if let Some(id) = item
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            tool_calls[index].id = Some(id.to_string());
                        }
                        if let Some(function) = item.get("function") {
                            if let Some(name) = function
                                .get("name")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                            {
                                tool_calls[index].name = Some(name.to_string());
                            }
                            if let Some(arguments) = function.get("arguments").and_then(Value::as_str)
                            {
                                tool_calls[index].arguments.push_str(arguments);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut message = Map::new();
    if content.trim().is_empty() {
        message.insert("content".to_string(), Value::Null);
    } else {
        message.insert("content".to_string(), Value::String(content));
    }

    let normalized_tool_calls = tool_calls
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let name = item.name.as_deref()?.trim();
            if name.is_empty() {
                return None;
            }
            let id = item
                .id
                .clone()
                .unwrap_or_else(|| format!("call_{}", index + 1));
            let arguments = if item.arguments.trim().is_empty() {
                "{}".to_string()
            } else {
                item.arguments.clone()
            };
            Some(json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": arguments,
                }
            }))
        })
        .collect::<Vec<_>>();

    if !normalized_tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(normalized_tool_calls));
    }

    let mut out = json!({
        "choices": [
            {
                "message": Value::Object(message),
                "finish_reason": finish_reason.unwrap_or_else(|| "stop".to_string()),
            }
        ]
    });
    if let Some(value_usage) = usage {
        out["usage"] = value_usage;
    }
    Ok(out)
}

fn aggregate_responses_stream(events: &[Value]) -> Result<Value, (StatusCode, String)> {
    let mut completed_response: Option<Value> = None;
    let mut response_template: Option<Value> = None;
    let mut output_items: Vec<Value> = Vec::new();
    let mut output_text = String::new();
    let mut reasoning_text = String::new();

    for event in events {
        if event.get("object").and_then(Value::as_str) == Some("response") {
            completed_response = Some(event.clone());
        }

        if let Some(response) = event.get("response") {
            response_template = Some(response.clone());
            let event_type = event
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if event_type == "response.completed" || event_type == "response.failed" {
                completed_response = Some(response.clone());
            }
        }

        let event_type = event.get("type").and_then(Value::as_str).unwrap_or_default();
        if event_type == "response.output_text.delta" {
            if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                output_text.push_str(delta);
            }
        } else if event_type == "response.reasoning.delta" {
            if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                reasoning_text.push_str(delta);
            }
        } else if event_type == "response.reasoning.done" {
            if let Some(text) = event.get("text").and_then(Value::as_str) {
                reasoning_text = text.to_string();
            }
        } else if event_type == "response.output_item.done" {
            if let Some(item) = event.get("item") {
                output_items.push(item.clone());
            }
        }
    }

    if let Some(response) = completed_response {
        return Ok(response);
    }

    let mut response = response_template
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    if output_items.is_empty() && !output_text.trim().is_empty() {
        output_items.push(json!({
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [
                {
                    "type": "output_text",
                    "text": output_text.clone(),
                }
            ],
        }));
    }

    if !output_items.is_empty() {
        response.insert("output".to_string(), Value::Array(output_items));
    }
    if !output_text.trim().is_empty() {
        response.insert("output_text".to_string(), Value::String(output_text));
    }
    if !reasoning_text.trim().is_empty() {
        response.insert("reasoning".to_string(), Value::String(reasoning_text));
    }
    if !response.contains_key("status") {
        response.insert("status".to_string(), Value::String("completed".to_string()));
    }
    if !response.contains_key("object") {
        response.insert("object".to_string(), Value::String("response".to_string()));
    }

    if response.is_empty() {
        return Err(bad_gateway_error(
            "agent builder ai stream parse failed: no response payload assembled",
        ));
    }

    Ok(Value::Object(response))
}

fn build_responses_input_from_messages(messages: &[Value]) -> Value {
    let mut items = Vec::new();

    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if role.is_empty() {
            continue;
        }

        if role == "tool" {
            let call_id = message
                .get("tool_call_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if call_id.is_empty() {
                continue;
            }

            let raw_output = message.get("content").cloned().unwrap_or(Value::Null);
            let output = if let Some(text) = raw_output.as_str() {
                parse_json_candidate(text).unwrap_or_else(|| Value::String(text.to_string()))
            } else {
                raw_output
            };

            items.push(json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": output,
            }));
            continue;
        }

        if role == "assistant" {
            if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
                for call in tool_calls {
                    let call_id = call
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    let function = call.get("function");
                    let name = function
                        .and_then(|item| item.get("name"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    if call_id.is_empty() || name.is_empty() {
                        continue;
                    }

                    let arguments = function
                        .and_then(|item| item.get("arguments"))
                        .cloned()
                        .unwrap_or_else(|| Value::String("{}".to_string()));
                    let arguments = arguments
                        .as_str()
                        .map(|raw| raw.to_string())
                        .unwrap_or_else(|| arguments.to_string());

                    items.push(json!({
                        "type": "function_call",
                        "call_id": call_id,
                        "name": name,
                        "arguments": arguments,
                    }));
                }
            }
        }

        if let Some(text) = extract_message_text(message.get("content")) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                items.push(json!({
                    "type": "message",
                    "role": role,
                    "content": [
                        {
                            "type": "input_text",
                            "text": trimmed,
                        }
                    ],
                }));
            }
        }
    }

    Value::Array(items)
}

fn adapt_responses_to_chat_completion(value: Value) -> Value {
    let content = extract_responses_output_text(&value);
    let tool_calls = extract_responses_tool_calls(&value);
    let finish_reason = value
        .get("status")
        .and_then(Value::as_str)
        .map(|status| {
            if status == "completed" {
                "stop".to_string()
            } else {
                status.to_string()
            }
        })
        .unwrap_or_else(|| "stop".to_string());

    let mut message = Map::new();
    message.insert(
        "content".to_string(),
        content.map(Value::String).unwrap_or(Value::Null),
    );
    if !tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), Value::Array(tool_calls));
    }

    json!({
        "choices": [
            {
                "message": Value::Object(message),
                "finish_reason": finish_reason,
            }
        ]
    })
}

fn extract_responses_output_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let mut parts = Vec::new();
    let Some(items) = value.get("output").and_then(Value::as_array) else {
        return None;
    };

    for item in items {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
        if item_type == "message" {
            if let Some(contents) = item.get("content").and_then(Value::as_array) {
                for content in contents {
                    let content_type = content.get("type").and_then(Value::as_str).unwrap_or("");
                    if content_type == "output_text"
                        || content_type == "input_text"
                        || content_type == "text"
                    {
                        if let Some(text) = content.get("text").and_then(Value::as_str) {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                parts.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
            continue;
        }

        if (item_type == "output_text" || item_type == "input_text" || item_type == "text")
            && item.get("text").and_then(Value::as_str).is_some()
        {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn extract_responses_tool_calls(value: &Value) -> Vec<Value> {
    let Some(items) = value.get("output").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            continue;
        }

        let call_id = item
            .get("call_id")
            .and_then(Value::as_str)
            .or_else(|| item.get("id").and_then(Value::as_str))
            .map(str::trim)
            .unwrap_or("");
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if call_id.is_empty() || name.is_empty() {
            continue;
        }

        let arguments = item
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| Value::String("{}".to_string()));
        let arguments = arguments
            .as_str()
            .map(|raw| raw.to_string())
            .unwrap_or_else(|| arguments.to_string());

        out.push(json!({
            "id": call_id,
            "type": "function",
            "function": {
                "name": name,
                "arguments": arguments,
            }
        }));
    }

    out
}

fn build_agent_builder_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "list_available_skills",
                "description": "List installed skills from Memory skill center. Call this before creating an agent.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "plugin_source": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 1000 }
                    },
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_existing_agents",
                "description": "List visible existing agents as design references.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "enabled": { "type": "boolean" },
                        "query": { "type": "string" },
                        "category": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                    },
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "create_memory_agent",
                "description": "Create the final Memory agent. You must call list_available_skills first, must not invent missing skill_id values, and must not send inline skills unless the skill center is empty or the user explicitly provided skill_prompts.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "role_definition": { "type": "string" },
                        "description": { "type": "string" },
                        "category": { "type": "string" },
                        "enabled": { "type": "boolean" },
                        "plugin_sources": { "type": "array", "items": { "type": "string" } },
                        "skill_ids": { "type": "array", "items": { "type": "string" } },
                        "default_skill_ids": { "type": "array", "items": { "type": "string" } },
                        "skills": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "name": { "type": "string" },
                                    "content": { "type": "string" }
                                },
                                "required": ["id", "name", "content"],
                                "additionalProperties": false
                            }
                        },
                        "mcp_policy": { "type": "object" },
                        "project_policy": { "type": "object" }
                    },
                    "additionalProperties": false
                }
            }
        }),
    ]
}

fn build_tool_loop_system_prompt() -> String {
    [
        "你是 Memory 服务内部的 AI 智能体创建器。",
        "你的任务是根据用户需求，先看技能，再决定要复用哪些 skill_ids，必要时参考现有 agent，最后创建一个新的 Memory agent。",
        "硬性规则：",
        "1. 必须先调用 list_available_skills。",
        "2. 如有必要再调用 list_existing_agents。",
        "3. 严禁虚构不存在的 skill_id。",
        "4. plugin_sources 表示能力包范围，skill_ids 表示具体技能引用；最终输出应尽量同时包含两者。",
        "5. 只有技能中心为空，或者用户显式提供了 skill_prompts 时，才允许创建内联 skills。",
        "6. 最终必须调用 create_memory_agent，且只能成功一次。",
        "7. 用户显式给出的 name/category/description/role_definition/plugin_sources/skill_ids/default_skill_ids/mcp/project 约束必须优先尊重。",
        "8. 最终回复必须是紧凑 JSON，不要输出 markdown。",
    ]
    .join("\n")
}

fn build_tool_loop_user_prompt(
    request: &NormalizedRequest,
    skills: &[MemorySkill],
    agents: &[MemoryAgent],
    plugins: &[MemorySkillPlugin],
) -> String {
    let skill_index = build_skill_index(skills);
    let agent_index = build_agent_index(agents);
    let plugin_index = build_plugin_index(plugins);
    let payload = json!({
        "target_user_id": request.scope_user_id,
        "requirement": request.requirement,
        "explicit_name": request.name,
        "explicit_category": request.category,
        "explicit_description": request.description,
        "explicit_role_definition": request.role_definition,
        "preferred_plugin_sources": request.plugin_sources,
        "preferred_skill_ids": request.skill_ids,
        "preferred_default_skill_ids": request.default_skill_ids,
        "skill_prompts": request.skill_prompts,
        "enabled": request.enabled,
        "mcp_policy": {
            "enabled": request.mcp_enabled,
            "enabled_mcp_ids": request.enabled_mcp_ids,
        },
        "project_policy": {
            "project_id": request.project_id,
            "project_root": request.project_root,
        },
        "skill_selection_policy": {
            "prefer_installed_skill_ids": true,
            "allow_inline_skills_only_when_skill_center_empty_or_explicit_prompts": true,
        },
        "visible_skill_plugins": plugin_index,
        "visible_skills": skill_index,
        "reference_agents": agent_index,
    });

    format!(
        "请根据下面的输入创建一个新的 Memory agent。先看技能，再创建。\n\n{}",
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
    )
}

fn build_plain_system_prompt() -> String {
    [
        "你是 Memory 服务内部的 AI 智能体创建器。",
        "当前模型不支持工具调用，下面会直接给你可用技能和参考 agent。",
        "请输出一个紧凑 JSON 对象，字段遵循 create_memory_agent 的参数结构。",
        "规则：优先输出 plugin_sources + 已安装 skill_ids；只有当技能中心为空，或者用户显式提供了 skill_prompts 时，才允许输出 inline skills；不要输出 markdown。",
    ]
    .join("\n")
}

fn build_plain_user_prompt(
    request: &NormalizedRequest,
    skills: &[MemorySkill],
    agents: &[MemoryAgent],
    plugins: &[MemorySkillPlugin],
) -> String {
    let skills_view = build_skill_index(skills);
    let agents_view = build_agent_index(agents);
    let plugins_view = build_plugin_index(plugins);
    let payload = json!({
        "request": {
            "target_user_id": request.scope_user_id,
            "requirement": request.requirement,
            "explicit_name": request.name,
            "explicit_category": request.category,
            "explicit_description": request.description,
            "explicit_role_definition": request.role_definition,
            "preferred_plugin_sources": request.plugin_sources,
            "preferred_skill_ids": request.skill_ids,
            "preferred_default_skill_ids": request.default_skill_ids,
            "skill_prompts": request.skill_prompts,
            "enabled": request.enabled,
            "mcp_policy": {
                "enabled": request.mcp_enabled,
                "enabled_mcp_ids": request.enabled_mcp_ids,
            },
            "project_policy": {
                "project_id": request.project_id,
                "project_root": request.project_root,
            }
        },
        "visible_skill_plugins": plugins_view,
        "visible_skills": skills_view,
        "reference_agents": agents_view,
        "skill_selection_policy": {
            "visible_skill_count": skills_view.len(),
            "allow_inline_skills_only_when_skill_center_empty_or_explicit_prompts": true,
            "prefer_installed_skill_ids": true,
        }
    });

    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
}

fn build_skill_index(skills: &[MemorySkill]) -> Vec<Value> {
    skills
        .iter()
        .map(|skill| {
            json!({
                "id": skill.id,
                "name": skill.name,
                "description": skill.description.as_deref().map(|value| truncate_text(value, 180)),
                "plugin_source": skill.plugin_source,
                "source_path": skill.source_path,
                "content_preview": truncate_text(skill.content.as_str(), 220),
            })
        })
        .collect::<Vec<_>>()
}

fn build_agent_index(agents: &[MemoryAgent]) -> Vec<Value> {
    agents
        .iter()
        .map(|agent| {
            json!({
                "id": agent.id,
                "name": agent.name,
                "category": agent.category,
                "description": agent.description.as_deref().map(|value| truncate_text(value, 160)),
                "plugin_sources": agent.plugin_sources,
                "skill_ids": agent.skill_ids,
                "default_skill_ids": agent.default_skill_ids,
                "role_definition_preview": truncate_text(agent.role_definition.as_str(), 220),
            })
        })
        .collect::<Vec<_>>()
}

fn build_plugin_index(plugins: &[MemorySkillPlugin]) -> Vec<Value> {
    plugins
        .iter()
        .map(|plugin| {
            json!({
                "id": plugin.id,
                "source": plugin.source,
                "name": plugin.name,
                "category": plugin.category,
                "description": plugin.description.as_deref().map(|value| truncate_text(value, 160)),
                "installed": plugin.installed,
                "discoverable_skills": plugin.discoverable_skills,
                "installed_skill_count": plugin.installed_skill_count,
            })
        })
        .collect::<Vec<_>>()
}

fn parse_tool_calls(value: Option<&Value>) -> Vec<ToolCall> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?.trim().to_string();
            let function = item.get("function")?;
            let name = function.get("name")?.as_str()?.trim().to_string();
            if id.is_empty() || name.is_empty() {
                return None;
            }
            let arguments = match function.get("arguments") {
                Some(Value::String(raw)) => parse_json_candidate(raw).unwrap_or_else(|| json!({})),
                Some(other) => other.clone(),
                None => json!({}),
            };
            Some(ToolCall {
                id,
                name,
                arguments,
                raw: item.clone(),
            })
        })
        .collect()
}

fn extract_message_text(value: Option<&Value>) -> Option<String> {
    let content = value?;
    match content {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| item.get("content").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

fn parse_json_candidate(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }
    if let Some(inner) = extract_json_code_block(trimmed) {
        if let Ok(value) = serde_json::from_str::<Value>(inner.as_str()) {
            return Some(value);
        }
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if start >= end {
        return None;
    }
    serde_json::from_str::<Value>(&trimmed[start..=end]).ok()
}

fn extract_json_code_block(raw: &str) -> Option<String> {
    let stripped = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))?;
    let end = stripped.rfind("```")?;
    let inner = stripped[..end].trim();
    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

fn normalize_required_text(
    value: Option<String>,
    field: &str,
) -> Result<String, (StatusCode, String)> {
    normalize_optional_text(value).ok_or_else(|| bad_request_error(format!("{field} is required")))
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_optional_string_array(value: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut items = value
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    dedupe_strings(&mut items);
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

fn optional_string(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn payload_optional_string(payload: &Map<String, Value>, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn optional_i64(value: &Value, field: &str) -> Option<i64> {
    value.get(field).and_then(Value::as_i64)
}

fn parse_string_array_from_value(value: &Value) -> Option<Vec<String>> {
    let items = value.as_array()?;
    let mut out = items
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    dedupe_strings(&mut out);
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn parse_skill_objects_from_value(value: &Value) -> Option<Vec<MemoryAgentSkill>> {
    let items = value.as_array()?;
    let mut out = Vec::new();
    for item in items {
        let obj = item.as_object()?;
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)?;
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)?;
        let content = obj
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)?;
        out.push(MemoryAgentSkill { id, name, content });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn build_inline_skills_from_prompts(prompts: Option<&[String]>) -> Option<Vec<MemoryAgentSkill>> {
    let prompts = prompts?;
    let mut out = Vec::new();
    for (index, prompt) in prompts.iter().enumerate() {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(MemoryAgentSkill {
            id: format!("inline_skill_{}", index + 1),
            name: format!("Inline Skill {}", index + 1),
            content: trimmed.to_string(),
        });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

async fn load_visible_skill_catalog(
    context: &ToolContext<'_>,
) -> Result<VisibleSkillCatalog, (StatusCode, String)> {
    let items = skills_repo::list_skills(
        context.db,
        context.visible_user_ids.as_slice(),
        None,
        None,
        1000,
        0,
    )
    .await
    .map_err(|err| internal_error(format!("load visible skills failed: {err}")))?;
    let ids = items
        .iter()
        .map(|skill| skill.id.clone())
        .collect::<HashSet<_>>();
    Ok(VisibleSkillCatalog { items, ids })
}

fn dedupe_strings(items: &mut Vec<String>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

fn dedupe_skills(items: &mut Vec<MemoryAgentSkill>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.id.clone()));
}

fn resolve_mcp_policy(request: &NormalizedRequest, payload: &Map<String, Value>) -> Option<Value> {
    if request.mcp_enabled.is_some() || request.enabled_mcp_ids.is_some() {
        return Some(json!({
            "enabled": request.mcp_enabled.unwrap_or(true),
            "enabled_mcp_ids": request.enabled_mcp_ids.clone().unwrap_or_default(),
        }));
    }

    if let Some(value) = payload
        .get("mcp_policy")
        .and_then(normalize_mcp_policy_value)
    {
        return Some(value);
    }

    Some(json!({
        "enabled": true,
        "enabled_mcp_ids": [],
    }))
}

fn normalize_mcp_policy_value(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    Some(json!({
        "enabled": obj.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        "enabled_mcp_ids": obj
            .get("enabled_mcp_ids")
            .and_then(parse_string_array_from_value)
            .unwrap_or_default(),
    }))
}

fn resolve_project_policy(
    request: &NormalizedRequest,
    payload: &Map<String, Value>,
) -> Option<Value> {
    if request.project_id.is_some() || request.project_root.is_some() {
        return Some(json!({
            "project_id": request.project_id,
            "project_root": request.project_root,
        }));
    }

    payload
        .get("project_policy")
        .and_then(normalize_project_policy_value)
}

fn normalize_project_policy_value(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    let project_id = obj
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned);
    let project_root = obj
        .get("project_root")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned);
    if project_id.is_none() && project_root.is_none() {
        None
    } else {
        Some(json!({
            "project_id": project_id,
            "project_root": project_root,
        }))
    }
}

fn resolve_visible_user_ids(scope_user_id: &str) -> Vec<String> {
    let normalized = scope_user_id.trim();
    if normalized.is_empty() || normalized == ADMIN_USER_ID {
        return vec![ADMIN_USER_ID.to_string()];
    }
    vec![normalized.to_string(), ADMIN_USER_ID.to_string()]
}

fn normalize_provider(provider: &str) -> String {
    let normalized = provider.trim().to_lowercase();
    if normalized.is_empty() || normalized == "openai" {
        "gpt".to_string()
    } else {
        normalized
    }
}

fn normalize_base_url(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.is_empty() {
        "https://api.openai.com/v1".to_string()
    } else {
        normalized.to_string()
    }
}

fn classify_transport_error(err: &reqwest::Error) -> &'static str {
    if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_request() {
        "request"
    } else if err.is_body() {
        "body"
    } else if err.is_decode() {
        "decode"
    } else {
        "other"
    }
}

fn error_source_chain(err: &reqwest::Error) -> String {
    let mut parts = Vec::new();
    let mut current = err.source();
    while let Some(source) = current {
        parts.push(source.to_string());
        current = source.source();
    }
    parts.join(" | ")
}

fn format_transport_error(runtime: &ModelRuntime, endpoint: &str, err: &reqwest::Error) -> String {
    let kind = classify_transport_error(err);
    let sources = error_source_chain(err);
    let source_suffix = if sources.is_empty() {
        String::new()
    } else {
        format!(" source_chain={sources}")
    };
    let message = format!(
        "agent builder ai transport failed kind={} provider={} model={} endpoint={} timeout_secs={} detail={}{}",
        kind,
        runtime.provider,
        runtime.model,
        endpoint,
        runtime.request_timeout_secs,
        err,
        source_suffix
    );
    warn!("[AGENT_BUILDER] {}", message);
    message
}

fn normalize_model_name(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        "gpt-4o-mini".to_string()
    } else {
        trimmed.to_string()
    }
}

fn build_chat_completion_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/chat/completions") {
        normalized
    } else {
        format!("{}/chat/completions", normalized)
    }
}

fn build_responses_endpoint(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/responses") {
        normalized
    } else {
        format!("{}/responses", normalized)
    }
}

fn infer_agent_category(requirement: &str) -> &'static str {
    if contains_any(requirement, &["代码", "开发", "编程", "debug", "code"]) {
        "engineering"
    } else if contains_any(requirement, &["产品", "需求", "roadmap", "prd"]) {
        "product"
    } else if contains_any(requirement, &["运营", "增长", "营销", "campaign"]) {
        "growth"
    } else {
        "general"
    }
}

fn default_agent_name(requirement: &str) -> String {
    match infer_agent_category(requirement) {
        "engineering" => "研发协作助手".to_string(),
        "product" => "产品分析助手".to_string(),
        "growth" => "增长运营助手".to_string(),
        _ => "通用业务助手".to_string(),
    }
}

fn default_role_definition(name: &str, requirement: &str) -> String {
    format!(
        "你是{name}。你的目标是围绕“{}”为用户提供清晰、可执行、可验证的行动建议，并在信息不足时优先澄清约束。",
        truncate_text(requirement, 180)
    )
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    let lowered = text.to_lowercase();
    patterns
        .iter()
        .any(|pattern| lowered.contains(&pattern.to_lowercase()))
}

fn truncate_text(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let mut out = raw.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn is_tooling_unsupported(detail: &str) -> bool {
    let lowered = detail.to_lowercase();
    (lowered.contains("tool") || lowered.contains("function"))
        && (lowered.contains("unsupported")
            || lowered.contains("unknown parameter")
            || lowered.contains("not allowed")
            || lowered.contains("invalid param"))
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
