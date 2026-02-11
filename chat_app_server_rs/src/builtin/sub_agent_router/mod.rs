mod catalog;
mod marketplace;
mod registry;
mod runner;
mod selector;
pub mod settings;
mod types;
mod utils;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use tokio::time::{timeout, Duration};

use crate::config::Config;
use crate::repositories::{ai_model_configs, mcp_configs};
use crate::services::builtin_mcp::{list_builtin_mcp_configs, SUB_AGENT_ROUTER_MCP_ID};
use crate::services::mcp_loader::load_mcp_configs_for_user;
use crate::services::user_settings::{apply_settings_to_ai_client, get_effective_user_settings};
use crate::services::v3::ai_client::{AiClient, AiClientCallbacks, ProcessOptions};
use crate::services::v3::ai_request_handler::{AiRequestHandler, StreamCallbacks};
use crate::services::v3::mcp_tool_execute::McpToolExecute;
use crate::services::v3::message_manager::MessageManager;
use crate::utils::model_config::normalize_provider;

use self::catalog::SubAgentCatalog;
use self::registry::AgentRegistry;
use self::runner::run_command;
use self::selector::{pick_agent, PickOptions, PickResult};
use self::types::{AgentSpec, CommandSpec, JobEvent, JobRecord, SkillSpec};
use self::utils::{generate_id, normalize_name, unique_strings};

const SUBAGENT_GUARDRAIL: &str = "Tooling guard: sub-agents cannot call mcp_subagent_router_* or other sub-agent routing tools. Complete the task directly with available project/shell/task tools.";

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

#[derive(Clone, Debug)]
struct AgentRecommendationCandidate {
    agent: AgentSpec,
    skill_ids: Vec<String>,
    prompt_item: Value,
}

#[derive(Clone, Debug)]
struct AgentRecommendation {
    agent_id: String,
    skill_ids: Vec<String>,
    reason: String,
}

static JOBS: Lazy<Mutex<HashMap<String, JobRecord>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static JOB_EVENTS: Lazy<Mutex<HashMap<String, Vec<JobEvent>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static JOB_CANCEL_FLAGS: Lazy<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

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
                "Pick the best sub-agent using task with optional category hint.",
                json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string" },
                        "category": { "type": "string" }
                    },
                    "additionalProperties": false,
                    "required": ["task"]
                }),
                Arc::new(move |args, _tool_ctx| {
                    let task = required_trimmed_string(&args, "task")?;
                    let category = optional_trimmed_string(&args, "category");
                    let caller_model = optional_trimmed_string(&args, "caller_model");

                    let mut guard = ctx
                        .catalog
                        .lock()
                        .map_err(|_| "catalog lock poisoned".to_string())?;
                    let _ = guard.reload();
                    let agents = guard.list_agents();
                    if agents.is_empty() {
                        return Ok(text_result(with_chatos(
                            ctx.server_name.as_str(),
                            "suggest_sub_agent",
                            json!({
                                "agent_id": Value::Null,
                                "reason": "No sub-agents available. Import agents/skills first.",
                                "skills": []
                            }),
                            "ok",
                        )));
                    }

                    let candidates = build_agent_recommendation_candidates(&agents, &guard);
                    drop(guard);

                    let llm_pick = pick_agent_with_llm(
                        &ctx,
                        &agents,
                        &candidates,
                        task.as_str(),
                        category.clone(),
                        None,
                        None,
                        None,
                        caller_model.as_deref(),
                    );

                    let (picked, selector) = if let Some(picked) = llm_pick {
                        (picked, "llm")
                    } else if let Some(picked) = pick_agent_with_fallback(
                        &agents,
                        task.as_str(),
                        category.clone(),
                        None,
                        None,
                        None,
                    ) {
                        (picked, "heuristic")
                    } else {
                        return Ok(text_result(with_chatos(
                            ctx.server_name.as_str(),
                            "suggest_sub_agent",
                            json!({
                                "agent_id": Value::Null,
                                "reason": "No matching sub-agent. Import more agents/skills.",
                                "skills": [],
                                "filters": {
                                    "category": category,
                                }
                            }),
                            "ok",
                        )));
                    };

                    let used_skills = resolve_skill_ids(&picked.used_skills, &picked.agent);
                    Ok(text_result(with_chatos(
                        ctx.server_name.as_str(),
                        "suggest_sub_agent",
                        json!({
                            "agent_id": picked.agent.id,
                            "agent_name": picked.agent.name,
                            "score": picked.score,
                            "reason": picked.reason,
                            "skills": used_skills,
                            "selector": selector,
                        }),
                        "ok",
                    )))
                }),
            );
        }

        {
            let ctx = ctx.clone();
            service.register_tool(
                "run_sub_agent",
                "Run a sub-agent task synchronously. If command is missing, it falls back to AI generation.",
                run_sub_agent_schema(),
                Arc::new(move |args, tool_ctx| {
                    run_sub_agent_sync(ctx.clone(), args, tool_ctx)
                }),
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
        };

        (tool.handler)(args, &ctx)
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

fn run_sub_agent_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task": { "type": "string" },
            "agent_id": { "type": "string" },
            "category": { "type": "string" },
            "skills": { "type": "array", "items": { "type": "string" } },
            "query": { "type": "string" },
            "command_id": { "type": "string" }
        },
        "additionalProperties": false,
        "required": ["task"]
    })
}

