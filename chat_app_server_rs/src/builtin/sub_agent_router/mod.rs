mod ai_runtime;
mod catalog;
mod core;
mod marketplace;
mod prompting;
mod recommendation;
mod registry;
mod runner;
pub mod settings;
mod types;
mod utils;

use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration as StdDuration;

use chrono::Utc;
use once_cell::sync::Lazy;
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::mcp_tools::ToolStreamChunkCallback;
use crate::repositories::{ai_model_configs, mcp_configs};
use crate::services::builtin_mcp::{list_builtin_mcp_configs, SUB_AGENT_ROUTER_MCP_ID};
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v2::ai_client::{
    AiClient as LegacyAiClient, AiClientCallbacks as LegacyAiClientCallbacks,
};
use crate::services::v2::ai_request_handler::AiRequestHandler as LegacyAiRequestHandler;
use crate::services::v2::mcp_tool_execute::McpToolExecute as LegacyMcpToolExecute;
use crate::services::v2::message_manager::MessageManager as LegacyMessageManager;
use crate::services::v3::ai_client::{AiClient, AiClientCallbacks, ProcessOptions};
use crate::services::v3::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::services::v3::message_manager::MessageManager;
use crate::utils::abort_registry;
use crate::utils::model_config::normalize_provider;

use self::ai_runtime::{
    filter_legacy_tools_by_prefixes, filter_tools_by_prefixes, resolve_effective_mcp_selection,
    resolve_model_config, summarize_single_tool_result_for_event, summarize_tool_calls_for_event,
    summarize_tool_results_for_event,
};
use self::catalog::SubAgentCatalog;
use self::core::{
    append_job_event, block_on_result, canonical_or_original, create_job, get_cancel_flag,
    list_job_events, map_status_to_job_state, optional_trimmed_string, parse_string_array,
    remove_cancel_flag, required_trimmed_string, run_sub_agent_schema, run_sub_agent_sync,
    serialize_agent, serialize_commands, set_cancel_flag, text_result, trace_log_path_string,
    trace_router_node, truncate_for_event, update_job_status, with_chatos,
};
use self::prompting::{build_env, build_system_prompt, resolve_allow_prefixes, select_skills};
use self::recommendation::suggest_sub_agent_text_with_docs;
use self::registry::AgentRegistry;
use self::runner::run_command;
use self::types::{AgentSpec, CommandSpec, JobEvent, JobRecord, SkillSpec};
use self::utils::{generate_id, normalize_name, unique_strings};

const SUBAGENT_GUARDRAIL: &str = "Tooling guard: sub-agents cannot call mcp_subagent_router_* or other sub-agent routing tools. Complete the task directly with available project/shell/task tools.";
const ROUTER_TRACE_LOG_FILE: &str = "sub_agent_router_nodes.jsonl";
const ROUTER_TRACE_PAYLOAD_MAX_CHARS: usize = 20_000;

#[derive(Clone)]
struct JobExecutionContext {
    ctx: BoundContext,
    task: String,
    args: Value,
    resolved: ResolvedAgent,
    session_id: String,
    run_id: String,
    job_id: String,
}

#[derive(Debug, Clone)]
pub struct SubAgentRouterOptions {
    pub server_name: String,
    pub root: PathBuf,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub timeout_ms: i64,
    pub max_output_bytes: usize,
    pub ai_timeout_ms: i64,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Clone)]
pub struct SubAgentRouterService {
    tools: HashMap<String, Tool>,
    default_session_id: String,
    default_run_id: String,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value, &ToolContext) -> Result<Value, String> + Send + Sync>;

struct ToolContext<'a> {
    session_id: &'a str,
    run_id: &'a str,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
}

#[derive(Clone)]
struct BoundContext {
    server_name: String,
    workspace_root: PathBuf,
    user_id: Option<String>,
    project_id: Option<String>,
    timeout_ms: i64,
    max_output_bytes: usize,
    ai_timeout_ms: i64,
    catalog: Arc<Mutex<SubAgentCatalog>>,
}

#[derive(Clone)]
struct ResolvedAgent {
    agent: AgentSpec,
    command: Option<CommandSpec>,
    used_skills: Vec<SkillSpec>,
    reason: String,
}

