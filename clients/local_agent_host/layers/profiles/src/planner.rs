// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    memory::{external_tool_results, memory_plan},
    LocalAiStepPlanner, PreparedLocalAiStep,
};
use async_trait::async_trait;
use chatos_ai_runtime::model_config::normalize_thinking_level;
use chatos_ai_runtime::{
    append_responses_history_items, build_tool_output_items, message_item, user_text_item,
    ContextualTurnRunner, ModelRuntimeConfig, RuntimeTurnSpec,
};
use chatos_local_agent_protocol::{LocalAgentRunClaim, LocalConversationAttachmentSpec};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc};

use crate::planner_tools::task_scoped_tools;

pub const MAIN_CHAT_PROFILE_KEY: &str = "main_chat";
pub const TASK_EXECUTION_PROFILE_KEY: &str = "task_execution";

const TASK_EXECUTION_WORKSPACE_INSTRUCTIONS: &str = "[Local Workspace Boundary]\nWhen local project tools are available, path `.` is the authoritative root currently bound to this conversation. Treat the entries returned by `list_dir` for `.` as the root contents. Never replace that root with a child directory merely because its name appears to match the task or project; descend into a child only when the user's request requires inspecting that child, and keep root-relative paths and the distinction between the bound root and nested directories explicit in the final answer.";

const MAIN_CHAT_TASK_REVISION_INSTRUCTIONS: &str = "[Latest User Intent and Executing Work]\nBefore arranging work or answering a follow-up that adds, changes, replaces, or cancels requirements, use list_tasks and get_task to inspect related non-terminal tasks in this conversation. Compare their actual objectives with the latest user message, including same-turn guidance. A verbal acknowledgement does not update an executing task. If requirements conflict or replace earlier work, call cancel_task on the affected tasks first and verify its result before claiming they stopped. Inspect get_task_dependency_graph and stop affected downstream work too; do not stop unrelated tasks. For a clear correction, create corrected work only after the affected old tasks are confirmed cancelled, passing their ids as supersedes_task_ids, and include the full revised objective and the need to inspect existing side effects. For an explicit stop, cancel without creating replacement work. For an ambiguous target or correction, ask for clarification; do not broadly cancel. Status inquiries and unrelated requests must not cancel tasks. Preserve old execution history. Cancellation prevents further durable execution but cannot undo already-completed files, external operations, or an in-flight external call; never promise rollback. Confirm what actually changed or stopped based on tool results. Never close a handoff while new guidance remains unhandled.";

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
        }
        capabilities
            .prefixed_input_items
            .extend(self.local_prefixed_input_items.clone());
        let configured_instructions = merge_instructions(
            capabilities.instructions,
            transient.model_config.instructions,
        );
        transient.model_config.instructions = if self.profile_key == TASK_EXECUTION_PROFILE_KEY {
            merge_instructions(
                Some(TASK_EXECUTION_WORKSPACE_INSTRUCTIONS.to_string()),
                configured_instructions,
            )
        } else if self.profile_key == MAIN_CHAT_PROFILE_KEY {
            merge_instructions(
                Some(MAIN_CHAT_TASK_REVISION_INSTRUCTIONS.to_string()),
                configured_instructions,
            )
        } else {
            configured_instructions
        };
        let (mut current_input_items, reason) = durable_step_input(claim, self.initial_text_field)?;
        let async_handoff_confirmed = self.profile_key == MAIN_CHAT_PROFILE_KEY
            && completed_async_task_handoff(claim.run.continuation_input.as_ref())
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
                    "[Continued Work Accepted]\n`wait_for_task_completion` has succeeded and the requested work is continuing independently. Do not call any tool, inspect execution status, wait for completion, or claim that the requested work is finished. Immediately respond in the contact's first-person voice with one concise, natural sentence saying that you have started working on the request, then end the turn. Do not mention tasks, Task Runner, background work, callbacks, tool calls, handoffs, or any internal execution structure to the user."
                        .to_string(),
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