fn run_sub_agent_sync(
    ctx: BoundContext,
    args: Value,
    tool_ctx: &ToolContext,
) -> Result<Value, String> {
    let task = required_trimmed_string(&args, "task")?;
    let resolved = resolve_agent_and_command(&ctx, task.as_str(), &args)?;

    let job = create_job(
        task.as_str(),
        Some(resolved.agent.id.clone()),
        resolved.command.as_ref().map(|c| c.id.clone()),
        Some(args.clone()),
        tool_ctx.session_id,
        tool_ctx.run_id,
    );
    let _ = update_job_status(job.id.as_str(), "running", None, None);
    append_job_event(
        job.id.as_str(),
        "start",
        Some(json!({
            "agent_id": resolved.agent.id,
            "command_id": resolved.command.as_ref().map(|c| c.id.clone()).unwrap_or_default()
        })),
        tool_ctx.session_id,
        tool_ctx.run_id,
    );

    let execution = JobExecutionContext {
        ctx: ctx.clone(),
        task,
        args,
        resolved: resolved.clone(),
        session_id: tool_ctx.session_id.to_string(),
        run_id: tool_ctx.run_id.to_string(),
        job_id: job.id.clone(),
    };

    let (status, payload, error_text) = match execute_job(execution.clone(), None) {
        Ok((status, payload)) => (status, payload, None),
        Err(err) => (
            "error".to_string(),
            json!({
                "status": "error",
                "job_id": execution.job_id,
                "agent_id": execution.resolved.agent.id,
                "agent_name": execution.resolved.agent.name,
                "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
                "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                "reason": execution.resolved.reason,
                "error": err,
            }),
            Some(err),
        ),
    };
    let final_status = map_status_to_job_state(status.as_str());
    let _ = update_job_status(
        job.id.as_str(),
        final_status,
        Some(payload.to_string()),
        error_text,
    );
    append_job_event(
        job.id.as_str(),
        "finish",
        Some(json!({
            "status": final_status,
        })),
        tool_ctx.session_id,
        tool_ctx.run_id,
    );

    let mut response_payload = payload;
    if let Value::Object(ref mut map) = response_payload {
        map.insert(
            "job_events".to_string(),
            serde_json::to_value(list_job_events(job.id.as_str())).unwrap_or_else(|_| json!([])),
        );
    }

    Ok(text_result(with_chatos(
        ctx.server_name.as_str(),
        "run_sub_agent",
        response_payload,
        status.as_str(),
    )))
}