#[derive(Clone)]
struct ResolvedModel {
    id: String,
    name: String,
    provider: String,
    model: String,
    thinking_level: Option<String>,
    supports_responses: bool,
    api_key: String,
    base_url: String,
}

#[derive(Clone)]
struct AiTaskResult {
    response: String,
    reasoning: Option<String>,
    finish_reason: Option<String>,
    model_id: String,
    model_name: String,
    provider: String,
    model: String,
}

#[derive(Clone, Debug)]
struct AllowPrefixesPolicy {
    configured: bool,
    prefixes: Vec<String>,
}

#[derive(Clone, Debug)]
struct EffectiveMcpSelection {
    configured: bool,
    ids: Vec<String>,
}

impl SubAgentRouterService {
    pub fn new(opts: SubAgentRouterOptions) -> Result<Self, String> {
        let state_paths = settings::ensure_state_files()?;
        let registry = AgentRegistry::new(state_paths.registry_path.as_path())?;
        let catalog = SubAgentCatalog::new(
            registry,
            Some(state_paths.marketplace_path.clone()),
            Some(state_paths.plugins_root.clone()),
        );

        let workspace_root = canonical_or_original(opts.root);
        let ctx = BoundContext {
            server_name: normalize_name(opts.server_name.as_str()),
            workspace_root,
            user_id: opts
                .user_id
                .as_deref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            project_id: opts
                .project_id
                .as_deref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            timeout_ms: opts.timeout_ms.max(1_000),
            max_output_bytes: opts.max_output_bytes.max(4_096),
            ai_timeout_ms: opts.ai_timeout_ms.max(5_000),
            catalog: Arc::new(Mutex::new(catalog)),
        };

        let default_session_id = opts
            .session_id
            .as_deref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| generate_id("session"));
        let default_run_id = opts
            .run_id
            .as_deref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_default();

        let mut service = Self {
            tools: HashMap::new(),
            default_session_id,
            default_run_id,
        };

        {
            let ctx = ctx.clone();
            service.register_tool(
                "get_sub_agent",
                "Return details by agent_id (description, skills, commands, default command).",
                json!({
                    "type": "object",
                    "properties": { "agent_id": { "type": "string" } },
                    "additionalProperties": false,
                    "required": ["agent_id"]
                }),
                Arc::new(move |args, _tool_ctx| {
                    let agent_id = required_trimmed_string(&args, "agent_id")?;
                    let mut guard = ctx
                        .catalog
                        .lock()
                        .map_err(|_| "catalog lock poisoned".to_string())?;
                    let _ = guard.reload();
                    let agent = guard
                        .get_agent(agent_id.as_str())
                        .ok_or_else(|| format!("Sub-agent {} not found.", agent_id))?;
                    let payload = json!({
                        "agent": serialize_agent(&agent),
                        "commands": serialize_commands(&agent.commands.clone().unwrap_or_default()),
                        "default_command": agent.default_command.clone().unwrap_or_default()
                    });
                    Ok(text_result(with_chatos(
                        ctx.server_name.as_str(),
                        "get_sub_agent",
                        payload,
                        "ok",
                    )))
                }),
            );
        }