fn merge_local_tools(
    mut controlled: Vec<Value>,
    local: &[Value],
    local_prefixes: &[String],
) -> Result<Vec<Value>, String> {
    let mut merged_local = local.to_vec();
    for tool in &mut merged_local {
        let Some(name) = tool_name(tool) else {
            continue;
        };
        let Some(controlled_tool) = controlled
            .iter()
            .find(|candidate| tool_name(candidate) == Some(name))
        else {
            continue;
        };
        merge_task_selection_schema(tool, controlled_tool);
    }
    let local_names = validate_unique_tool_names(&merged_local)?;
    controlled.retain(|tool| {
        tool_name(tool).is_none_or(|name| {
            !local_names.contains(name)
                && !local_prefixes.iter().any(|prefix| name.starts_with(prefix))
        })
    });
    controlled.extend(merged_local);
    Ok(controlled)
}

fn merge_task_selection_schema(local: &mut Value, controlled: &Value) {
    let properties_pointer = match tool_name(local) {
        Some("create_task") => "/parameters/properties",
        Some("create_tasks_with_prerequisites") => "/parameters/properties/tasks/items/properties",
        _ => return,
    };
    let Some(controlled_properties) = controlled
        .pointer(properties_pointer)
        .and_then(Value::as_object)
    else {
        return;
    };
    let Some(local_properties) = local
        .pointer_mut(properties_pointer)
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    // The native client resolves the request-scoped Agent capability policy.
    // Keep the durable Rust implementations of the Task tools, but restore the
    // three selection fields from that trusted snapshot exactly as the former
    // Task Runner did. Hard-coding any of these here silently changes product
    // behavior when an administrator edits the Agent binding.
    for key in [
        "enabled_builtin_kinds",
        "external_mcp_config_ids",
        "plugin_hints",
    ] {
        if let Some(schema) = controlled_properties.get(key) {
            local_properties.insert(key.to_string(), schema.clone());
        }
    }
}

fn validate_unique_tool_names(tools: &[Value]) -> Result<HashSet<&str>, String> {
    let mut names = HashSet::with_capacity(tools.len());
    for tool in tools {
        let name = tool_name(tool)
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| "local tool definition is missing a name".to_string())?;
        if !names.insert(name) {
            return Err(format!("local tool definition is duplicated: {name}"));
        }
    }
    Ok(names)
}

fn tool_name(tool: &Value) -> Option<&str> {
    tool.get("name")
        .and_then(Value::as_str)
        .or_else(|| tool.pointer("/function/name").and_then(Value::as_str))
}

fn completed_async_task_handoff(continuation: Option<&Value>) -> bool {
    continuation
        .and_then(|value| value.get("invocations"))
        .and_then(Value::as_array)
        .is_some_and(|invocations| {
            invocations.iter().any(|invocation| {
                invocation.get("tool_name").and_then(Value::as_str)
                    == Some("wait_for_task_completion")
                    && invocation.get("status").and_then(Value::as_str) == Some("succeeded")
                    && invocation
                        .get("result")
                        .and_then(|result| result.get("accepted"))
                        .and_then(Value::as_bool)
                        == Some(true)
            })
        })
}

fn completed_task_outcome_report(continuation: Option<&Value>) -> bool {
    continuation
        .and_then(|value| value.get("invocations"))
        .and_then(Value::as_array)
        .is_some_and(|invocations| {
            invocations.iter().any(|invocation| {
                invocation.get("tool_name").and_then(Value::as_str)
                    == Some("task_run_process_report_outcome")
                    && invocation.get("status").and_then(Value::as_str) == Some("succeeded")
                    && invocation
                        .get("result")
                        .and_then(|result| result.get("reported"))
                        .and_then(Value::as_bool)
                        == Some(true)
            })
        })
}

