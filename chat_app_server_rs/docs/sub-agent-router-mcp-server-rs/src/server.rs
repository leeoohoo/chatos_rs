use crate::catalog::SubAgentCatalog;
use crate::config_store::ConfigStore;
use crate::job_store::JobStore;
use crate::runner::run_command;
use crate::selector::{pick_agent, PickOptions};
use crate::types::{AgentSpec, CommandSpec, McpServerConfig, SkillSpec};
use crate::utils::parse_command;
use serde_json::json;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

const SUBAGENT_GUARDRAIL: &str = "Tooling guard: sub-agents cannot call mcp_subagent_router_* or other sub-agent routing tools. Complete the task directly with available project/shell/task tools.";

pub struct ServerOptions {
    pub server_name: String,
    pub catalog: Arc<Mutex<SubAgentCatalog>>,
    pub job_store: Rc<RefCell<JobStore>>,
    pub config_store: Rc<RefCell<ConfigStore>>,
    pub default_session_id: String,
    pub default_run_id: String,
    pub db_path: String,
    pub timeout_ms: i64,
    pub max_output_bytes: i64,
    pub ai_timeout_ms: i64,
    pub ai_max_output_bytes: i64,
    pub async_registry: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

pub fn register_tools(server: &mut crate::mcp::McpServer, options: ServerOptions) {
    let workspace_name = options.server_name.clone();
    let catalog = options.catalog.clone();
    let job_store = options.job_store.clone();
    let config_store = options.config_store.clone();
    let default_session_id = options.default_session_id.clone();
    let default_run_id = options.default_run_id.clone();
    let timeout_ms = options.timeout_ms;
    let max_output_bytes = options.max_output_bytes;
    let ai_timeout_ms = options.ai_timeout_ms;
    let ai_max_output_bytes = options.ai_max_output_bytes;

    server.register_tool(
        "get_sub_agent",
        "Return details by agent_id (description, skills, commands, default command).",
        json!({
            "type": "object",
            "properties": { "agent_id": { "type": "string" } },
            "required": ["agent_id"]
        }),
        Box::new(move |args| {
            let agent_id = args
                .get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or("agent_id is required".to_string())?;
            let agent = {
                let guard = catalog
                    .lock()
                    .map_err(|_| "catalog lock poisoned".to_string())?;
                guard
                    .get_agent(agent_id)
                    .ok_or_else(|| format!("Sub-agent {agent_id} not found."))?
            };
            Ok(text_result(with_chatos(
                &workspace_name,
                "get_sub_agent",
                json!({
                    "agent": serialize_agent(&agent),
                    "commands": serialize_commands(agent.commands.as_ref().unwrap_or(&Vec::new())),
                    "default_command": agent.default_command.clone().unwrap_or_default()
                }),
                "ok",
            )))
        }),
    );

    let catalog = options.catalog.clone();
    let workspace_name = options.server_name.clone();
    server.register_tool(
        "suggest_sub_agent",
        "Pick the best sub-agent for a task using optional category/skills/command hints.",
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string" },
                "category": { "type": "string" },
                "skills": { "type": "array", "items": { "type": "string" } },
                "query": { "type": "string" },
                "command_id": { "type": "string" }
            },
            "required": ["task"]
        }),
        Box::new(move |args| {
            let task = args
                .get("task")
                .and_then(|v| v.as_str())
                .ok_or("task is required".to_string())?;
            let agents = {
                let guard = catalog
                    .lock()
                    .map_err(|_| "catalog lock poisoned".to_string())?;
                guard.list_agents()
            };
            if agents.is_empty() {
                return Ok(text_result(with_chatos(
                    &workspace_name,
                    "suggest_sub_agent",
                    json!({
                        "agent_id": serde_json::Value::Null,
                        "reason": "No sub-agents available. Load marketplace or registry.",
                        "skills": []
                    }),
                    "ok",
                )));
            }
            let skills = args.get("skills").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>()
            });
            let picked = pick_agent(
                &agents,
                PickOptions {
                    task: task.to_string(),
                    category: args.get("category").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    skills,
                    query: args.get("query").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    command_id: args.get("command_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                },
            );
            let Some(picked) = picked else {
                return Ok(text_result(with_chatos(
                    &workspace_name,
                    "suggest_sub_agent",
                    json!({
                        "agent_id": serde_json::Value::Null,
                        "reason": "No matching sub-agent. Add one to the registry or marketplace.",
                        "skills": []
                    }),
                    "ok",
                )));
            };
            let used_skills = resolve_skill_ids(&picked.used_skills, &picked.agent);
            Ok(text_result(with_chatos(
                &workspace_name,
                "suggest_sub_agent",
                json!({
                    "agent_id": picked.agent.id,
                    "agent_name": picked.agent.name,
                    "skills": used_skills,
                    "reason": picked.reason
                }),
                "ok",
            )))
        }),
    );

    let workspace_name = options.server_name.clone();
    let catalog = options.catalog.clone();
    server.register_tool(
        "run_sub_agent",
        "Select and run a sub-agent for a task (auto-pick or by agent_id).",
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string" },
                "agent_id": { "type": "string" },
                "category": { "type": "string" },
                "skills": { "type": "array", "items": { "type": "string" } },
                "model": { "type": "string" },
                "caller_model": { "type": "string" },
                "query": { "type": "string" },
                "command_id": { "type": "string" },
                "mcp_allow_prefixes": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["task"]
        }),
        Box::new(move |args| {
            let task = args
                .get("task")
                .and_then(|v| v.as_str())
                .ok_or("task is required".to_string())?;
            let agent_id = args.get("agent_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let command_id = args.get("command_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let skills = args.get("skills").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>()
            });

            let resolved = {
                let guard = catalog
                    .lock()
                    .map_err(|_| "catalog lock poisoned".to_string())?;
                resolve_agent_and_command(
                    task,
                    agent_id,
                    args.get("category").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    skills,
                    args.get("query").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    command_id.clone(),
                    &guard,
                )?
            };
            let agent = resolved.agent;
            let command = resolved.command;
            let used_skills = resolved.used_skills;
            let reason = resolved.reason;

            if command.is_none() && agent.system_prompt_path.is_none() {
                return Err(format!("Sub-agent {} has no runnable prompt or command.", agent.id));
            }

            let job = job_store
                .borrow()
                .create_job(task, Some(agent.id.clone()), command.as_ref().map(|c| c.id.clone()), Some(args.clone()))?;
            job_store
                .borrow()
                .update_job_status(&job.id, "running", None, None)?;
            job_store
                .borrow()
                .append_event(&job.id, "start", Some(json!({
                    "agent_id": agent.id,
                    "command_id": command.as_ref().map(|c| c.id.clone()).unwrap_or_default()
                })))?;

            let mcp_servers = config_store.borrow().list_mcp_servers();
            let allow_prefixes = resolve_allow_prefixes(args.get("mcp_allow_prefixes"), &config_store.borrow(), &mcp_servers);

            let run_env = build_env(
                task,
                &agent,
                command.as_ref(),
                &used_skills,
                &default_session_id,
                &default_run_id,
                args.get("query").and_then(|v| v.as_str()),
                args.get("model").and_then(|v| v.as_str()),
                args.get("caller_model").and_then(|v| v.as_str()),
                &allow_prefixes,
                &mcp_servers,
            );

            if let Some(cmd) = command.clone().and_then(|c| c.exec) {
                let result = run_command(
                    &cmd,
                    &run_env,
                    command.as_ref().and_then(|c| c.cwd.as_deref()),
                    timeout_ms,
                    max_output_bytes as usize,
                    None,
                    None,
                )?;
                let status = if result.exit_code.unwrap_or(0) == 0 && !result.timed_out {
                    "ok"
                } else {
                    "error"
                };
                let payload = json!({
                    "status": status,
                    "job_id": job.id,
                    "agent_id": agent.id,
                    "agent_name": agent.name,
                    "command_id": command.map(|c| c.id),
                    "skills": used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                    "reason": reason,
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
                    "timed_out": result.timed_out
                });
                let job_status = if status == "ok" { "done" } else { "error" };
                job_store
                    .borrow()
                    .update_job_status(&job.id, job_status, Some(payload.to_string()), result.error)?;
                job_store
                    .borrow()
                    .append_event(&job.id, "finish", Some(json!({
                        "status": job_status,
                        "exit_code": result.exit_code,
                        "signal": result.signal
                    })))?;
                return Ok(text_result(with_chatos(
                    &workspace_name,
                    "run_sub_agent",
                    payload,
                    status,
                )));
            }

            // AI command mode using SUBAGENT_LLM_CMD
            let llm_cmd = parse_command(std::env::var("SUBAGENT_LLM_CMD").ok().as_deref())
                .ok_or_else(|| "AI not configured. Set SUBAGENT_LLM_CMD.".to_string())?;
            let system_prompt = {
                let mut guard = catalog
                    .lock()
                    .map_err(|_| "catalog lock poisoned".to_string())?;
                build_system_prompt(&agent, &used_skills, command.as_ref(), &mut guard, &allow_prefixes)
            };
            let prompt = build_prompt(&system_prompt, task);
            let result = run_command(
                &llm_cmd,
                &run_env,
                None,
                ai_timeout_ms,
                ai_max_output_bytes as usize,
                Some(&prompt),
                None,
            )?;
            let status = if result.exit_code.unwrap_or(0) == 0 && !result.timed_out {
                "ok"
            } else {
                "error"
            };
            let payload = json!({
                "status": status,
                "job_id": job.id,
                "agent_id": agent.id,
                "agent_name": agent.name,
                "command_id": command.map(|c| c.id),
                "skills": used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                "reason": reason,
                "response": result.stdout.trim(),
                "stderr": result.stderr,
                "exit_code": result.exit_code,
                "signal": result.signal,
                "duration_ms": result.duration_ms,
                "started_at": result.started_at,
                "finished_at": result.finished_at,
                "stdout_truncated": result.stdout_truncated,
                "stderr_truncated": result.stderr_truncated,
                "error": result.error,
                "timed_out": result.timed_out
            });
            let job_status = if status == "ok" { "done" } else { "error" };
            job_store
                .borrow()
                .update_job_status(&job.id, job_status, Some(payload.to_string()), result.error)?;
            job_store
                .borrow()
                .append_event(&job.id, "finish", Some(json!({
                    "status": job_status,
                    "exit_code": result.exit_code,
                    "signal": result.signal
                })))?;
            Ok(text_result(with_chatos(
                &workspace_name,
                "run_sub_agent",
                payload,
                status,
            )))
        }),
    );

    let workspace_name = options.server_name.clone();
    let catalog = options.catalog.clone();
    let job_store = options.job_store.clone();
    let config_store = options.config_store.clone();
    let default_session_id = options.default_session_id.clone();
    let default_run_id = options.default_run_id.clone();
    let db_path = options.db_path.clone();
    let async_registry = options.async_registry.clone();
    server.register_tool(
        "start_sub_agent_async",
        "Start a sub-agent job asynchronously and return the job id.",
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string" },
                "agent_id": { "type": "string" },
                "category": { "type": "string" },
                "skills": { "type": "array", "items": { "type": "string" } },
                "model": { "type": "string" },
                "caller_model": { "type": "string" },
                "query": { "type": "string" },
                "command_id": { "type": "string" },
                "mcp_allow_prefixes": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["task"]
        }),
        Box::new(move |args| {
            let task = args
                .get("task")
                .and_then(|v| v.as_str())
                .ok_or("task is required".to_string())?
                .to_string();
            let agent_id = args.get("agent_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let command_id = args.get("command_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let skills = args.get("skills").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>()
            });

            let resolved = {
                let guard = catalog
                    .lock()
                    .map_err(|_| "catalog lock poisoned".to_string())?;
                resolve_agent_and_command(
                    &task,
                    agent_id,
                    args.get("category").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    skills,
                    args.get("query").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    command_id.clone(),
                    &guard,
                )?
            };
            let agent = resolved.agent;
            let command = resolved.command;
            let used_skills = resolved.used_skills;
            let reason = resolved.reason;

            if command.is_none() && agent.system_prompt_path.is_none() {
                return Err(format!("Sub-agent {} has no runnable prompt or command.", agent.id));
            }

            let job = job_store
                .borrow()
                .create_job(&task, Some(agent.id.clone()), command.as_ref().map(|c| c.id.clone()), Some(args.clone()))?;
            job_store
                .borrow()
                .update_job_status(&job.id, "running", None, None)?;
            job_store
                .borrow()
                .append_event(&job.id, "start", Some(json!({
                    "agent_id": agent.id,
                    "command_id": command.as_ref().map(|c| c.id.clone()).unwrap_or_default()
                })))?;

            let mcp_servers = config_store.borrow().list_mcp_servers();
            let allow_prefixes = resolve_allow_prefixes(args.get("mcp_allow_prefixes"), &config_store.borrow(), &mcp_servers);
            let system_prompt = {
                let mut guard = catalog
                    .lock()
                    .map_err(|_| "catalog lock poisoned".to_string())?;
                build_system_prompt(&agent, &used_skills, command.as_ref(), &mut guard, &allow_prefixes)
            };

            let run_env = build_env(
                &task,
                &agent,
                command.as_ref(),
                &used_skills,
                &default_session_id,
                &default_run_id,
                args.get("query").and_then(|v| v.as_str()),
                args.get("model").and_then(|v| v.as_str()),
                args.get("caller_model").and_then(|v| v.as_str()),
                &allow_prefixes,
                &mcp_servers,
            );

            let cancel_flag = Arc::new(AtomicBool::new(false));
            async_registry
                .lock()
                .map_err(|_| "async registry poisoned".to_string())?
                .insert(job.id.clone(), cancel_flag.clone());

            let job_id = job.id.clone();
            let db_path_clone = db_path.clone();
            let server_name_clone = workspace_name.clone();
            let agent_clone = agent.clone();
            let command_clone = command.clone();
            let used_skills_clone = used_skills.clone();
            let reason_clone = reason.clone();
            let run_env_clone = run_env.clone();
            let task_clone = task.clone();
            let default_session_id_clone = default_session_id.clone();
            let default_run_id_clone = default_run_id.clone();
            let async_registry_clone = async_registry.clone();

            std::thread::spawn(move || {
                let job_store = match JobStore::new(&db_path_clone, default_session_id_clone, default_run_id_clone) {
                    Ok(store) => store,
                    Err(err) => {
                        eprintln!("[{server_name_clone}] async job store error: {err}");
                        let _ = async_registry_clone.lock().map(|mut map| map.remove(&job_id));
                        return;
                    }
                };

                let mut status = "error".to_string();
                let payload = if let Some(cmd) = command_clone.clone().and_then(|c| c.exec) {
                    match run_command(
                        &cmd,
                        &run_env_clone,
                        command_clone.as_ref().and_then(|c| c.cwd.as_deref()),
                        timeout_ms,
                        max_output_bytes as usize,
                        None,
                        Some(cancel_flag.as_ref()),
                    ) {
                        Ok(result) => {
                            let cancelled = matches!(result.error.as_deref(), Some("cancelled"));
                            status = if cancelled {
                                "cancelled".to_string()
                            } else if result.exit_code.unwrap_or(0) == 0 && !result.timed_out {
                                "ok".to_string()
                            } else {
                                "error".to_string()
                            };
                            json!({
                                "status": status,
                                "job_id": job_id,
                                "agent_id": agent_clone.id,
                                "agent_name": agent_clone.name,
                                "command_id": command_clone.clone().map(|c| c.id),
                                "skills": used_skills_clone.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                                "reason": reason_clone,
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
                                "timed_out": result.timed_out
                            })
                        }
                        Err(err) => json!({
                            "status": "error",
                            "job_id": job_id,
                            "agent_id": agent_clone.id,
                            "agent_name": agent_clone.name,
                            "command_id": command_clone.clone().map(|c| c.id),
                            "skills": used_skills_clone.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                            "reason": reason_clone,
                            "error": err
                        }),
                    }
                } else {
                    let llm_cmd = parse_command(std::env::var("SUBAGENT_LLM_CMD").ok().as_deref());
                    match llm_cmd {
                        Some(cmd) => {
                            let prompt = build_prompt(&system_prompt, &task_clone);
                            match run_command(
                                &cmd,
                                &run_env_clone,
                                None,
                                ai_timeout_ms,
                                ai_max_output_bytes as usize,
                                Some(&prompt),
                                Some(cancel_flag.as_ref()),
                            ) {
                                Ok(result) => {
                                    let cancelled = matches!(result.error.as_deref(), Some("cancelled"));
                                    status = if cancelled {
                                        "cancelled".to_string()
                                    } else if result.exit_code.unwrap_or(0) == 0 && !result.timed_out {
                                        "ok".to_string()
                                    } else {
                                        "error".to_string()
                                    };
                                    json!({
                                        "status": status,
                                        "job_id": job_id,
                                        "agent_id": agent_clone.id,
                                        "agent_name": agent_clone.name,
                                        "command_id": command_clone.clone().map(|c| c.id),
                                        "skills": used_skills_clone.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                                        "reason": reason_clone,
                                        "response": result.stdout.trim(),
                                        "stderr": result.stderr,
                                        "exit_code": result.exit_code,
                                        "signal": result.signal,
                                        "duration_ms": result.duration_ms,
                                        "started_at": result.started_at,
                                        "finished_at": result.finished_at,
                                        "stdout_truncated": result.stdout_truncated,
                                        "stderr_truncated": result.stderr_truncated,
                                        "error": result.error,
                                        "timed_out": result.timed_out
                                    })
                                }
                                Err(err) => json!({
                                    "status": "error",
                                    "job_id": job_id,
                                    "agent_id": agent_clone.id,
                                    "agent_name": agent_clone.name,
                                    "command_id": command_clone.clone().map(|c| c.id),
                                    "skills": used_skills_clone.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                                    "reason": reason_clone,
                                    "error": err
                                }),
                            }
                        }
                        None => json!({
                            "status": "error",
                            "job_id": job_id,
                            "agent_id": agent_clone.id,
                            "agent_name": agent_clone.name,
                            "command_id": command_clone.clone().map(|c| c.id),
                            "skills": used_skills_clone.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                            "reason": reason_clone,
                            "error": "AI not configured. Set SUBAGENT_LLM_CMD."
                        }),
                    }
                };

                let job_status = if status == "ok" {
                    "done"
                } else if status == "cancelled" {
                    "cancelled"
                } else {
                    "error"
                };
                let _ = job_store.update_job_status(&job_id, job_status, Some(payload.to_string()), None);
                let _ = job_store.append_event(&job_id, "finish", Some(json!({
                    "status": job_status,
                })));
                let _ = async_registry_clone.lock().map(|mut map| map.remove(&job_id));
            });

            Ok(text_result(with_chatos(
                &workspace_name,
                "start_sub_agent_async",
                json!({
                    "status": "running",
                    "job_id": job.id,
                    "agent_id": agent.id,
                    "agent_name": agent.name,
                    "command_id": command.map(|c| c.id),
                    "skills": used_skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
                    "reason": reason
                }),
                "ok",
            )))
        }),
    );

    let job_store = options.job_store.clone();
    server.register_tool(
        "get_sub_agent_status",
        "Get sub-agent job status by job_id.",
        json!({
            "type": "object",
            "properties": { "job_id": { "type": "string" } },
            "required": ["job_id"]
        }),
        Box::new(move |args| {
            let job_id = args
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or("job_id is required".to_string())?;
            let job = job_store
                .borrow()
                .get_job(job_id)?
                .ok_or_else(|| format!("Job not found: {job_id}"))?;
            Ok(text_result(json!({ "job": job })))
        }),
    );

    let job_store = options.job_store.clone();
    let async_registry = options.async_registry.clone();
    server.register_tool(
        "cancel_sub_agent_job",
        "Cancel a running sub-agent job.",
        json!({
            "type": "object",
            "properties": { "job_id": { "type": "string" } },
            "required": ["job_id"]
        }),
        Box::new(move |args| {
            let job_id = args
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or("job_id is required".to_string())?;
            if let Ok(map) = async_registry.lock() {
                if let Some(flag) = map.get(job_id) {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            let job = job_store
                .borrow()
                .update_job_status(job_id, "cancelled", None, Some("cancelled".to_string()))?;
            Ok(text_result(json!({ "job": job })))
        }),
    );
}

struct ResolvedAgent {
    agent: AgentSpec,
    command: Option<CommandSpec>,
    used_skills: Vec<SkillSpec>,
    reason: String,
}

fn resolve_agent_and_command(
    task: &str,
    agent_id: Option<String>,
    category: Option<String>,
    skills: Option<Vec<String>>,
    query: Option<String>,
    command_id: Option<String>,
    catalog: &SubAgentCatalog,
) -> Result<ResolvedAgent, String> {
    if let Some(id) = agent_id {
        let agent = catalog
            .get_agent(&id)
            .ok_or_else(|| format!("Sub-agent {id} not found."))?;
        let command = catalog.resolve_command(&agent, command_id.as_deref());
        let used_skills = select_skills(&agent, skills, catalog);
        return Ok(ResolvedAgent {
            agent,
            command,
            used_skills,
            reason: id,
        });
    }
    let command_id_for_pick = command_id.clone();
    let picked = pick_agent(
        &catalog.list_agents(),
        PickOptions {
            task: task.to_string(),
            category,
            skills,
            query,
            command_id: command_id_for_pick,
        },
    )
    .ok_or_else(|| "No matching sub-agent. Add one to the registry or marketplace.".to_string())?;
    let command = catalog.resolve_command(&picked.agent, command_id.as_deref());
    let used_skills = select_skills(&picked.agent, Some(picked.used_skills.clone()), catalog);
    Ok(ResolvedAgent {
        agent: picked.agent,
        command,
        used_skills,
        reason: picked.reason,
    })
}

fn select_skills(agent: &AgentSpec, input: Option<Vec<String>>, catalog: &SubAgentCatalog) -> Vec<SkillSpec> {
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
    allow_prefixes: &[String],
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
                sections.push(format!("Command instructions:\n{command_prompt}"));
            }
        }
    }
    if !skills.is_empty() {
        let mut blocks = Vec::new();
        for skill in skills {
            let content = catalog.read_content(Some(&skill.path));
            if !content.is_empty() {
                blocks.push(format!("Skill: {}\n{}", skill.name, content));
            }
        }
        if !blocks.is_empty() {
            sections.push(format!("Skills:\n{}", blocks.join("\n\n")));
        }
    }
    if !allow_prefixes.is_empty() {
        sections.push(format!("Allowed MCP prefixes: {}", allow_prefixes.join(", ")));
    }
    sections.push(SUBAGENT_GUARDRAIL.to_string());
    sections.join("\n\n")
}