fn execute_job(
    execution: JobExecutionContext,
    cancel_flag: Option<&AtomicBool>,
) -> Result<(String, Value), String> {
    if let Some(flag) = cancel_flag {
        if flag.load(Ordering::Relaxed) {
            return Ok((
                "cancelled".to_string(),
                json!({
                    "status": "cancelled",
                    "job_id": execution.job_id,
                    "agent_id": execution.resolved.agent.id,
                    "agent_name": execution.resolved.agent.name,
                    "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
                    "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                    "reason": execution.resolved.reason,
                    "error": "cancelled"
                }),
            ));
        }
    }

    let requested_model = optional_trimmed_string(&execution.args, "caller_model")
        .or_else(|| optional_trimmed_string(&execution.args, "model"));
    let allow_policy = resolve_allow_prefixes(execution.args.get("mcp_allow_prefixes"));
    append_job_event(
        execution.job_id.as_str(),
        "execute_prepare",
        Some(json!({
            "agent_id": execution.resolved.agent.id,
            "agent_name": execution.resolved.agent.name,
            "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
            "skills": execution
                .resolved
                .used_skills
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>(),
            "requested_model": requested_model.clone(),
            "allow_prefixes": allow_policy.prefixes.clone(),
            "query": optional_trimmed_string(&execution.args, "query").unwrap_or_default(),
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    let run_env = build_env(
        execution.task.as_str(),
        &execution.resolved.agent,
        execution.resolved.command.as_ref(),
        &execution.resolved.used_skills,
        execution.session_id.as_str(),
        execution.run_id.as_str(),
        optional_trimmed_string(&execution.args, "query").as_deref(),
        optional_trimmed_string(&execution.args, "model").as_deref(),
        optional_trimmed_string(&execution.args, "caller_model").as_deref(),
        &allow_policy.prefixes,
        execution.ctx.project_id.as_deref(),
    );

    if let Some(cmd) = execution
        .resolved
        .command
        .clone()
        .and_then(|command| command.exec)
    {
        let cwd = resolve_command_cwd(
            execution.ctx.workspace_root.as_path(),
            execution
                .resolved
                .command
                .as_ref()
                .and_then(|command| command.cwd.as_deref()),
        );

        append_job_event(
            execution.job_id.as_str(),
            "command_start",
            Some(json!({
                "command": cmd.clone(),
                "cwd": cwd,
                "timeout_ms": execution.ctx.timeout_ms,
            })),
            execution.session_id.as_str(),
            execution.run_id.as_str(),
        );

        let result = run_command(
            &cmd,
            &run_env,
            cwd.as_deref(),
            execution.ctx.timeout_ms,
            execution.ctx.max_output_bytes,
            None,
            cancel_flag,
        )?;

        let status = if matches!(result.error.as_deref(), Some("cancelled")) {
            "cancelled".to_string()
        } else if result.exit_code.unwrap_or(0) == 0 && !result.timed_out {
            "ok".to_string()
        } else {
            "error".to_string()
        };

        let payload = json!({
            "status": status,
            "job_id": execution.job_id,
            "agent_id": execution.resolved.agent.id,
            "agent_name": execution.resolved.agent.name,
            "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
            "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
            "reason": execution.resolved.reason,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.exit_code,
            "signal": result.signal,
            "duration_ms": result.duration_ms,
            "started_at": result.started_at,
            "finished_at": result.finished_at,
            "stdout_truncated": result.stdout_truncated,
            "stderr_truncated": result.stderr_truncated,
            "error": result.error,
            "timed_out": result.timed_out,
        });

        append_job_event(
            execution.job_id.as_str(),
            "command_finish",
            Some(json!({
                "status": payload.get("status").cloned().unwrap_or(Value::String("error".to_string())),
                "exit_code": result.exit_code,
                "signal": result.signal,
                "duration_ms": result.duration_ms,
                "timed_out": result.timed_out,
                "error": result.error,
                "stdout_preview": truncate_for_event(result.stdout.as_str(), 2000),
                "stderr_preview": truncate_for_event(result.stderr.as_str(), 2000),
                "stdout_truncated": result.stdout_truncated,
                "stderr_truncated": result.stderr_truncated,
            })),
            execution.session_id.as_str(),
            execution.run_id.as_str(),
        );

        return Ok((
            payload
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("error")
                .to_string(),
            payload,
        ));
    }

    let system_prompt = {
        let mut guard = execution
            .ctx
            .catalog
            .lock()
            .map_err(|_| "catalog lock poisoned".to_string())?;
        build_system_prompt(
            &execution.resolved.agent,
            &execution.resolved.used_skills,
            execution.resolved.command.as_ref(),
            &mut guard,
            &allow_policy,
        )
    };

    append_job_event(
        execution.job_id.as_str(),
        "ai_start",
        Some(json!({
            "requested_model": requested_model.clone(),
            "timeout_ms": execution.ctx.ai_timeout_ms,
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    let ai = {
        let ctx = execution.ctx.clone();
        let task = execution.task.clone();
        let requested = requested_model
            .as_deref()
            .map(|value| value.trim().to_string());
        let prompt = system_prompt.clone();
        let allow_policy = allow_policy.clone();
        let job_id = execution.job_id.clone();
        let session_id = execution.session_id.clone();
        let run_id = execution.run_id.clone();

        block_on_result(async move {
            let model = resolve_model_config(ctx.user_id.clone(), requested).await?;
            if model.api_key.trim().is_empty() {
                return Err(
                    "No usable AI API key found in model configs or OPENAI_API_KEY".to_string(),
                );
            }

            let mcp_selection = resolve_effective_mcp_selection(ctx.user_id.clone())
                .await
                .unwrap_or(EffectiveMcpSelection {
                    configured: false,
                    ids: Vec::new(),
                });

            let workspace_dir = ctx.workspace_root.to_string_lossy().to_string();
            let mcp_ids = mcp_selection.ids.clone();

            let (http_servers, mut stdio_servers, builtin_servers) =
                if mcp_selection.configured && mcp_ids.is_empty() {
                    (Vec::new(), Vec::new(), Vec::new())
                } else {
                    load_mcp_configs_for_user(
                        ctx.user_id.clone(),
                        if mcp_ids.is_empty() {
                            None
                        } else {
                            Some(mcp_ids.clone())
                        },
                        if workspace_dir.trim().is_empty() {
                            None
                        } else {
                            Some(workspace_dir.as_str())
                        },
                        ctx.project_id.as_deref(),
                    )
                    .await
                    .unwrap_or((Vec::new(), Vec::new(), Vec::new()))
                };

            if !workspace_dir.trim().is_empty() {
                for server in stdio_servers.iter_mut() {
                    if server
                        .cwd
                        .as_ref()
                        .map(|value| value.trim().is_empty())
                        .unwrap_or(true)
                    {
                        server.cwd = Some(workspace_dir.clone());
                    }
                }
            }

            let mut mcp_execute = McpToolExecute::new(
                http_servers.clone(),
                stdio_servers.clone(),
                builtin_servers.clone(),
            );

            if !http_servers.is_empty() || !stdio_servers.is_empty() || !builtin_servers.is_empty()
            {
                if let Err(err) = mcp_execute.init().await {
                    append_job_event(
                        job_id.as_str(),
                        "ai_mcp_init_error",
                        Some(json!({ "error": err })),
                        session_id.as_str(),
                        run_id.as_str(),
                    );
                }
            }

            let (tools_before_filter, tools_after_filter) = if allow_policy.configured {
                filter_tools_by_prefixes(&mut mcp_execute, &allow_policy.prefixes)
            } else {
                let count = mcp_execute.tools.len();
                (count, count)
            };

            append_job_event(
                job_id.as_str(),
                "ai_mcp_ready",
                Some(json!({
                    "configured": mcp_selection.configured,
                    "enabled_mcp_ids": mcp_ids,
                    "allow_prefixes": allow_policy.prefixes,
                    "servers": {
                        "http": http_servers.len(),
                        "stdio": stdio_servers.len(),
                        "builtin": builtin_servers.len(),
                    },
                    "tools_before_filter": tools_before_filter,
                    "tools_after_filter": tools_after_filter,
                })),
                session_id.as_str(),
                run_id.as_str(),
            );

            let message_manager = MessageManager::new();
            let handler = AiRequestHandler::new(
                model.api_key.clone(),
                model.base_url.clone(),
                message_manager.clone(),
            );
            let mut ai_client = AiClient::new(handler, mcp_execute, message_manager);

            let effective_settings = get_effective_user_settings(ctx.user_id.clone())
                .await
                .unwrap_or_else(|_| json!({}));
            apply_settings_to_ai_client(&mut ai_client, &effective_settings);
            let max_tokens = effective_settings
                .get("CHAT_MAX_TOKENS")
                .and_then(|value| value.as_i64())
                .filter(|value| *value > 0);

            let chunk_buffer = Arc::new(Mutex::new(String::new()));
            let thinking_buffer = Arc::new(Mutex::new(String::new()));

            let on_chunk = {
                let chunk_buffer = chunk_buffer.clone();
                Arc::new(move |chunk: String| {
                    if chunk.trim().is_empty() {
                        return;
                    }
                    if let Ok(mut guard) = chunk_buffer.lock() {
                        guard.push_str(chunk.as_str());
                        if guard.chars().count() > 24_000 {
                            let trimmed = guard
                                .chars()
                                .rev()
                                .take(24_000)
                                .collect::<String>()
                                .chars()
                                .rev()
                                .collect::<String>();
                            *guard = trimmed;
                        }
                    }
                })
            };

            let on_thinking = {
                let thinking_buffer = thinking_buffer.clone();
                Arc::new(move |chunk: String| {
                    if chunk.trim().is_empty() {
                        return;
                    }
                    if let Ok(mut guard) = thinking_buffer.lock() {
                        guard.push_str(chunk.as_str());
                        if guard.chars().count() > 24_000 {
                            let trimmed = guard
                                .chars()
                                .rev()
                                .take(24_000)
                                .collect::<String>()
                                .chars()
                                .rev()
                                .collect::<String>();
                            *guard = trimmed;
                        }
                    }
                })
            };

            let on_tools_start = {
                let job_id = job_id.clone();
                let session_id = session_id.clone();
                let run_id = run_id.clone();
                Arc::new(move |tool_calls: Value| {
                    append_job_event(
                        job_id.as_str(),
                        "ai_tools_start",
                        Some(json!({
                            "tool_calls": summarize_tool_calls_for_event(&tool_calls),
                        })),
                        session_id.as_str(),
                        run_id.as_str(),
                    );
                })
            };

            let on_tools_stream = {
                let job_id = job_id.clone();
                let session_id = session_id.clone();
                let run_id = run_id.clone();
                Arc::new(move |result: Value| {
                    append_job_event(
                        job_id.as_str(),
                        "ai_tools_stream",
                        Some(summarize_single_tool_result_for_event(&result)),
                        session_id.as_str(),
                        run_id.as_str(),
                    );
                })
            };

            let on_tools_end = {
                let job_id = job_id.clone();
                let session_id = session_id.clone();
                let run_id = run_id.clone();
                Arc::new(move |result: Value| {
                    append_job_event(
                        job_id.as_str(),
                        "ai_tools_end",
                        Some(summarize_tool_results_for_event(&result)),
                        session_id.as_str(),
                        run_id.as_str(),
                    );
                })
            };

            let messages = vec![json!({
                "role": "user",
                "content": [
                    { "type": "input_text", "text": task }
                ]
            })];

            let req = ai_client.process_request(
                messages,
                None,
                ProcessOptions {
                    model: Some(model.model.clone()),
                    provider: Some(model.provider.clone()),
                    thinking_level: model.thinking_level.clone(),
                    temperature: Some(0.7),
                    max_tokens,
                    reasoning_enabled: Some(true),
                    system_prompt: Some(prompt),
                    history_limit: Some(1),
                    purpose: Some("sub_agent_router".to_string()),
                    callbacks: Some(AiClientCallbacks {
                        on_chunk: Some(on_chunk),
                        on_thinking: Some(on_thinking),
                        on_tools_start: Some(on_tools_start),
                        on_tools_stream: Some(on_tools_stream),
                        on_tools_end: Some(on_tools_end),
                    }),
                },
            );

            let response = timeout(Duration::from_millis(ctx.ai_timeout_ms as u64), req)
                .await
                .map_err(|_| format!("AI timeout after {} ms", ctx.ai_timeout_ms))??;

            let mut content = response
                .get("content")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_default();

            if content.is_empty() {
                if let Ok(guard) = chunk_buffer.lock() {
                    let fallback = guard.trim();
                    if !fallback.is_empty() {
                        content = fallback.to_string();
                    }
                }
            }

            if content.is_empty() {
                content = "(empty)".to_string();
            }

            let mut reasoning = response
                .get("reasoning")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());

            if reasoning.is_none() {
                if let Ok(guard) = thinking_buffer.lock() {
                    let fallback = guard.trim();
                    if !fallback.is_empty() {
                        reasoning = Some(fallback.to_string());
                    }
                }
            }

            let finish_reason = response
                .get("finish_reason")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());

            Ok(AiTaskResult {
                response: content,
                reasoning,
                finish_reason,
                model_id: model.id,
                model_name: model.name,
                provider: model.provider,
                model: model.model,
            })
        })
    }?;

    append_job_event(
        execution.job_id.as_str(),
        "ai_finish",
        Some(json!({
            "model_id": ai.model_id,
            "model_name": ai.model_name,
            "provider": ai.provider,
            "model": ai.model,
            "finish_reason": ai.finish_reason,
            "reasoning": truncate_for_event(ai.reasoning.as_deref().unwrap_or(""), 12000),
            "response_preview": truncate_for_event(ai.response.as_str(), 6000),
        })),
        execution.session_id.as_str(),
        execution.run_id.as_str(),
    );

    let payload = json!({
        "status": "ok",
        "job_id": execution.job_id,
        "agent_id": execution.resolved.agent.id,
        "agent_name": execution.resolved.agent.name,
        "command_id": execution.resolved.command.as_ref().map(|c| c.id.clone()),
        "skills": execution.resolved.used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        "reason": execution.resolved.reason,
        "response": ai.response,
        "reasoning": ai.reasoning,
        "finish_reason": ai.finish_reason,
        "model_id": ai.model_id,
        "model_name": ai.model_name,
        "provider": ai.provider,
        "model": ai.model,
    });

    Ok(("ok".to_string(), payload))
}

fn pick_agent_with_fallback(
    agents: &[AgentSpec],
    task: &str,
    category: Option<String>,
    skills: Option<Vec<String>>,
    query: Option<String>,
    command_id: Option<String>,
) -> Option<PickResult> {
    let strict = pick_agent(
        agents,
        PickOptions {
            task: task.to_string(),
            category: category.clone(),
            skills: skills.clone(),
            query: query.clone(),
            command_id: command_id.clone(),
        },
    );

    if strict.is_some() {
        return strict;
    }

    let relax_category = category
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let relax_command = command_id
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    if !relax_category && !relax_command {
        return None;
    }

    let mut fallback = pick_agent(
        agents,
        PickOptions {
            task: task.to_string(),
            category: None,
            skills,
            query,
            command_id: None,
        },
    )?;

    let mut notes = Vec::new();
    if relax_category {
        notes.push("category");
    }
    if relax_command {
        notes.push("command");
    }

    if !notes.is_empty() {
        fallback.reason = format!("{} | fallback_without_{}", fallback.reason, notes.join("+"));
    }

    Some(fallback)
}

fn build_agent_recommendation_candidates(
    agents: &[AgentSpec],
    catalog: &SubAgentCatalog,
) -> Vec<AgentRecommendationCandidate> {
    let mut candidates = Vec::new();

    for agent in agents {
        let raw_skill_ids = agent
            .default_skills
            .clone()
            .or_else(|| agent.skills.clone())
            .unwrap_or_default();
        let normalized_skill_ids = unique_strings(
            raw_skill_ids
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty()),
        );

        let resolved_skills = catalog.resolve_skills(&normalized_skill_ids);
        let mut skill_ids = Vec::new();
        let mut skill_items = Vec::new();

        if resolved_skills.is_empty() {
            for skill_id in &normalized_skill_ids {
                skill_ids.push(skill_id.clone());
                skill_items.push(json!({
                    "id": skill_id,
                    "name": skill_id,
                    "description": ""
                }));
            }
        } else {
            for skill in resolved_skills {
                skill_ids.push(skill.id.clone());
                skill_items.push(json!({
                    "id": skill.id,
                    "name": skill.name,
                    "description": skill.description.unwrap_or_default()
                }));
            }
        }

        if skill_ids.is_empty() {
            skill_ids = normalized_skill_ids;
        }

        let skill_ids = unique_strings(
            skill_ids
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty()),
        );

        let command_items = agent
            .commands
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|command| {
                json!({
                    "id": command.id,
                    "name": command.name.unwrap_or_default(),
                    "description": command.description.unwrap_or_default()
                })
            })
            .collect::<Vec<_>>();

        candidates.push(AgentRecommendationCandidate {
            agent: agent.clone(),
            skill_ids,
            prompt_item: json!({
                "agent_id": agent.id,
                "name": agent.name,
                "description": agent.description.clone().unwrap_or_default(),
                "category": agent.category.clone().unwrap_or_default(),
                "skills": skill_items,
                "commands": command_items,
                "default_command": agent.default_command.clone().unwrap_or_default(),
                "plugin": agent.plugin.clone().unwrap_or_default(),
            }),
        });
    }

    candidates
}

fn pick_agent_with_llm(
    ctx: &BoundContext,
    agents: &[AgentSpec],
    candidates: &[AgentRecommendationCandidate],
    task: &str,
    category: Option<String>,
    skills: Option<Vec<String>>,
    query: Option<String>,
    command_id: Option<String>,
    requested_model: Option<&str>,
) -> Option<PickResult> {
    if agents.is_empty() || candidates.is_empty() {
        return None;
    }

    let recommendation = recommend_agent_with_ai(
        ctx,
        task,
        category,
        skills,
        query,
        command_id,
        candidates,
        requested_model,
    )
    .ok()
    .flatten()?;

    let candidate = find_candidate_by_agent_id(candidates, recommendation.agent_id.as_str())?;
    let used_skills = normalize_recommended_skill_ids(
        recommendation.skill_ids.as_slice(),
        candidate.skill_ids.as_slice(),
    );

    let reason = if recommendation.reason.trim().is_empty() {
        "LLM router selected the best matching sub-agent.".to_string()
    } else {
        format!("LLM router: {}", recommendation.reason.trim())
    };

    Some(PickResult {
        agent: candidate.agent.clone(),
        score: 100,
        reason,
        used_skills,
    })
}

fn recommend_agent_with_ai(
    ctx: &BoundContext,
    task: &str,
    category: Option<String>,
    skills: Option<Vec<String>>,
    query: Option<String>,
    command_id: Option<String>,
    candidates: &[AgentRecommendationCandidate],
    requested_model: Option<&str>,
) -> Result<Option<AgentRecommendation>, String> {
    if candidates.is_empty() {
        return Ok(None);
    }

    let max_candidates = 120usize;
    let candidate_items = candidates
        .iter()
        .take(max_candidates)
        .map(|candidate| candidate.prompt_item.clone())
        .collect::<Vec<_>>();

    let system_prompt = "You are a sub-agent routing recommender. Choose exactly one sub-agent and optional skill IDs. Return JSON only with fields: agent_id (string), skill_ids (array of strings), reason (string). Never return markdown.";

    let request_payload = json!({
        "task": task,
        "hints": {
            "category": category,
            "skills": skills.unwrap_or_default(),
            "query": query,
            "command_id": command_id,
        },
        "candidates": candidate_items,
    });

    let request_text = serde_json::to_string(&request_payload)
        .map_err(|err| format!("failed to build recommendation payload: {}", err))?;

    let ai = run_ai_task(ctx, system_prompt, request_text.as_str(), requested_model)?;
    let parsed = parse_json_object_from_text(ai.response.as_str());
    let Some(parsed) = parsed else {
        return Ok(None);
    };

    let agent_id = parsed
        .get("agent_id")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(agent_id) = agent_id else {
        return Ok(None);
    };

    let skill_ids = parse_string_array(parsed.get("skill_ids"))
        .or_else(|| parse_string_array(parsed.get("skills")))
        .unwrap_or_default();

    let reason = parsed
        .get("reason")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .unwrap_or_default();

    Ok(Some(AgentRecommendation {
        agent_id,
        skill_ids,
        reason,
    }))
}

fn find_candidate_by_agent_id<'a>(
    candidates: &'a [AgentRecommendationCandidate],
    agent_id: &str,
) -> Option<&'a AgentRecommendationCandidate> {
    let target = agent_id.trim().to_lowercase();
    if target.is_empty() {
        return None;
    }

    candidates.iter().find(|candidate| {
        candidate
            .agent
            .id
            .trim()
            .eq_ignore_ascii_case(target.as_str())
            || candidate
                .agent
                .name
                .trim()
                .eq_ignore_ascii_case(target.as_str())
    })
}

fn normalize_recommended_skill_ids(selected: &[String], available: &[String]) -> Vec<String> {
    if available.is_empty() {
        return Vec::new();
    }

    let mut lookup = HashMap::new();
    for skill_id in available {
        let key = skill_id.trim().to_lowercase();
        if !key.is_empty() {
            lookup.insert(key, skill_id.clone());
        }
    }

    if selected.is_empty() {
        return available.to_vec();
    }

    let out = selected
        .iter()
        .map(|item| item.trim().to_lowercase())
        .filter(|item| !item.is_empty())
        .filter_map(|item| lookup.get(item.as_str()).cloned())
        .collect::<Vec<_>>();

    let out = unique_strings(out);
    if out.is_empty() {
        available.to_vec()
    } else {
        out
    }
}

fn parse_json_object_from_text(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }

    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end <= start {
        return None;
    }

    serde_json::from_str::<Value>(&trimmed[start..=end]).ok()
}