fn apply_run_thinking_level(config: &mut ModelRuntimeConfig, input: &Value) -> Result<(), String> {
    let Some(settings) = input.get("runtime_settings").and_then(Value::as_object) else {
        return Ok(());
    };
    let enabled = settings
        .get("reasoning_enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| "runtime_settings.reasoning_enabled must be a boolean".to_string())?;
    let requested = if enabled {
        settings
            .get("selected_thinking_level")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "enabled runtime reasoning requires selected_thinking_level".to_string()
            })?
    } else {
        "none"
    };
    config.thinking_level = normalize_thinking_level(&config.provider, Some(requested))?;
    Ok(())
}

fn merge_instructions(controlled: Option<String>, configured: Option<String>) -> Option<String> {
    let controlled = controlled.and_then(non_empty);
    let configured = configured.and_then(non_empty);
    match (controlled, configured) {
        (Some(controlled), Some(configured)) => Some(format!("{controlled}\n\n{configured}")),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

fn durable_step_input(
    claim: &LocalAgentRunClaim,
    initial_text_field: &str,
) -> Result<(Vec<Value>, String), String> {
    let checkpoint = claim.run.checkpoint.as_object();
    let Some(response) = checkpoint.and_then(|value| value.get("response")) else {
        let (mut items, mut reason) = initial_step_input(
            &claim.run.input,
            initial_text_field,
            claim.run.model_attempt,
        )?;
        if append_guidance_items(&mut items, claim.run.continuation_input.as_ref())? {
            reason = format!("{reason}_with_guidance");
        }
        return Ok((items, reason));
    };
    let request_items = value_array(response, "request_input_items")?;
    let response_items = value_array(response, "response_output_items")?;
    let internal_items = checkpoint
        .and_then(|value| value.get("continuation"))
        .and_then(|value| value.get("input_items"))
        .map(array_value)
        .transpose()?
        .unwrap_or_default();
    let mut history = append_responses_history_items(
        Value::Array(request_items),
        &response_items,
        &internal_items,
    )
    .as_array()
    .cloned()
    .unwrap_or_default();
    let mut reason = match claim.run.continuation_input.as_ref() {
        Some(value) if value.get("type").and_then(Value::as_str) == Some("tool_results") => {
            history.extend(tool_output_items(claim)?);
            "tool_results".to_string()
        }
        Some(value) if value.get("type").and_then(Value::as_str) == Some("resume") => {
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("resumed");
            let input = value.get("input").cloned().unwrap_or(Value::Null);
            if reason == "ask_user_submitted"
                && input.get("source").and_then(Value::as_str) == Some("ask_user")
            {
                history.push(ask_user_output_item(&response_items, &input)?);
            } else {
                history.push(user_text_item(format!(
                    "Local Agent resumed ({reason}). User input: {}",
                    input
                )));
            }
            "user_resume".to_string()
        }
        Some(value) if value.get("type").and_then(Value::as_str) == Some("guidance") => {
            "user_guidance".to_string()
        }
        Some(_) => return Err("unsupported durable continuation payload".to_string()),
        None if claim.run.model_attempt > 1 => "model_retry".to_string(),
        None => checkpoint
            .and_then(|value| value.get("continuation"))
            .and_then(|value| value.get("reason"))
            .and_then(Value::as_str)
            .unwrap_or("durable_continuation")
            .to_string(),
    };
    if append_guidance_items(&mut history, claim.run.continuation_input.as_ref())?
        && reason != "user_guidance"
    {
        reason = format!("{reason}_with_guidance");
    }
    Ok((history, reason))
}

fn initial_step_input(
    input: &Value,
    text_field: &str,
    model_attempt: u32,
) -> Result<(Vec<Value>, String), String> {
    if let Some(items) = input.get("input_items") {
        return Ok((array_value(items)?, initial_reason(model_attempt)));
    }
    let text = input
        .get(text_field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let attachments = input
        .get("attachments")
        .map(array_value)
        .transpose()?
        .unwrap_or_default();
    if text.is_none() && attachments.is_empty() {
        return Err(format!("run input requires {text_field} or input_items"));
    }
    Ok((
        vec![local_user_item(text, &attachments)?],
        initial_reason(model_attempt),
    ))
}

fn local_user_item(text: Option<&str>, attachments: &[Value]) -> Result<Value, String> {
    if attachments.is_empty() {
        return Ok(user_text_item(text.unwrap_or_default()));
    }
    let manifest = local_attachment_manifest(attachments)?;
    let mut content = Vec::with_capacity(2);
    if let Some(text) = text {
        content.push(json!({"type": "input_text", "text": text}));
    }
    content.push(json!({
        "type": "input_text",
        "text": format!(
            "Local attachments are available only through authorized local tools. \
             Treat each authorized_local_ref as an opaque capability and verify sha256 \
             before using content. Attachment manifest: {manifest}"
        )
    }));
    Ok(message_item("user", Value::Array(content)))
}

fn append_guidance_items(
    history: &mut Vec<Value>,
    continuation: Option<&Value>,
) -> Result<bool, String> {
    let Some(guidance) = continuation
        .and_then(|value| value.get("guidance"))
        .and_then(Value::as_array)
    else {
        return Ok(false);
    };
    for item in guidance {
        let text = item
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let attachments = item
            .get("attachments")
            .map(array_value)
            .transpose()?
            .unwrap_or_default();
        if text.is_none() && attachments.is_empty() {
            return Err("guidance requires a message or attachments".to_string());
        }
        history.push(local_user_item(text, &attachments)?);
    }
    Ok(!guidance.is_empty())
}

fn local_attachment_manifest(attachments: &[Value]) -> Result<String, String> {
    let records = attachments
        .iter()
        .map(|attachment| {
            let attachment: LocalConversationAttachmentSpec =
                serde_json::from_value(attachment.clone())
                    .map_err(|error| format!("decode local attachment failed: {error}"))?;
            attachment.validate()?;
            Ok(json!({
                "attachment_id": attachment.attachment_id,
                "display_name": attachment.display_name,
                "media_type": attachment.media_type,
                "byte_size": attachment.byte_size,
                "sha256": attachment.sha256,
                "authorized_local_ref": attachment.authorized_local_ref,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    serde_json::to_string(&records)
        .map_err(|error| format!("serialize local attachment manifest failed: {error}"))
}

fn initial_reason(model_attempt: u32) -> String {
    if model_attempt > 1 {
        "model_retry"
    } else {
        "initial_request"
    }
    .to_string()
}

fn value_array(value: &Value, field: &str) -> Result<Vec<Value>, String> {
    value
        .get(field)
        .map(array_value)
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn array_value(value: &Value) -> Result<Vec<Value>, String> {
    value
        .as_array()
        .cloned()
        .ok_or_else(|| "durable input items must be an array".to_string())
}

fn tool_output_items(claim: &LocalAgentRunClaim) -> Result<Vec<Value>, String> {
    Ok(build_tool_output_items(
        external_tool_results(claim)?.as_slice(),
    ))
}

fn ask_user_output_item(response_items: &[Value], input: &Value) -> Result<Value, String> {
    let explicit_call_id = input
        .get("tool_call_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let call_id = explicit_call_id
        .or_else(|| {
            response_items.iter().rev().find_map(|item| {
                let name = item.get("name").and_then(Value::as_str)?;
                name.starts_with("ask_user_")
                    .then(|| item.get("call_id").and_then(Value::as_str))
                    .flatten()
            })
        })
        .ok_or_else(|| "Ask User resume has no matching function call".to_string())?;
    let output = json!({
        "status": "ok",
        "values": input.get("values").cloned().unwrap_or_else(|| json!({})),
        "selection": input.get("selection").cloned().unwrap_or(Value::Null)
    });
    Ok(json!({
        "type": "function_call_output",
        "call_id": call_id,
        "output": output.to_string()
    }))
}

#[cfg(test)]
#[path = "planner_tests.rs"]
mod tests;