        {
            let ctx = ctx.clone();
            service.register_tool(
                "suggest_sub_agent",
                "Pick the best sub-agent for the task. Call this at most once per user task.",
                json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string" }
                    },
                    "additionalProperties": false,
                    "required": ["task"]
                }),
                Arc::new(move |args, _tool_ctx| {
                    let task = required_trimmed_string(&args, "task")?;
                    let caller_model = optional_trimmed_string(&args, "caller_model");
                    trace_router_node(
                        "suggest_sub_agent",
                        "start",
                        None,
                        Some(_tool_ctx.session_id),
                        Some(_tool_ctx.run_id),
                        Some(json!({
                            "task": truncate_for_event(task.as_str(), 2_000),
                            "caller_model": caller_model.clone(),
                        })),
                    );

                    let ai_text = suggest_sub_agent_text_with_docs(
                        &ctx,
                        task.as_str(),
                        caller_model.as_deref(),
                        _tool_ctx.on_stream_chunk.clone(),
                    )?;
                    trace_router_node(
                        "suggest_sub_agent",
                        "finish",
                        None,
                        Some(_tool_ctx.session_id),
                        Some(_tool_ctx.run_id),
                        Some(json!({
                            "response_preview": truncate_for_event(ai_text.as_str(), 2_000),
                        })),
                    );
                    Ok(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": ai_text,
                            }
                        ]
                    }))
                }),
            );
        }

        {
            let ctx = ctx.clone();
            service.register_tool(
                "run_sub_agent",
                "Run a sub-agent task synchronously using the provided agent_id.",
                run_sub_agent_schema(),
                Arc::new(move |args, tool_ctx| run_sub_agent_sync(ctx.clone(), args, tool_ctx)),
            );
        }

        service.register_tool(
            "cancel_sub_agent_job",
            "Cancel a running sub-agent job.",
            json!({
                "type": "object",
                "properties": { "job_id": { "type": "string" } },
                "additionalProperties": false,
                "required": ["job_id"]
            }),
            Arc::new(move |args, _tool_ctx| {
                let job_id = required_trimmed_string(&args, "job_id")?;
                if let Some(flag) = get_cancel_flag(job_id.as_str()) {
                    flag.store(true, Ordering::Relaxed);
                }
                let updated = update_job_status(
                    job_id.as_str(),
                    "cancelled",
                    None,
                    Some("cancelled".to_string()),
                )
                .ok_or_else(|| format!("Job not found: {}", job_id))?;
                append_job_event(
                    job_id.as_str(),
                    "cancel",
                    None,
                    updated.session_id.as_str(),
                    updated.run_id.as_str(),
                );
                Ok(text_result(json!({ "job": updated })))
            }),
        );

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema
                })
            })
            .collect()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        session_id: Option<&str>,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;

        let session = session_id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(self.default_session_id.as_str());
        let run = if self.default_run_id.trim().is_empty() {
            session
        } else {
            self.default_run_id.as_str()
        };

        let ctx = ToolContext {
            session_id: session,
            run_id: run,
            on_stream_chunk,
        };

        trace_router_node(
            "tool_call",
            "start",
            None,
            Some(session),
            Some(run),
            Some(json!({
                "tool": name,
                "args": args.clone(),
            })),
        );

        let result = (tool.handler)(args, &ctx);

        match &result {
            Ok(payload) => {
                trace_router_node(
                    "tool_call",
                    "finish",
                    None,
                    Some(session),
                    Some(run),
                    Some(json!({
                        "tool": name,
                        "status": "ok",
                        "result_preview": truncate_for_event(payload.to_string().as_str(), 4_000),
                    })),
                );
            }
            Err(err) => {
                trace_router_node(
                    "tool_call",
                    "error",
                    None,
                    Some(session),
                    Some(run),
                    Some(json!({
                        "tool": name,
                        "status": "error",
                        "error": err,
                    })),
                );
            }
        }

        result
    }

    fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: ToolHandler,
    ) {
        self.tools.insert(
            name.to_string(),
            Tool {
                name: name.to_string(),
                description: description.to_string(),
                input_schema,
                handler,
            },
        );
    }
}

pub fn summarize_settings() -> Result<Value, String> {
    settings::load_settings_summary()
}

pub fn get_mcp_permissions_settings() -> Result<Value, String> {
    settings::load_mcp_permissions()
}

pub fn save_mcp_permissions_settings(
    enabled_mcp_ids: &[String],
    enabled_tool_prefixes: &[String],
) -> Result<Value, String> {
    settings::save_mcp_permissions(enabled_mcp_ids, enabled_tool_prefixes)
}

pub fn import_agents_from_json(raw: &str) -> Result<Value, String> {
    settings::import_agents_json(raw)
}

pub fn import_skills_from_json(raw: &str) -> Result<Value, String> {
    settings::import_marketplace_json(raw)
}

pub fn import_from_git(
    repository: &str,
    branch: Option<&str>,
    agents_path: Option<&str>,
    skills_path: Option<&str>,
) -> Result<Value, String> {
    settings::import_from_git(settings::GitImportOptions {
        repository: repository.to_string(),
        branch: branch.map(|v| v.to_string()),
        agents_path: agents_path.map(|v| v.to_string()),
        skills_path: skills_path.map(|v| v.to_string()),
    })
}

pub fn install_plugins_from_marketplace(
    source: Option<&str>,
    install_all: bool,
) -> Result<Value, String> {
    settings::install_plugins(settings::InstallPluginOptions {
        source: source.map(|v| v.to_string()),
        install_all,
    })
}