fn resolve_agent_and_command(
    ctx: &BoundContext,
    task: &str,
    args: &Value,
) -> Result<ResolvedAgent, String> {
    let agent_id = optional_trimmed_string(args, "agent_id");
    let command_id = optional_trimmed_string(args, "command_id");
    let category = optional_trimmed_string(args, "category");
    let query = optional_trimmed_string(args, "query");
    let skills = parse_string_array(args.get("skills"));
    let caller_model = optional_trimmed_string(args, "caller_model")
        .or_else(|| optional_trimmed_string(args, "model"));

    let mut guard = ctx
        .catalog
        .lock()
        .map_err(|_| "catalog lock poisoned".to_string())?;
    let _ = guard.reload();

    if let Some(id) = agent_id {
        let agent = guard
            .get_agent(id.as_str())
            .ok_or_else(|| format!("Sub-agent {} not found.", id))?;
        let command = guard.resolve_command(&agent, command_id.as_deref());
        let used_skills = select_skills(&agent, skills, &guard);
        return Ok(ResolvedAgent {
            agent,
            command,
            used_skills,
            reason: id,
        });
    }

    let agents = guard.list_agents();
    let candidates = build_agent_recommendation_candidates(&agents, &guard);
    drop(guard);

    let picked = pick_agent_with_llm(
        ctx,
        &agents,
        &candidates,
        task,
        category.clone(),
        skills.clone(),
        query.clone(),
        command_id.clone(),
        caller_model.as_deref(),
    )
    .or_else(|| {
        pick_agent_with_fallback(&agents, task, category, skills, query, command_id.clone())
    })
    .ok_or_else(|| "No matching sub-agent. Import agents/skills first.".to_string())?;

    let mut guard = ctx
        .catalog
        .lock()
        .map_err(|_| "catalog lock poisoned".to_string())?;
    let _ = guard.reload();

    let agent = guard
        .get_agent(picked.agent.id.as_str())
        .unwrap_or_else(|| picked.agent.clone());

    let command = guard.resolve_command(&agent, command_id.as_deref());
    let used_skills = select_skills(&agent, Some(picked.used_skills.clone()), &guard);

    Ok(ResolvedAgent {
        agent,
        command,
        used_skills,
        reason: picked.reason,
    })
}