fn build_prompt(system_prompt: &str, task: &str) -> String {
    format!("SYSTEM:\n{system_prompt}\n\nUSER:\n{task}\n")
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
    mcp_servers: &[McpServerConfig],
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
        skills.iter().map(|s| s.id.clone()).collect::<Vec<_>>().join(","),
    );
    env_map.insert("SUBAGENT_SESSION_ID".to_string(), session_id.to_string());
    env_map.insert("SUBAGENT_RUN_ID".to_string(), run_id.to_string());
    env_map.insert(
        "SUBAGENT_CATEGORY".to_string(),
        agent.category.clone().unwrap_or_default(),
    );
    env_map.insert("SUBAGENT_QUERY".to_string(), query.unwrap_or("").to_string());
    env_map.insert("SUBAGENT_MODEL".to_string(), model.unwrap_or("").to_string());
    env_map.insert(
        "SUBAGENT_CALLER_MODEL".to_string(),
        caller_model.unwrap_or("").to_string(),
    );
    env_map.insert(
        "SUBAGENT_MCP_ALLOW_PREFIXES".to_string(),
        allow_prefixes.join(","),
    );
    if !mcp_servers.is_empty() {
        let summary: Vec<serde_json::Value> = mcp_servers
            .iter()
            .map(|s| {
                json!({
                    "id": s.id,
                    "name": s.name,
                    "transport": s.transport,
                    "command": s.command,
                    "args": s.args,
                    "endpoint_url": s.endpoint_url,
                    "headers_json": s.headers_json,
                })
            })
            .collect();
        env_map.insert("SUBAGENT_MCP_SERVERS".to_string(), serde_json::to_string(&summary).unwrap_or_default());
    }
    env_map
}

