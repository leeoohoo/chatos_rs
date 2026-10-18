// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{memory::memory_plan, LocalAiStepPlanner, PreparedLocalAiStep};
use async_trait::async_trait;
use chatos_ai_runtime::{message_item, ContextualTurnRunner, ModelRuntimeConfig, RuntimeTurnSpec};
use chatos_local_agent_protocol::LocalAgentRunClaim;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::planner_support::{
    apply_run_thinking_level, completed_async_task_handoff, completed_task_outcome_report,
    durable_step_input, main_chat_scoped_tools, merge_instructions, merge_local_tools,
    validate_unique_tool_names,
};
use crate::planner_tools::task_scoped_tools;

#[cfg(test)]
use crate::planner_support::{requirement_survey_output_item, tool_name};

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

const TASK_EXECUTION_WORKSPACE_INSTRUCTIONS: &str = "[Local Workspace Boundary]\nWhen local project tools are available, path `.` is the authoritative root currently bound to this conversation. Treat the entries returned by `list_dir` for `.` as the root contents. Never replace that root with a child directory merely because its name appears to match the task or project; descend into a child only when the user's request requires inspecting that child, and keep root-relative paths and the distinction between the bound root and nested directories explicit in the final answer.";

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
                // The selected Plugin is program-owned Task scope, not a soft model hint.
                // Repeat it on every step so memory truncation or a tempting built-in
                // cannot silently change the requested delivery surface.
                capabilities.prefixed_input_items.push(message_item(
                    "system",
                    Value::String(
                        json!({
                            "type": "task_selected_plugin_contract",
                            "selected_plugins": selected_plugins,
                            "delivery_route": "selected_plugin_required",
                            "guidance": "The selected Plugin is the required primary delivery route for this Task. Discover, describe, activate, and invoke it. Built-in project reads may supply evidence. Notepad may only read an existing note explicitly bound as input; this Plugin Task cannot create or update notes or use Notepad as scratch storage. Do not report success until the selected Plugin has persisted and verified the requested deliverable."
                        })
                        .to_string(),
                    ),
                ));
            }
        }
        if self.profile_key == MAIN_CHAT_PROFILE_KEY {
            if let Some(selection) = claim.run.input.get("explicit_capability_selection") {
                let kind = selection.get("kind").and_then(Value::as_str);
                let contract = match kind {
                    Some("plugin") => Some(json!({
                        "type": "user_selected_capability_contract",
                        "selection": selection,
                        "delivery_route": "selected_plugin_required",
                        "guidance": "The user explicitly selected this Plugin in the composer. Create the requested durable Task with this exact Plugin as its required delivery route. Do not ask the user for a Plugin key or version, do not substitute Notepad or an ordinary file, and do not claim completion in Main Chat. The Host independently freezes and enforces the selected Plugin on every created Task."
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