fn run_ai_task(
    ctx: &BoundContext,
    system_prompt: &str,
    task: &str,
    requested_model: Option<&str>,
) -> Result<AiTaskResult, String> {
    let user_id = ctx.user_id.clone();
    let requested = requested_model.map(|v| v.trim().to_string());
    let prompt = system_prompt.to_string();
    let task = task.to_string();
    let timeout_ms = ctx.ai_timeout_ms;

    block_on_result(async move {
        let model = resolve_model_config(user_id, requested).await?;
        if model.api_key.trim().is_empty() {
            return Err(
                "No usable AI API key found in model configs or OPENAI_API_KEY".to_string(),
            );
        }

        let message_manager = MessageManager::new();
        let handler = AiRequestHandler::new(
            model.api_key.clone(),
            model.base_url.clone(),
            message_manager,
        );

        let input = json!([
            {
                "role": "user",
                "content": [
                    { "type": "input_text", "text": task }
                ]
            }
        ]);

        let req = handler.handle_request(
            input,
            model.model.clone(),
            Some(prompt),
            None,
            None,
            Some(0.2),
            None,
            StreamCallbacks::default(),
            Some(model.provider.clone()),
            model.thinking_level.clone(),
            None,
            false,
            "sub_agent_router",
        );

        let response = timeout(Duration::from_millis(timeout_ms as u64), req)
            .await
            .map_err(|_| format!("AI timeout after {} ms", timeout_ms))??;

        let content = if response.content.trim().is_empty() {
            "(empty)".to_string()
        } else {
            response.content.trim().to_string()
        };

        Ok(AiTaskResult {
            response: content,
            reasoning: response.reasoning,
            finish_reason: response.finish_reason,
            model_id: model.id,
            model_name: model.name,
            provider: model.provider,
            model: model.model,
        })
    })
}

