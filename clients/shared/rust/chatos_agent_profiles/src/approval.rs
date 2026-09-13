// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalAgentRun, ModelStepResult};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileStep, ModelGatewayOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::shared::{parse_tool_arguments, validate_context_strategy};

pub const APPROVAL_PROFILE_KEY: &str = "approval_review";
pub const APPROVAL_DECISION_TOOL: &str = "approval_decision";
pub const APPROVAL_PROMPT_REVISION: &str = "approval-review-v1";
pub const APPROVAL_CAPABILITY_SNAPSHOT_REF: &str = "approval-decision-v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalReviewInput {
    pub review_id: String,
    pub source: String,
    pub cwd: String,
    pub operation: String,
    pub requested_permissions_description: Option<String>,
    pub risk_level: String,
    pub risk_reason: Option<String>,
    pub reasoning_effort: Option<String>,
}

impl ApprovalReviewInput {
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("review ID", self.review_id.as_str()),
            ("source", self.source.as_str()),
            ("working directory", self.cwd.as_str()),
            ("operation", self.operation.as_str()),
            ("risk level", self.risk_level.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("approval {field} must not be empty"));
            }
        }
        for (field, value) in [
            (
                "requested permissions",
                self.requested_permissions_description.as_deref(),
            ),
            ("risk reason", self.risk_reason.as_deref()),
            ("reasoning effort", self.reasoning_effort.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty()) {
                return Err(format!("approval {field} must not be blank"));
            }
        }
        Ok(())
    }

    pub fn user_prompt(&self) -> String {
        format!(
            "Review this one local operation.\n\nsource: {}\ncwd: {}\noperation: {}\nrequested_permissions: {}\nstatic_risk_level: {}\nstatic_risk_reason: {}",
            self.source,
            self.cwd,
            self.operation,
            self.requested_permissions_description.as_deref().unwrap_or("null"),
            self.risk_level,
            self.risk_reason.as_deref().unwrap_or("none")
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalReviewStepContext {
    pub input: ApprovalReviewInput,
    pub model_input_items: Vec<Value>,
    pub maximum_output_tokens: u32,
    pub native_compaction_threshold: Option<u64>,
    pub memory_engine_active_threshold: Option<u64>,
    pub maximum_summary_attempts: u8,
}

#[async_trait]
pub trait ApprovalReviewContextProvider: Send + Sync {
    async fn load_step_context(
        &self,
        run: &LocalAgentRun,
    ) -> Result<ApprovalReviewStepContext, String>;
}

pub struct ApprovalReviewAgentProfile {
    context_provider: Arc<dyn ApprovalReviewContextProvider>,
}

impl ApprovalReviewAgentProfile {
    pub fn new(context_provider: Arc<dyn ApprovalReviewContextProvider>) -> Self {
        Self { context_provider }
    }
}

#[async_trait]
impl LocalAgentProfile for ApprovalReviewAgentProfile {
    fn profile_key(&self) -> &'static str {
        APPROVAL_PROFILE_KEY
    }

    async fn prepare_model_step(
        &self,
        run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        let context = self.context_provider.load_step_context(run).await?;
        context.input.validate()?;
        validate_context_strategy(
            run,
            context.native_compaction_threshold,
            context.memory_engine_active_threshold,
            context.maximum_summary_attempts,
        )?;
        Ok(LocalAgentProfileStep {
            model_input_items: context.model_input_items,
            tools: vec![approval_decision_schema()],
            instructions: Some(approval_system_prompt().to_string()),
            maximum_output_tokens: context.maximum_output_tokens.min(1_200),
            reasoning_effort: context.input.reasoning_effort,
            temperature: Some(0.0),
            native_compaction_threshold: context.native_compaction_threshold,
            memory_engine_active_threshold: context.memory_engine_active_threshold,
            maximum_summary_attempts: context.maximum_summary_attempts,
        })
    }

    async fn interpret_completed_output(
        &self,
        _run: &LocalAgentRun,
        output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        interpret_approval_output(output)
    }
}

fn approval_system_prompt() -> &'static str {
    "You are ChatOS's local operation approval reviewer. Judge exactly one proposed shell, Browser CDP, Computer Use, or local Plugin operation. Never execute the operation and never claim to have inspected anything not present in the request. Approve only when intent and scope are clear and controlled. Deny only operations that are clearly malicious, unauthorized, or in conflict with the user's goal. When information is insufficient, paths are ambiguous, scope is broad, or consequences may be irreversible, choose ask_user. Browser and Computer Use opaque session or tab identifiers are normal boundaries and are not filenames. Your only valid response is one approval_decision function call."
}