fn resolve_allow_prefixes(
    input: Option<&serde_json::Value>,
    config_store: &ConfigStore,
    mcp_servers: &[McpServerConfig],
) -> Vec<String> {
    if let Some(value) = input {
        if let Some(arr) = value.as_array() {
            let list: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect();
            if !list.is_empty() {
                return list;
            }
        }
    }
    let manual = config_store.get_allow_prefixes();
    if !manual.is_empty() {
        return manual;
    }
    if !mcp_servers.is_empty() {
        let mut prefixes = std::collections::HashSet::new();
        for server in mcp_servers.iter().filter(|s| s.enabled) {
            let name = normalize_mcp_name(&server.name);
            if !name.is_empty() {
                prefixes.insert(format!("mcp_{name}_"));
            }
        }
        return prefixes.into_iter().collect();
    }
    config_store.get_effective_allow_prefixes()
}

fn normalize_mcp_name(value: &str) -> String {
    let mut out = String::new();
    let mut prev = false;
    for ch in value.trim().to_lowercase().chars() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if valid {
            out.push(ch);
            prev = false;
        } else if !prev {
            out.push('_');
            prev = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn serialize_agent(agent: &AgentSpec) -> serde_json::Value {
    json!({
        "id": agent.id,
        "name": agent.name,
        "description": agent.description.clone().unwrap_or_default(),
        "category": agent.category.clone().unwrap_or_default(),
        "skills": agent.skills.clone().unwrap_or_default(),
    })
}

fn serialize_commands(commands: &[CommandSpec]) -> Vec<serde_json::Value> {
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

fn with_chatos(server_name: &str, tool: &str, payload: serde_json::Value, status: &str) -> serde_json::Value {
    let mut object = payload.as_object().cloned().unwrap_or_default();
    object.insert(
        "chatos".to_string(),
        json!({ "status": status, "server": server_name, "tool": tool }),
    );
    serde_json::Value::Object(object)
}

fn text_result(payload: serde_json::Value) -> serde_json::Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [
            { "type": "text", "text": text }
        ]
    })
}