async fn resolve_effective_mcp_selection(
    user_id: Option<String>,
) -> Result<EffectiveMcpSelection, String> {
    let mut configured = false;
    let mut ids = Vec::new();

    if let Ok(saved) = settings::load_mcp_permissions() {
        configured = saved
            .get("configured")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        ids = parse_string_array(saved.get("enabled_mcp_ids")).unwrap_or_default();
    }

    ids.retain(|id| !id.eq_ignore_ascii_case(SUB_AGENT_ROUTER_MCP_ID));
    let ids = unique_strings(
        ids.into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    );

    if configured {
        return Ok(EffectiveMcpSelection { configured, ids });
    }

    let mut all_ids = list_builtin_mcp_configs()
        .into_iter()
        .map(|cfg| cfg.id)
        .collect::<Vec<_>>();

    let mut custom = mcp_configs::list_mcp_configs(user_id.clone()).await?;
    if custom.is_empty() && user_id.is_some() {
        custom = mcp_configs::list_mcp_configs(None).await?;
    }

    all_ids.extend(custom.into_iter().map(|cfg| cfg.id));

    let ids = unique_strings(
        all_ids
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| {
                !value.is_empty() && !value.eq_ignore_ascii_case(SUB_AGENT_ROUTER_MCP_ID)
            }),
    );

    Ok(EffectiveMcpSelection {
        configured: false,
        ids,
    })
}

fn filter_tools_by_prefixes(
    mcp_execute: &mut McpToolExecute,
    allow_prefixes: &[String],
) -> (usize, usize) {
    let before = mcp_execute.tools.len();

    let prefixes = unique_strings(
        allow_prefixes
            .iter()
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty()),
    );

    if prefixes.is_empty() {
        mcp_execute.tools.clear();
        mcp_execute.tool_metadata.clear();
        return (before, 0);
    }

    let mut kept_tool_names = HashSet::new();
    mcp_execute.tools.retain(|tool| {
        let Some(name) = tool.get("name").and_then(|value| value.as_str()) else {
            return false;
        };

        let keep = prefixes
            .iter()
            .any(|prefix| tool_matches_allowed_prefix(name, prefix.as_str()));

        if keep {
            kept_tool_names.insert(name.to_string());
        }

        keep
    });

    mcp_execute
        .tool_metadata
        .retain(|name, _| kept_tool_names.contains(name));

    (before, kept_tool_names.len())
}

fn tool_matches_allowed_prefix(tool_name: &str, prefix: &str) -> bool {
    let tool = tool_name.trim().to_lowercase();
    let prefix = prefix.trim().to_lowercase();

    if tool.is_empty() || prefix.is_empty() {
        return false;
    }

    tool == prefix || tool.starts_with(format!("{}_", prefix).as_str())
}

fn summarize_tool_calls_for_event(tool_calls: &Value) -> Value {
    let Some(arr) = tool_calls.as_array() else {
        return tool_calls.clone();
    };

    Value::Array(
        arr.iter()
            .map(|item| {
                let tool_call_id = item
                    .get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                let name = item
                    .get("function")
                    .and_then(|func| func.get("name"))
                    .and_then(|value| value.as_str())
                    .or_else(|| item.get("name").and_then(|value| value.as_str()))
                    .unwrap_or_default();

                let arguments_value = item
                    .get("function")
                    .and_then(|func| func.get("arguments"))
                    .or_else(|| item.get("arguments"));
                let arguments_preview = arguments_value
                    .map(|value| value_to_preview(value, 2_000))
                    .unwrap_or_default();

                json!({
                    "tool_call_id": tool_call_id,
                    "name": name,
                    "arguments_preview": arguments_preview,
                })
            })
            .collect(),
    )
}

fn summarize_tool_results_for_event(tool_results: &Value) -> Value {
    let arr = tool_results
        .get("tool_results")
        .and_then(|value| value.as_array())
        .or_else(|| tool_results.as_array());

    let Some(arr) = arr else {
        return summarize_single_tool_result_for_event(tool_results);
    };

    let summarized = arr
        .iter()
        .map(summarize_single_tool_result_for_event)
        .collect::<Vec<_>>();

    json!({ "tool_results": summarized })
}

