// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalAiStepPlanner, PreparedLocalAiStep};
use async_trait::async_trait;
use chatos_ai_runtime::{
    append_responses_history_items, message_item, user_text_item, ContextualTurnRunner,
    ModelRuntimeConfig, RuntimeTurnSpec,
};
use chatos_local_agent_protocol::{LocalAgentRunClaim, LocalConversationAttachmentSpec};
use serde_json::{json, Value};
use std::sync::Arc;

pub const MAIN_CHAT_PROFILE_KEY: &str = "main_chat";
pub const TASK_RUNNER_PROFILE_KEY: &str = "task_runner";

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
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<TransientLocalModelRuntime, String> {
        (**self)
            .resolve_model_runtime(model_config_ref, model_config_revision)
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
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<ResolvedLocalCapabilities, String> {
        (**self)
            .resolve_capabilities(profile_key, capability_policy_revision)
            .await
    }
}

pub struct ControlPlaneLocalAiStepPlanner {
    profile_key: &'static str,
    initial_text_field: &'static str,
    model_resolver: Arc<dyn LocalModelRuntimeResolver>,
    capability_resolver: Arc<dyn LocalCapabilityResolver>,
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

    pub fn task_runner<M, C>(model_resolver: M, capability_resolver: C) -> Self
    where
        M: LocalModelRuntimeResolver + 'static,
        C: LocalCapabilityResolver + 'static,
    {
        Self::new(
            TASK_RUNNER_PROFILE_KEY,
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
        }
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
                &claim.run.model_config_ref,
                &claim.run.model_config_revision,
            )
            .await?;
        let capabilities = self
            .capability_resolver
            .resolve_capabilities(self.profile_key, &claim.run.capability_policy_revision)
            .await?;
        transient.model_config.instructions = merge_instructions(
            capabilities.instructions,
            transient.model_config.instructions,
        );
        let (current_input_items, reason) = durable_step_input(claim, self.initial_text_field)?;
        let caller_model = transient.model_config.model.clone();
        let request =
            RuntimeTurnSpec::new(transient.model_config, claim.run.owner_entity_id.clone())
                .with_conversation_turn_id(claim.run.run_id.clone())
                .with_caller_model(caller_model)
                .with_prefixed_input_items(capabilities.prefixed_input_items)
                .with_current_input_items(current_input_items)
                .with_tools(capabilities.tools)
                .into_contextual_turn_request();
        Ok(PreparedLocalAiStep {
            runner: transient.runner,
            request,
            reason,
        })
    }
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
            history.extend(tool_output_items(value)?);
            "tool_results".to_string()
        }
        Some(value) if value.get("type").and_then(Value::as_str) == Some("resume") => {
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("resumed");
            let input = value.get("input").cloned().unwrap_or(Value::Null);
            history.push(user_text_item(format!(
                "Local Agent resumed ({reason}). User input: {}",
                input
            )));
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

fn tool_output_items(continuation: &Value) -> Result<Vec<Value>, String> {
    let invocations = continuation
        .get("invocations")
        .and_then(Value::as_array)
        .ok_or_else(|| "tool continuation requires invocations".to_string())?;
    invocations
        .iter()
        .map(|invocation| {
            let call_id = invocation
                .get("call_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "tool result requires call_id".to_string())?;
            let output = invocation
                .get("result")
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or_else(
                    || json!({"error": invocation.get("error").cloned().unwrap_or(Value::Null)}),
                );
            Ok(json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": output.to_string()
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentRunStatus};

    fn claim(checkpoint: Value, continuation_input: Option<Value>) -> LocalAgentRunClaim {
        LocalAgentRunClaim {
            worker_id: "worker".to_string(),
            claim_token: "token".to_string(),
            run: LocalAgentRunRecord {
                run_id: "run-1".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: MAIN_CHAT_PROFILE_KEY.to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "hello"}),
                status: LocalAgentRunStatus::ModelRunning,
                iteration: 2,
                model_attempt: 1,
                max_iterations: 8,
                version: 4,
                claim_token: Some("token".to_string()),
                claim_until_unix_ms: Some(20_000),
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                checkpoint,
                continuation_input,
                terminal_outcome: None,
                created_at_unix_ms: 1,
                updated_at_unix_ms: 2,
            },
        }
    }

    #[test]
    fn reconstructs_tool_results_from_checkpoint_and_continuation() {
        let claim = claim(
            json!({"response": {
                "request_input_items": [{"role": "user", "content": "hello"}],
                "response_output_items": [{"type": "function_call", "call_id": "call-1"}]
            }}),
            Some(json!({
                "type": "tool_results",
                "invocations": [{"call_id": "call-1", "result": {"content": "ok"}}]
            })),
        );
        let (items, reason) = durable_step_input(&claim, "message").expect("input");
        assert_eq!(reason, "tool_results");
        assert_eq!(
            items.last().and_then(|item| item.get("call_id")),
            Some(&json!("call-1"))
        );
        assert_eq!(
            items.last().and_then(|item| item.get("type")),
            Some(&json!("function_call_output"))
        );
    }

    #[test]
    fn initial_retry_reuses_durable_run_input() {
        let mut claim = claim(Value::Null, None);
        claim.run.model_attempt = 2;
        let (items, reason) = durable_step_input(&claim, "message").expect("input");
        assert_eq!(reason, "model_retry");
        assert_eq!(items[0]["role"], "user");
    }

    #[test]
    fn guidance_is_appended_to_initial_or_existing_history() {
        let guidance = Some(json!({
            "type": "guidance",
            "guidance": [{"message_id": "message-2", "message": "inspect tests", "attachments": []}]
        }));
        let initial = claim(Value::Null, guidance.clone());
        let (items, reason) = durable_step_input(&initial, "message").expect("initial guidance");
        assert_eq!(reason, "initial_request_with_guidance");
        assert_eq!(items.len(), 2);

        let continued = claim(
            json!({"response": {
                "request_input_items": [{"role": "user", "content": "hello"}],
                "response_output_items": [{"type": "message", "content": "working"}]
            }}),
            guidance,
        );
        let (items, reason) =
            durable_step_input(&continued, "message").expect("continued guidance");
        assert_eq!(reason, "user_guidance");
        assert!(items.len() >= 3);
    }

    #[test]
    fn attachment_only_input_becomes_an_opaque_local_resource_manifest() {
        let mut claim = claim(Value::Null, None);
        claim.run.input = json!({
            "message": "",
            "attachments": [{
                "attachment_id": "attachment-1",
                "display_name": "brief.pdf",
                "media_type": "application/pdf",
                "byte_size": 42,
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "authorized_local_ref": "local-attachment:authority-1",
                "metadata": {"must_not_be_forwarded": true}
            }]
        });

        let (items, reason) = durable_step_input(&claim, "message").expect("input");
        assert_eq!(reason, "initial_request");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["role"], "user");
        let manifest = items[0]["content"][0]["text"]
            .as_str()
            .expect("manifest text");
        assert!(manifest.contains("local-attachment:authority-1"));
        assert!(manifest.contains("brief.pdf"));
        assert!(!manifest.contains("must_not_be_forwarded"));
    }
}
