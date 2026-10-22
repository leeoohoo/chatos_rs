// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{memory::memory_plan, LocalAiStepPlanner, PreparedLocalAiStep};
use async_trait::async_trait;
use chatos_ai_runtime::{message_item, ContextualTurnRunner, ModelRuntimeConfig, RuntimeTurnSpec};
use chatos_local_agent_protocol::LocalAgentRunClaim;
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::planner_support::{
    apply_run_thinking_level, completed_async_task_handoff, completed_task_outcome_report,
    durable_step_input, main_chat_scoped_tools, merge_instructions, merge_local_tools, tool_name,
    validate_unique_tool_names,
};
use crate::planner_tools::task_scoped_tools;

#[cfg(test)]
use crate::planner_support::requirement_survey_output_item;

pub const MAIN_CHAT_PROFILE_KEY: &str = "main_chat";
pub const TASK_EXECUTION_PROFILE_KEY: &str = "task_execution";

fn runtime_clock_context() -> Value {
    let now = chrono::Local::now();
    message_item(
        "system",
        Value::String(
            json!({
                "type": "runtime_clock_context",
                "current_date": now.format("%Y-%m-%d").to_string(),
                "current_time": now.to_rfc3339(),
                "timezone": "host-local",
                "utc_offset": now.format("%:z").to_string(),
                "current_time_utc": now.with_timezone(&chrono::Utc).to_rfc3339()
            })
            .to_string(),
        ),
    )
}

const TASK_EXECUTION_WORKSPACE_INSTRUCTIONS: &str = "[Local Workspace Boundary]\nWhen project tools are available, path `.` is the authoritative root currently bound to this conversation. Treat the entries returned by `list_dir` for `.` as the root contents. Never replace that root with a child directory merely because its name appears to match the task or project; descend into a child only when the user's request requires inspecting that child, and keep root-relative paths and the distinction between the bound root and nested directories explicit in the final answer.";

const AGENT_PROJECT_PERMISSION: &str = "local.project.list";
const LEGACY_AGENT_PROJECT_PERMISSION: &str = "builtin.project-steward";
const AGENT_TASK_TOOLS: [&str; 8] = [
    "list_tasks",
    "get_task",
    "create_task",
    "create_tasks_with_prerequisites",
    "cancel_task",
    "wait_for_task_completion",
    "get_task_dependency_graph",
    "resolve_task_followup",
];