fn summarize_single_tool_result_for_event(result: &Value) -> Value {
    let tool_call_id = result
        .get("tool_call_id")
        .or_else(|| result.get("toolCallId"))
        .or_else(|| result.get("id"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    let name = result
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    let success = result
        .get("success")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let is_error = result
        .get("is_error")
        .or_else(|| result.get("isError"))
        .and_then(|value| value.as_bool())
        .unwrap_or(!success);

    let content_preview = result
        .get("content")
        .or_else(|| result.get("result"))
        .or_else(|| result.get("output"))
        .map(|value| value_to_preview(value, 4_000))
        .unwrap_or_default();

    json!({
        "tool_call_id": tool_call_id,
        "name": name,
        "success": success,
        "is_error": is_error,
        "content_preview": content_preview,
    })
}

fn value_to_preview(value: &Value, max_chars: usize) -> String {
    let raw = if let Some(text) = value.as_str() {
        text.to_string()
    } else {
        value.to_string()
    };

    truncate_for_event(raw.as_str(), max_chars)
}

async fn resolve_model_config(
    user_id: Option<String>,
    requested: Option<String>,
) -> Result<ResolvedModel, String> {
    let mut models = ai_model_configs::list_ai_model_configs(user_id.clone()).await?;
    if models.is_empty() && user_id.is_some() {
        models = ai_model_configs::list_ai_model_configs(None).await?;
    }

    let enabled_models: Vec<_> = models.into_iter().filter(|m| m.enabled).collect();
    let requested_norm = requested
        .as_deref()
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty());

    if let Some(ref needle) = requested_norm {
        if let Some(found) = enabled_models
            .iter()
            .find(|cfg| model_matches(cfg, needle.as_str()))
        {
            return Ok(to_resolved_model(found.clone()));
        }
    }

    if let Some(first) = enabled_models.into_iter().next() {
        return Ok(to_resolved_model(first));
    }

    let cfg = Config::get();
    let fallback_model = requested
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "gpt-4o-mini".to_string());

    Ok(ResolvedModel {
        id: "env_default".to_string(),
        name: "Environment Default".to_string(),
        provider: "gpt".to_string(),
        model: fallback_model,
        thinking_level: None,
        api_key: cfg.openai_api_key.clone(),
        base_url: cfg.openai_base_url.clone(),
    })
}

fn model_matches(cfg: &crate::models::ai_model_config::AiModelConfig, needle: &str) -> bool {
    cfg.id.trim().eq_ignore_ascii_case(needle)
        || cfg.name.trim().eq_ignore_ascii_case(needle)
        || cfg.model.trim().eq_ignore_ascii_case(needle)
}

fn to_resolved_model(cfg: crate::models::ai_model_config::AiModelConfig) -> ResolvedModel {
    let env_cfg = Config::get();
    ResolvedModel {
        id: cfg.id,
        name: cfg.name,
        provider: normalize_provider(cfg.provider.as_str()),
        model: cfg.model,
        thinking_level: cfg.thinking_level,
        api_key: cfg
            .api_key
            .as_deref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| env_cfg.openai_api_key.clone()),
        base_url: cfg
            .base_url
            .as_deref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| env_cfg.openai_base_url.clone()),
    }
}

fn select_skills(
    agent: &AgentSpec,
    input: Option<Vec<String>>,
    catalog: &SubAgentCatalog,
) -> Vec<SkillSpec> {
    let preferred = if let Some(list) = input {
        list
    } else if let Some(defaults) = &agent.default_skills {
        defaults.clone()
    } else {
        agent.skills.clone().unwrap_or_default()
    };
    catalog.resolve_skills(&preferred)
}

fn resolve_skill_ids(skill_ids: &[String], agent: &AgentSpec) -> Vec<String> {
    if let Some(skills) = &agent.skills {
        let available: std::collections::HashSet<String> =
            skills.iter().map(|s| s.to_lowercase()).collect();
        skill_ids
            .iter()
            .filter(|s| available.is_empty() || available.contains(&s.to_lowercase()))
            .cloned()
            .collect()
    } else {
        skill_ids.to_vec()
    }
}

fn build_system_prompt(
    agent: &AgentSpec,
    skills: &[SkillSpec],
    command: Option<&CommandSpec>,
    catalog: &mut SubAgentCatalog,
    allow_policy: &AllowPrefixesPolicy,
) -> String {
    let mut sections = Vec::new();
    sections.push(format!("You are {}.", agent.name));

    if let Some(prompt_path) = agent.system_prompt_path.as_deref() {
        let agent_prompt = catalog.read_content(Some(prompt_path));
        if !agent_prompt.is_empty() {
            sections.push(agent_prompt);
        }
    }

    if let Some(cmd) = command {
        if let Some(path) = cmd.instructions_path.as_deref() {
            let command_prompt = catalog.read_content(Some(path));
            if !command_prompt.is_empty() {
                sections.push(format!("Command instructions:\n{}", command_prompt));
            }
        }
    }

    if !skills.is_empty() {
        let mut blocks = Vec::new();
        for skill in skills {
            let content = catalog.read_content(Some(skill.path.as_str()));
            if !content.is_empty() {
                blocks.push(format!("Skill: {}\n{}", skill.name, content));
            }
        }
        if !blocks.is_empty() {
            sections.push(format!("Skills:\n{}", blocks.join("\n\n")));
        }
    }

    if allow_policy.configured {
        if allow_policy.prefixes.is_empty() {
            sections.push("Allowed MCP prefixes: (none)".to_string());
        } else {
            sections.push(format!(
                "Allowed MCP prefixes: {}",
                allow_policy.prefixes.join(", ")
            ));
        }
    }
    sections.push(SUBAGENT_GUARDRAIL.to_string());

    sections.join("\n\n")
}

fn build_env(
    task: &str,
    agent: &AgentSpec,
    command: Option<&CommandSpec>,
    skills: &[SkillSpec],
    session_id: &str,
    run_id: &str,
    query: Option<&str>,
    model: Option<&str>,
    caller_model: Option<&str>,
    allow_prefixes: &[String],
    project_id: Option<&str>,
) -> HashMap<String, String> {
    let mut env_map: HashMap<String, String> = std::env::vars().collect();
    env_map.insert("SUBAGENT_TASK".to_string(), task.to_string());
    env_map.insert("SUBAGENT_AGENT_ID".to_string(), agent.id.clone());
    env_map.insert(
        "SUBAGENT_COMMAND_ID".to_string(),
        command.map(|c| c.id.clone()).unwrap_or_default(),
    );
    env_map.insert(
        "SUBAGENT_SKILLS".to_string(),
        skills
            .iter()
            .map(|s| s.id.clone())
            .collect::<Vec<_>>()
            .join(","),
    );
    env_map.insert("SUBAGENT_SESSION_ID".to_string(), session_id.to_string());
    env_map.insert("SUBAGENT_RUN_ID".to_string(), run_id.to_string());
    env_map.insert(
        "SUBAGENT_CATEGORY".to_string(),
        agent.category.clone().unwrap_or_default(),
    );
    env_map.insert(
        "SUBAGENT_QUERY".to_string(),
        query.unwrap_or("").to_string(),
    );
    env_map.insert(
        "SUBAGENT_MODEL".to_string(),
        model.unwrap_or("").to_string(),
    );
    env_map.insert(
        "SUBAGENT_CALLER_MODEL".to_string(),
        caller_model.unwrap_or("").to_string(),
    );
    env_map.insert(
        "SUBAGENT_MCP_ALLOW_PREFIXES".to_string(),
        allow_prefixes.join(","),
    );
    if let Some(pid) = project_id {
        env_map.insert("SUBAGENT_PROJECT_ID".to_string(), pid.to_string());
    }
    env_map
}