fn approval_decision_schema() -> Value {
    json!({
        "type": "function",
        "name": APPROVAL_DECISION_TOOL,
        "description": "Submit the one terminal approval decision.",
        "parameters": {
            "type": "object",
            "properties": {
                "decision": {"type": "string", "enum": ["approve", "deny", "ask_user"]},
                "reason": {"type": "string"},
                "remember_allow": {"type": "boolean"}
            },
            "required": ["decision", "reason", "remember_allow"],
            "additionalProperties": false
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ApprovalDecisionKind {
    Approve,
    Deny,
    AskUser,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovalDecision {
    decision: ApprovalDecisionKind,
    reason: String,
    remember_allow: bool,
}

fn interpret_approval_output(output: &ModelGatewayOutput) -> Result<ModelStepResult, String> {
    let calls = output
        .terminal
        .output_items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .collect::<Vec<_>>();
    if calls.len() != 1
        || calls[0].get("name").and_then(Value::as_str) != Some(APPROVAL_DECISION_TOOL)
    {
        return Ok(ModelStepResult::Failed(json!({
            "reason": "approval_requires_one_terminal_decision"
        })));
    }
    let decision: ApprovalDecision = serde_json::from_value(parse_tool_arguments(calls[0])?)
        .map_err(|error| format!("approval decision is invalid: {error}"))?;
    if decision.reason.trim().is_empty() {
        return Err("approval decision reason must not be empty".to_string());
    }
    let (kind, remember_allow) = match decision.decision {
        ApprovalDecisionKind::Approve => ("approve", decision.remember_allow),
        ApprovalDecisionKind::Deny => ("deny", false),
        ApprovalDecisionKind::AskUser => ("ask_user", false),
    };
    Ok(ModelStepResult::Final(json!({
        "kind": "approval_decision",
        "decision": kind,
        "reason": decision.reason,
        "remember_allow": remember_allow,
    })))
}

#[cfg(test)]
mod tests {
    use chatos_local_agent_protocol::{
        ModelGatewayTerminal, ModelGatewayTerminalSource, ModelGatewayTerminalStatus,
    };

    use super::*;

    #[test]
    fn one_valid_decision_becomes_a_terminal_outcome() {
        let output = gateway_output(vec![json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": APPROVAL_DECISION_TOOL,
            "arguments": {"decision": "approve", "reason": "bounded", "remember_allow": true}
        })]);
        assert_eq!(
            interpret_approval_output(&output).unwrap(),
            ModelStepResult::Final(json!({
                "kind": "approval_decision",
                "decision": "approve",
                "reason": "bounded",
                "remember_allow": true,
            }))
        );
    }

    #[test]
    fn plain_text_or_multiple_calls_cannot_approve() {
        let text = gateway_output(Vec::new());
        assert!(matches!(
            interpret_approval_output(&text).unwrap(),
            ModelStepResult::Failed(_)
        ));
        let multiple = gateway_output(vec![
            json!({"type":"function_call","name":APPROVAL_DECISION_TOOL,"arguments":{"decision":"approve","reason":"a","remember_allow":false}}),
            json!({"type":"function_call","name":APPROVAL_DECISION_TOOL,"arguments":{"decision":"approve","reason":"b","remember_allow":false}}),
        ]);
        assert!(matches!(
            interpret_approval_output(&multiple).unwrap(),
            ModelStepResult::Failed(_)
        ));
    }

    fn gateway_output(items: Vec<Value>) -> ModelGatewayOutput {
        ModelGatewayOutput {
            content: "ignored".to_string(),
            reasoning: String::new(),
            output_items: items.clone(),
            terminal: ModelGatewayTerminal {
                status: ModelGatewayTerminalStatus::Completed,
                source: ModelGatewayTerminalSource::Provider,
                response_id: Some("response-1".to_string()),
                provider_request_id: Some("request-1".to_string()),
                terminal_event: "response.completed".to_string(),
                provider_http_status: Some(200),
                usage: None,
                output_items: items,
                incomplete_details: None,
                provider_error: None,
            },
        }
    }
}