fn agent_direct_instructions(input: &Value) -> Result<Option<String>, String> {
    let Some(context) = input.get("agent_execution_context") else {
        return Ok(None);
    };
    let required = |field: &str| {
        context
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Agent execution context is missing {field}"))
    };
    let agent_name = required("agent_name")?;
    let agent_id = required("agent_id")?;
    let room_id = required("room_id")?;
    let role_prompt = required("role_prompt")?;
    let profession_key = required("profession_key")?;
    let profession_label = required("profession_label")?;
    let skill_ref = required("profession_skill_ref")?;
    let skill_sha256 = required("profession_skill_sha256")?;
    let skill_markdown = required("profession_skill_markdown")?;
    let permissions = context
        .get("default_skill_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "Agent execution context is missing default_skill_ids".to_string())?;
    let plugins = context
        .get("default_plugin_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "Agent execution context is missing default_plugin_ids".to_string())?;
    if context.get("schema_version").and_then(Value::as_u64) == Some(2) {
        let project_id = required("project_id")?;
        let team_name = required("team_name")?;
        let team_goal = required("team_goal")?;
        let team_role = required("team_role")?;
        let team_responsibility = required("team_responsibility")?;
        let is_project_manager = context
            .get("is_project_manager")
            .and_then(Value::as_bool)
            .ok_or_else(|| "Agent execution context is missing is_project_manager".to_string())?;
        return Ok(Some(format!(
            "[Bound ChatOS Team Agent Identity]\nAgent name: {agent_name}\nAgent id: {agent_id}\nTeam room id: {room_id}\nProject id: {project_id}\nTeam: {team_name}\nTeam goal: {team_goal}\nCurrent team role: {team_role}\nCurrent team responsibility: {team_responsibility}\nProject manager: {is_project_manager}\nProfession: {profession_label} ({profession_key})\nProfession Skill snapshot: {skill_ref}\nProfession Skill SHA-256: {skill_sha256}\nBound permission ids: {}\nBound plugin ids: {}\n\nIdentity and authority rules:\n- You are the bound member of the named project team, not the default Main Chat assistant and not a private-chat Agent. Keep this identity for the entire Run.\n- Work from the Human or Agent message delivered to this team lane and produce a concrete team response grounded in the actual result.\n- Your team role and responsibility are scoped to this team. Do not claim project-manager authority when Project manager is false.\n- The native client selected your model and thinking level from the persisted Agent profile. Never claim a different model or identity.\n- Permission ids and plugin ids are frozen data, not authority by themselves. Only tools actually present in this Run may be used.\n- Do not invent teammates, project state, completed work, messages, Todo items, or side effects.\n\n[Agent role prompt]\n{role_prompt}\n\n[Bound profession Skill]\n{skill_markdown}",
            Value::Array(permissions.clone()),
            Value::Array(plugins.clone()),
        )));
    }
    Ok(Some(format!(
        "[Bound ChatOS Agent Identity]\nAgent name: {agent_name}\nAgent id: {agent_id}\nDirect room id: {room_id}\nProfession: {profession_label} ({profession_key})\nProfession Skill snapshot: {skill_ref}\nProfession Skill SHA-256: {skill_sha256}\nBound permission ids: {}\nBound plugin ids: {}\n\nIdentity and authority rules:\n- You are the bound Agent above, not the default Main Chat assistant. Keep this identity for the entire Run.\n- The native client selected your model and thinking level from the persisted Agent profile. Never claim a different model or identity.\n- Permission ids and plugin ids are frozen data, not authority by themselves. Only tools actually present in this Run may be used.\n- Do not claim access to a project, staff management, a Plugin, or a side effect unless both the bound permission and an actual tool allow it.\n- Answer the Human in this private conversation. Do not invent another Agent or route the message through a Contact.\n\n[Agent role prompt]\n{role_prompt}\n\n[Bound profession Skill]\n{skill_markdown}",
        Value::Array(permissions.clone()),
        Value::Array(plugins.clone()),
    )))
}

fn scope_agent_direct_tools(mut tools: Vec<Value>, input: &Value) -> Vec<Value> {
    let Some(context) = input.get("agent_execution_context") else {
        tools.retain(|tool| tool_name(tool).is_none_or(|name| !name.starts_with("team_asset_")));
        return tools;
    };
    let is_team_agent = context.get("schema_version").and_then(Value::as_u64) == Some(2);
    let is_project_manager = context
        .get("is_project_manager")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let permissions = context
        .get("default_skill_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<std::collections::HashSet<_>>();
    let can_access_projects = permissions.contains(AGENT_PROJECT_PERMISSION)
        || permissions.contains(LEGACY_AGENT_PROJECT_PERMISSION);
    if !can_access_projects {
        tools.retain(|tool| tool_name(tool).is_none_or(|name| !AGENT_TASK_TOOLS.contains(&name)));
    }
    tools.retain(|tool| {
        tool_name(tool).is_none_or(|name| {
            if matches!(name, "team_asset_list" | "team_asset_get") {
                is_team_agent
            } else if matches!(
                name,
                "team_asset_create" | "team_asset_update" | "team_asset_archive"
            ) {
                is_team_agent && is_project_manager
            } else {
                true
            }
        })
    });
    tools
}

/// A model runtime resolved for one step. This type is deliberately neither
/// serializable nor debuggable because `model_config` may contain credentials.
pub struct TransientLocalModelRuntime {
    pub runner: Arc<ContextualTurnRunner>,
    pub model_config: ModelRuntimeConfig,
}

#[async_trait]
pub trait LocalModelRuntimeResolver: Send + Sync {
    async fn resolve_model_runtime(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<TransientLocalModelRuntime, String>;
}

#[async_trait]
impl<T> LocalModelRuntimeResolver for Arc<T>
where
    T: LocalModelRuntimeResolver + ?Sized,
{
    async fn resolve_model_runtime(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<TransientLocalModelRuntime, String> {
        (**self)
            .resolve_model_runtime(owner_user_id, model_config_ref, model_config_revision)
            .await
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedLocalCapabilities {
    pub instructions: Option<String>,
    pub prefixed_input_items: Vec<Value>,
    pub tools: Vec<Value>,
}

#[async_trait]
pub trait LocalCapabilityResolver: Send + Sync {
    async fn resolve_capabilities(
        &self,
        owner_user_id: &str,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<ResolvedLocalCapabilities, String>;
}

#[async_trait]
impl<T> LocalCapabilityResolver for Arc<T>
where
    T: LocalCapabilityResolver + ?Sized,
{
    async fn resolve_capabilities(
        &self,
        owner_user_id: &str,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<ResolvedLocalCapabilities, String> {
        (**self)
            .resolve_capabilities(owner_user_id, profile_key, capability_policy_revision)
            .await
    }
}

pub struct ControlPlaneLocalAiStepPlanner {
    profile_key: &'static str,
    initial_text_field: &'static str,
    model_resolver: Arc<dyn LocalModelRuntimeResolver>,
    capability_resolver: Arc<dyn LocalCapabilityResolver>,
    memory_source_id: Option<String>,
    local_tools: Vec<Value>,
    local_tool_prefixes: Vec<String>,
    local_prefixed_input_items: Vec<Value>,
    local_context_runtime: Option<Arc<LocalAgentRuntime>>,
}

impl ControlPlaneLocalAiStepPlanner {
    pub fn main_chat<M, C>(model_resolver: M, capability_resolver: C) -> Self
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
    {
        Self::new(
            MAIN_CHAT_PROFILE_KEY,
            "message",
            model_resolver,
            capability_resolver,
        )
    }

    pub fn task_execution<M, C>(model_resolver: M, capability_resolver: C) -> Self
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
    {
        Self::new(
            TASK_EXECUTION_PROFILE_KEY,
            "prompt",
            model_resolver,
            capability_resolver,
        )
    }

    fn new<M, C>(
        profile_key: &'static str,
        initial_text_field: &'static str,
        model_resolver: M,
        capability_resolver: C,
    ) -> Self
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
    {
        Self {
            profile_key,
            initial_text_field,
            model_resolver: Arc::new(model_resolver),
            capability_resolver: Arc::new(capability_resolver),
            memory_source_id: None,
            local_tools: Vec::new(),
            local_tool_prefixes: Vec::new(),
            local_prefixed_input_items: Vec::new(),
            local_context_runtime: None,
        }
    }

    /// Adds client-owned tools to every resolved capability revision. A local
    /// definition replaces a control-plane definition with the same name so
    /// retired server implementations cannot shadow the on-device executor.
    pub fn with_local_tools(mut self, tools: Vec<Value>) -> Result<Self, String> {
        validate_unique_tool_names(&tools)?;
        self.local_tools = tools;
        Ok(self)
    }

    /// Reserves a client-owned tool namespace. Control-plane definitions in
    /// that namespace are removed even when the local client intentionally
    /// supports only a smaller replacement surface.
    pub fn with_local_tool_prefixes<I, S>(mut self, prefixes: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.local_tool_prefixes = prefixes
            .into_iter()
            .map(Into::into)
            .map(|prefix: String| prefix.trim().to_string())
            .collect();
        if self
            .local_tool_prefixes
            .iter()
            .any(|prefix| prefix.is_empty())
        {
            return Err("local tool prefix must not be empty".to_string());
        }
        Ok(self)
    }

    pub fn with_local_prefixed_input_items(mut self, items: Vec<Value>) -> Self {
        self.local_prefixed_input_items = items;
        self
    }

    pub fn with_local_conversation_context(mut self, runtime: Arc<LocalAgentRuntime>) -> Self {
        self.local_context_runtime = Some(runtime);
        self
    }

    pub fn with_memory_source_id(mut self, source_id: impl Into<String>) -> Result<Self, String> {
        let source_id = source_id.into().trim().to_string();
        if source_id.is_empty() {
            return Err("Memory source_id must not be empty".to_string());
        }
        self.memory_source_id = Some(source_id);
        Ok(self)
    }
}

#[async_trait]
impl LocalAiStepPlanner for ControlPlaneLocalAiStepPlanner {
    async fn prepare_ai_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<PreparedLocalAiStep, String> {
        if claim.run.profile_key != self.profile_key {
            return Err(format!(
                "planner for {} cannot execute profile {}",
                self.profile_key, claim.run.profile_key
            ));
        }
        let mut transient = self
            .model_resolver
            .resolve_model_runtime(
                &claim.run.owner_user_id,
                &claim.run.model_config_ref,
                &claim.run.model_config_revision,
            )
            .await?;
        apply_run_thinking_level(&mut transient.model_config, &claim.run.input)?;
        let mut capabilities = self
            .capability_resolver
            .resolve_capabilities(
                &claim.run.owner_user_id,
                self.profile_key,
                &claim.run.capability_policy_revision,
            )
            .await?;
        capabilities.tools = merge_local_tools(
            capabilities.tools,
            &self.local_tools,
            &self.local_tool_prefixes,
        )?;
        if self.profile_key == TASK_EXECUTION_PROFILE_KEY {
            capabilities.tools = task_scoped_tools(capabilities.tools, &claim.run.input)?;
        } else if self.profile_key == MAIN_CHAT_PROFILE_KEY {
            capabilities.tools = main_chat_scoped_tools(capabilities.tools, &claim.run.input);
            capabilities.tools = scope_agent_direct_tools(capabilities.tools, &claim.run.input);
        }
        capabilities
            .prefixed_input_items
            .extend(self.local_prefixed_input_items.clone());
        // Refresh even for resumed Tasks; page text is not the runtime clock.
        capabilities
            .prefixed_input_items
            .push(runtime_clock_context());
        let configured_instructions = merge_instructions(
            capabilities.instructions,
            transient.model_config.instructions,
        );
        let configured_instructions = merge_instructions(
            agent_direct_instructions(&claim.run.input)?,
            configured_instructions,
        );
        transient.model_config.instructions = if self.profile_key == TASK_EXECUTION_PROFILE_KEY {
            merge_instructions(
                Some(TASK_EXECUTION_WORKSPACE_INSTRUCTIONS.to_string()),
                configured_instructions,
            )
        } else {
            configured_instructions
        };
        let (mut current_input_items, reason) = durable_step_input(claim, self.initial_text_field)?;
        if transient.model_config.supports_images == Some(false) {
            // Do not let an image-producing tool turn a text-only model request
            // into an invalid gateway call or imply that this model saw pixels.
            for item in &mut current_input_items {
                if let Some(parts) = item.get_mut("content").and_then(Value::as_array_mut) {
                    parts.retain(|part| {
                        !matches!(
                            part.get("type").and_then(Value::as_str),
                            Some("input_image" | "image_url")
                        )
                    });
                    if parts.is_empty() {
                        parts.push(json!({"type":"input_text", "text":"Image input withheld: the configured model does not support images."}));
                    }
                }
            }
            capabilities.prefixed_input_items.push(message_item("system", Value::String(
                "[Model image capability] This Run's configured model explicitly does not support images. Image parts are not sent to this model. For image-dependent work, explain this specific limitation and ask for an image-capable model or an authorized OCR alternative. Do not claim to have seen an image or create a Task merely to bypass this limitation.".into()
            )));
        }
        if self.profile_key == TASK_EXECUTION_PROFILE_KEY
            && claim
                .run
                .input
                .get("acceptance_report_version")
                .and_then(Value::as_u64)
                == Some(1)
        {
            // Program-owned frozen data is available even after history truncation.
            capabilities.prefixed_input_items.push(message_item(
                "user",
                Value::String(
                    json!({
                        "type":"task_acceptance_contract",
                        "acceptance_report_version":1,
                        "expected_outputs":claim.run.input.get("expected_outputs"),
                        "acceptance_criteria":claim.run.input.get("acceptance_criteria"),
                        "final_response_max_chars":claim.run.input.get("final_response_max_chars"),
                        "source_user_message":claim.run.input.get("source_user_message")
                    })
                    .to_string(),
                ),
            ));
        }
        if self.profile_key == TASK_EXECUTION_PROFILE_KEY {
            if let Some(selected_plugins) = claim
                .run
                .input
                .pointer("/tool_options/plugin_hints")
                .and_then(Value::as_array)
                .filter(|plugins| !plugins.is_empty())
            {
                // Selection freezes tool scope, not a new output requirement. Repeat the
                // distinction after truncation: research tools need not produce artifacts,
                // while a deliverable explicitly requested in a Plugin still belongs there.
                capabilities.prefixed_input_items.push(message_item(
                    "system",
                    Value::String(
                        json!({
                            "type": "task_selected_plugin_contract",
                            "selected_plugins": selected_plugins,
                            "completion_basis": "task_objective_and_acceptance_criteria",
                            "guidance": "Use the selected Plugins for their stated purpose; discover, describe, activate, and invoke the relevant tools. Tool selection does not add deliverables to the Task. Determine completion from the objective, expected_outputs, and acceptance_criteria. For example, a browser used to research a text report supplies evidence; it does not make a persisted screenshot mandatory. If the Task permits HTTP fallback, a failed browser call does not prevent success when that fallback actually satisfies the criteria; disclose the failure and any visual evidence limits. In contrast, if the Task requests a screenshot or a document persisted in a selected Plugin, create and verify that deliverable there; prose, Notepad, an unrelated successful tool call, or an ordinary file cannot substitute for it. Report blocked only when an actual required outcome remains unmet and needs external action, not merely because an optional tool or artifact failed."
                        })
                        .to_string(),
                    ),
                ));
            }
        }
        if self.profile_key == MAIN_CHAT_PROFILE_KEY {
            capabilities.prefixed_input_items.push(message_item(
                "system",
                Value::String(
                    json!({
                        "type": "current_task_status_contract",
                        "guidance": "Conversation text and retained Memory are historical evidence, not authoritative current Task state. Before answering any question about what a Task is doing now, whether it completed, failed, blocked, was cancelled, or was replaced, call get_task or list_tasks. get_task resolves a historical id through its replacement chain; report current_tasks as the current state."
                    })
                    .to_string(),
                ),
            ));
            if let (Some(runtime), Some(conversation_id)) = (
                self.local_context_runtime.as_ref(),
                claim
                    .run
                    .input
                    .get("conversation_id")
                    .and_then(Value::as_str),
            ) {
                let task_updates = runtime
                    .latest_task_context_items(&claim.run.owner_user_id, conversation_id)
                    .await
                    .map_err(|error| error.to_string())?;
                if !task_updates.is_empty() {
                    capabilities.prefixed_input_items.push(message_item(
                        "system",
                        Value::String(
                            json!({
                                "type": "local_conversation_fresh_context",
                                "conversation_id": conversation_id,
                                "authority": "local_committed_task_writebacks",
                                "guidance": "These are the newest committed local Task lifecycle facts. They override older conversation prose or retained Memory when discussing current Task status. For a status question, still call get_task or list_tasks so replacement chains and the current leaf are resolved before answering.",
                                "task_updates": task_updates,
                            })
                            .to_string(),
                        ),
                    ));
                }
            }
            if let Some(selection) = claim.run.input.get("explicit_capability_selection") {
                let kind = selection.get("kind").and_then(Value::as_str);
                let contract = match kind {
                    Some("plugin") => Some(json!({
                        "type": "user_selected_capability_contract",
                        "selection": selection,
                        "required_capability": "selected_plugin",
                        "completion_basis": "task_objective_and_acceptance_criteria",
                        "guidance": "The user explicitly selected this Plugin in the composer. Create the requested durable Task with this exact Plugin for its requested purpose. The Host independently freezes and enforces this selection on every created Task. Tool selection alone does not require a persisted artifact: browser research may deliver a sourced text report, whereas a document explicitly requested in a Plugin must be created and verified there, not substituted with Notepad or an ordinary file. Express the user's actual deliverable and evidence requirements in the objective and acceptance criteria; do not invent an artifact requirement merely because a Plugin is selected. Do not ask the user for a Plugin key or version, and do not claim completion in Main Chat."
                    })),
                    Some("requirement_survey") => Some(json!({
                        "type": "user_selected_capability_contract",
                        "selection": selection,
                        "delivery_route": "requirement_survey_direct",
                        "guidance": "The user explicitly selected Requirement Survey in the composer. Activate the requirement-discovery Skill and call requirement_survey_create directly in Main Chat. Do not create a survey placeholder Task and do not dispatch dependent work before the submitted answers and confirmed proposal."
                    })),
                    _ => None,
                };
                if let Some(contract) = contract {
                    capabilities
                        .prefixed_input_items
                        .push(message_item("system", Value::String(contract.to_string())));
                }
            }
        }
        if self.profile_key == TASK_EXECUTION_PROFILE_KEY {
            if let Some(instructions) = claim
                .run
                .input
                .get("retry_instructions")
                .and_then(Value::as_array)
                .filter(|instructions| !instructions.is_empty())
            {
                capabilities.prefixed_input_items.push(message_item(
                    "user",
                    Value::String(
                        json!({
                            "type": "task_retry_context",
                            "instructions": instructions,
                            "fresh_evidence_required": true,
                            "guidance": "This is a fresh Task retry. Treat conclusions and tool outputs from earlier Runs as historical only. Re-execute every read or validation needed to address the retry instructions. Do not repeat a previous blocker unless fresh evidence from this Run still demonstrates it."
                        })
                        .to_string(),
                    ),
                ));
            }
        }
        let pending_review = claim.run.input.get("task_followup_review");
        if let Some(plan) = claim.run.input.get("requirement_plan") {
            // Frozen program-owned context is sent on every step independently of
            // conversation memory truncation. Plan prose remains user/task data.
            capabilities.prefixed_input_items.push(message_item(
                "user",
                Value::String(
                    json!({"type": "requirement_plan_context", "plan": plan}).to_string(),
                ),
            ));
        }
        if let Some(review) = pending_review {
            // Program-owned facts only. New Runs get behavior from their frozen Product Skill.
            capabilities
                .prefixed_input_items
                .push(message_item("system", Value::String(review.to_string())));
        }
        let async_handoff_confirmed = self.profile_key == MAIN_CHAT_PROFILE_KEY
            && completed_async_task_handoff(claim.run.continuation_input.as_ref())
            && !pending_review
                .is_some_and(|review| review.get("pending").and_then(Value::as_bool) == Some(true))
            && !claim.run.continuation_input.as_ref().is_some_and(|value| {
                value
                    .get("guidance")
                    .and_then(Value::as_array)
                    .is_some_and(|items| !items.is_empty())
            });
        let task_outcome_reported = self.profile_key == TASK_EXECUTION_PROFILE_KEY
            && completed_task_outcome_report(claim.run.continuation_input.as_ref());
        if async_handoff_confirmed {
            // Preserve the former server Task Runner boundary exactly: after the
            // explicit handoff succeeds, Main Chat gets one tool-free response
            // whose only job is to acknowledge that work has started. Task
            // results arrive later through the normal per-Task callbacks.
            capabilities.tools.clear();
            current_input_items.push(message_item(
                "system",
                Value::String(
                    json!({"type": "continued_work_accepted", "accepted": true}).to_string(),
                ),
            ));
        } else if task_outcome_reported {
            // Match the former Task Runner lifecycle hook: once the explicit
            // outcome has been accepted, the next request is a tool-free final
            // response. The model may explain the completed work, but it may
            // not perform more work or revise the reported terminal status.
            capabilities.tools.clear();
            current_input_items.push(message_item(
                "system",
                Value::String(
                    "[Task Outcome Reported]\nThe task outcome has been recorded. Tools are now disabled. Provide the final user-facing response based on the completed work and the reported outcome. Do not perform more work or revise the reported status."
                        .to_string(),
                ),
            ));
        }
        let memory = self
            .memory_source_id
            .as_deref()
            .map(|source_id| memory_plan(claim, source_id, self.initial_text_field))
            .transpose()?;
        let caller_model = transient.model_config.model.clone();
        let conversation_id = memory
            .as_ref()
            .map(|memory| memory.thread_id.clone())
            .unwrap_or_else(|| claim.run.owner_entity_id.clone());
        let conversation_turn_id = memory
            .as_ref()
            .map(|memory| memory.turn_id.clone())
            .unwrap_or_else(|| claim.run.run_id.clone());
        let mut spec = RuntimeTurnSpec::new(transient.model_config, conversation_id)
            .with_conversation_turn_id(conversation_turn_id)
            .with_caller_model(caller_model)
            .with_prefixed_input_items(capabilities.prefixed_input_items)
            .with_current_input_items(current_input_items)
            .with_tools(capabilities.tools);
        if let Some(memory) = &memory {
            spec = spec
                .with_memory_scope(Some(memory.scope.clone()))
                .with_record_options(memory.record_options.clone())
                .with_user_record(memory.user_record.clone());
        }
        let request = spec.into_contextual_turn_request();
        Ok(PreparedLocalAiStep {
            runner: transient.runner,
            request,
            reason,
            external_tool_results: memory
                .map(|memory| memory.external_tool_results)
                .unwrap_or_default(),
        })
    }
}

#[cfg(test)]
#[path = "planner_tests.rs"]
mod tests;

#[cfg(test)]
mod agent_direct_tests {
    use super::*;

    fn input(permission_ids: &[&str]) -> Value {
        json!({
            "agent_execution_context": {
                "room_id": "room-1",
                "agent_id": "agent-1",
                "agent_name": "工程 Agent",
                "role_prompt": "负责工程实现。",
                "profession_key": "software_engineer",
                "profession_label": "软件工程师",
                "profession_skill_ref": "AS-profession-software_engineer-123456789abc",
                "profession_skill_sha256": "a".repeat(64),
                "profession_skill_markdown": "# 工作流\n先核实，再实现。",
                "default_plugin_ids": [],
                "default_skill_ids": permission_ids,
            }
        })
    }

    #[test]
    fn bound_agent_identity_and_skill_become_model_instructions() {
        let instructions = agent_direct_instructions(&input(&[])).unwrap().unwrap();
        assert!(instructions.contains("工程 Agent"));
        assert!(instructions.contains("负责工程实现"));
        assert!(instructions.contains("AS-profession-software_engineer"));
        assert!(instructions.contains("先核实，再实现"));
    }

    #[test]
    fn project_task_tools_require_the_persisted_agent_permission() {
        let tools = vec![
            json!({"name": "create_task"}),
            json!({"name": "agent_skill_activate"}),
            json!({"name": "read_conversation_attachment"}),
        ];
        let restricted = scope_agent_direct_tools(tools.clone(), &input(&[]));
        assert!(!restricted.iter().any(|tool| tool["name"] == "create_task"));
        assert!(restricted
            .iter()
            .any(|tool| tool["name"] == "agent_skill_activate"));

        let allowed = scope_agent_direct_tools(tools, &input(&[AGENT_PROJECT_PERMISSION]));
        assert!(allowed.iter().any(|tool| tool["name"] == "create_task"));
    }

    #[test]
    fn team_agent_identity_includes_scoped_role_without_claiming_manager_authority() {
        let mut team_input = input(&[AGENT_PROJECT_PERMISSION]);
        let context = team_input["agent_execution_context"]
            .as_object_mut()
            .unwrap();
        context.insert("schema_version".into(), json!(2));
        context.insert("project_id".into(), json!("project-1"));
        context.insert("team_name".into(), json!("Alpha 团队"));
        context.insert("team_goal".into(), json!("交付 Alpha"));
        context.insert("team_role".into(), json!("前端工程师"));
        context.insert("team_responsibility".into(), json!("负责 Windows UI"));
        context.insert("is_project_manager".into(), json!(false));

        let instructions = agent_direct_instructions(&team_input).unwrap().unwrap();
        assert!(instructions.contains("Bound ChatOS Team Agent Identity"));
        assert!(instructions.contains("Current team role: 前端工程师"));
        assert!(instructions.contains("Project manager: false"));
        assert!(instructions.contains("Do not claim project-manager authority"));
        assert!(!instructions.contains("private conversation"));
    }

    #[test]
    fn team_asset_tools_are_visible_only_to_the_bound_team_authority() {
        let tools = vec![
            json!({"name": "team_asset_list"}),
            json!({"name": "team_asset_get"}),
            json!({"name": "team_asset_create"}),
            json!({"name": "team_asset_update"}),
            json!({"name": "team_asset_archive"}),
        ];
        assert!(scope_agent_direct_tools(tools.clone(), &json!({})).is_empty());

        let mut member = input(&[]);
        member["agent_execution_context"]["schema_version"] = json!(2);
        member["agent_execution_context"]["is_project_manager"] = json!(false);
        let member_tools = scope_agent_direct_tools(tools.clone(), &member);
        assert_eq!(member_tools.len(), 2);
        assert!(member_tools.iter().all(|tool| matches!(
            tool["name"].as_str(),
            Some("team_asset_list" | "team_asset_get")
        )));

        member["agent_execution_context"]["is_project_manager"] = json!(true);
        assert_eq!(scope_agent_direct_tools(tools, &member).len(), 5);
    }
}