fn resolve_allow_prefixes(input: Option<&Value>) -> AllowPrefixesPolicy {
    if let Some(arr) = input.and_then(|v| v.as_array()) {
        let parsed = unique_strings(
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty()),
        );
        return AllowPrefixesPolicy {
            configured: true,
            prefixes: parsed,
        };
    }

    if let Ok(saved) = settings::load_mcp_permissions() {
        let configured = saved
            .get("configured")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if configured {
            let parsed = unique_strings(
                saved
                    .get("enabled_tool_prefixes")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry.as_str().map(|s| s.trim().to_string()))
                    .filter(|entry| !entry.is_empty()),
            );
            return AllowPrefixesPolicy {
                configured: true,
                prefixes: parsed,
            };
        }
    }

    let env_value = std::env::var("SUBAGENT_MCP_ALLOW_PREFIXES").unwrap_or_default();
    if env_value.trim().is_empty() {
        return AllowPrefixesPolicy {
            configured: false,
            prefixes: Vec::new(),
        };
    }

    let parsed = unique_strings(
        env_value
            .split(",")
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty()),
    );

    AllowPrefixesPolicy {
        configured: true,
        prefixes: parsed,
    }
}

fn resolve_command_cwd(workspace_root: &Path, command_cwd: Option<&str>) -> Option<String> {
    let cwd = command_cwd
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            let path = PathBuf::from(value.as_str());
            if path.is_absolute() {
                path
            } else {
                workspace_root.join(path)
            }
        })
        .unwrap_or_else(|| workspace_root.to_path_buf());

    Some(cwd.to_string_lossy().to_string())
}

fn parse_string_array(value: Option<&Value>) -> Option<Vec<String>> {
    let arr = value.and_then(|v| v.as_array())?;
    let items = arr
        .iter()
        .filter_map(|item| item.as_str().map(|v| v.trim().to_string()))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

fn truncate_for_event(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return input.to_string();
    }

    chars.into_iter().take(max_chars).collect::<String>() + "…(truncated)"
}

fn optional_trimmed_string(args: &Value, field: &str) -> Option<String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn required_trimmed_string(args: &Value, field: &str) -> Result<String, String> {
    let value = args
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{} is required", field))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{} is required", field));
    }
    Ok(trimmed.to_string())
}

fn canonical_or_original(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(path.as_path()).unwrap_or(path)
}

fn map_status_to_job_state(status: &str) -> &'static str {
    match status {
        "ok" => "done",
        "cancelled" => "cancelled",
        _ => "error",
    }
}

fn serialize_agent(agent: &AgentSpec) -> Value {
    json!({
        "id": agent.id,
        "name": agent.name,
        "description": agent.description.clone().unwrap_or_default(),
        "category": agent.category.clone().unwrap_or_default(),
        "skills": agent.skills.clone().unwrap_or_default(),
    })
}

fn serialize_commands(commands: &[CommandSpec]) -> Vec<Value> {
    commands
        .iter()
        .map(|cmd| {
            json!({
                "id": cmd.id,
                "name": cmd.name.clone().unwrap_or_default(),
                "description": cmd.description.clone().unwrap_or_default(),
            })
        })
        .collect()
}

fn with_chatos(server_name: &str, tool: &str, payload: Value, status: &str) -> Value {
    let mut object = payload.as_object().cloned().unwrap_or_default();
    object.insert(
        "chatos".to_string(),
        json!({ "status": status, "server": server_name, "tool": tool }),
    );
    Value::Object(object)
}

fn text_result(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [
            { "type": "text", "text": text }
        ]
    })
}

fn create_job(
    task: &str,
    agent_id: Option<String>,
    command_id: Option<String>,
    payload: Option<Value>,
    session_id: &str,
    run_id: &str,
) -> JobRecord {
    let now = Utc::now().to_rfc3339();
    let record = JobRecord {
        id: generate_id("job"),
        status: "queued".to_string(),
        task: task.to_string(),
        agent_id,
        command_id,
        payload_json: payload.map(|value| value.to_string()),
        result_json: None,
        error: None,
        created_at: now.clone(),
        updated_at: now,
        session_id: session_id.to_string(),
        run_id: run_id.to_string(),
    };

    if let Ok(mut jobs) = JOBS.lock() {
        jobs.insert(record.id.clone(), record.clone());
    }

    record
}

fn update_job_status(
    job_id: &str,
    status: &str,
    result_json: Option<String>,
    error: Option<String>,
) -> Option<JobRecord> {
    let mut jobs = JOBS.lock().ok()?;
    let job = jobs.get_mut(job_id)?;
    job.status = status.to_string();
    job.result_json = result_json;
    job.error = error;
    job.updated_at = Utc::now().to_rfc3339();
    Some(job.clone())
}

fn append_job_event(
    job_id: &str,
    event_type: &str,
    payload: Option<Value>,
    session_id: &str,
    run_id: &str,
) {
    let event = JobEvent {
        id: generate_id("event"),
        job_id: job_id.to_string(),
        r#type: event_type.to_string(),
        payload_json: payload.map(|value| value.to_string()),
        created_at: Utc::now().to_rfc3339(),
        session_id: session_id.to_string(),
        run_id: run_id.to_string(),
    };

    if let Ok(mut events) = JOB_EVENTS.lock() {
        events
            .entry(job_id.to_string())
            .or_insert_with(Vec::new)
            .push(event);
    }
}

fn list_job_events(job_id: &str) -> Vec<JobEvent> {
    JOB_EVENTS
        .lock()
        .ok()
        .and_then(|events| events.get(job_id).cloned())
        .unwrap_or_default()
}

fn get_cancel_flag(job_id: &str) -> Option<Arc<AtomicBool>> {
    JOB_CANCEL_FLAGS
        .lock()
        .ok()
        .and_then(|flags| flags.get(job_id).cloned())
}

fn block_on_result<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        let rt = tokio::runtime::Runtime::new().map_err(|err| err.to_string())?;
        rt.block_on(future)
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
