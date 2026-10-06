// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_ai_runtime::{
    message_item,
    tool_call::{clone_tool_call_arguments, extract_tool_call_id, extract_tool_call_name},
    AiRuntimeResult, AiSingleStepOutcome, ContextualTurnRequest, ContextualTurnRunner,
};
use chatos_local_agent_protocol::{LocalAgentRunClaim, LocalAgentStepOutcome, LocalAgentToolCall};
use chatos_local_agent_runtime::LocalAgentProfile;
use chatos_mcp_runtime::ToolResult;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

#[async_trait]
pub trait LocalAiStepExecutor: Send + Sync {
    async fn execute_ai_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<AiSingleStepOutcome, String>;
}

pub struct PreparedLocalAiStep {
    pub runner: Arc<ContextualTurnRunner>,
    pub request: ContextualTurnRequest,
    pub reason: String,
    pub external_tool_results: Vec<ToolResult>,
}

#[async_trait]
pub trait LocalAiStepPlanner: Send + Sync {
    /// Resolves the current model revision, prompt/context, tools and transient
    /// credentials for exactly one request. Long-lived secrets must not be
    /// copied into the Run checkpoint returned by this planner.
    async fn prepare_ai_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<PreparedLocalAiStep, String>;
}

pub struct ChatosAiRuntimeStepExecutor {
    planner: Arc<dyn LocalAiStepPlanner>,
}

impl ChatosAiRuntimeStepExecutor {
    pub fn new<P>(planner: P) -> Self
    where
        P: LocalAiStepPlanner + 'static,
    {
        Self {
            planner: Arc::new(planner),
        }
    }
}

#[async_trait]
impl LocalAiStepExecutor for ChatosAiRuntimeStepExecutor {
    async fn execute_ai_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<AiSingleStepOutcome, String> {
        let prepared = self.planner.prepare_ai_step(claim).await?;
        if prepared.reason.trim().is_empty() {
            return Err("Local AI step reason must not be empty".to_string());
        }
        prepared
            .runner
            .persist_external_tool_results(
                &prepared.request.runtime_options,
                prepared.external_tool_results.as_slice(),
            )
            .await?;
        let iteration = usize::try_from(claim.run.iteration)
            .map_err(|_| "Local Agent iteration exceeds usize".to_string())?;
        let model_attempt = usize::try_from(claim.run.model_attempt)
            .map_err(|_| "Local Agent model attempt exceeds usize".to_string())?;
        prepared
            .runner
            .execute_once(prepared.request, iteration, prepared.reason, model_attempt)
            .await
    }
}

pub trait ToolSafetyPolicy: Send + Sync {
    fn is_side_effecting(&self, tool_name: &str) -> bool;

    fn requires_approval(&self, tool_name: &str) -> bool {
        self.is_side_effecting(tool_name)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConservativeToolSafetyPolicy;

impl ToolSafetyPolicy for ConservativeToolSafetyPolicy {
    fn is_side_effecting(&self, _tool_name: &str) -> bool {
        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct NamedReadOnlyTools {
    read_only: HashSet<String>,
    approval_exempt: HashSet<String>,
}

impl NamedReadOnlyTools {
    pub fn new<I, S>(tool_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            read_only: tool_names
                .into_iter()
                .map(Into::into)
                .map(|value: String| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect(),
            approval_exempt: HashSet::new(),
        }
    }

    pub fn with_approval_exempt<I, S>(mut self, tool_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.approval_exempt.extend(
            tool_names
                .into_iter()
                .map(Into::into)
                .map(|value: String| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        );
        self
    }
}

impl ToolSafetyPolicy for NamedReadOnlyTools {
    fn is_side_effecting(&self, tool_name: &str) -> bool {
        !self.read_only.contains(tool_name)
    }

    fn requires_approval(&self, tool_name: &str) -> bool {
        self.is_side_effecting(tool_name)
            && !self.approval_exempt.contains(tool_name)
            // External MCP authorization is enforced by the request-scoped
            // native adapter and the MCP provider itself. The former Task
            // Runner did not add a second generic Host approval to every MCP
            // call; doing so here would also leave these calls permanently
            // pending because project approval handlers cannot interpret them.
            && !tool_name.starts_with("external_mcp__")
    }
}

type ProfileClock = Arc<dyn Fn() -> Result<i64, String> + Send + Sync>;

pub struct DurableAiProfile {
    executor: Arc<dyn LocalAiStepExecutor>,
    tool_safety: Arc<dyn ToolSafetyPolicy>,
    clock: ProfileClock,
}

impl DurableAiProfile {
    pub fn new<E, P>(executor: E, tool_safety: P) -> Self
    where
        E: LocalAiStepExecutor + 'static,
        P: ToolSafetyPolicy + 'static,
    {
        Self::with_clock(executor, tool_safety, Arc::new(system_now_unix_ms))
    }

    pub fn with_clock<E, P>(executor: E, tool_safety: P, clock: ProfileClock) -> Self
    where
        E: LocalAiStepExecutor + 'static,
        P: ToolSafetyPolicy + 'static,
    {
        Self {
            executor: Arc::new(executor),
            tool_safety: Arc::new(tool_safety),
            clock,
        }
    }
}

#[async_trait]
impl LocalAgentProfile for DurableAiProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        let outcome = self.executor.execute_ai_step(claim).await?;
        reduce_ai_step_outcome_for_claim(outcome, claim, (self.clock)()?, self.tool_safety.as_ref())
    }
}

fn reduce_ai_step_outcome_for_claim(
    outcome: AiSingleStepOutcome,
    claim: &LocalAgentRunClaim,
    now_unix_ms: i64,
    tool_safety: &dyn ToolSafetyPolicy,
) -> Result<LocalAgentStepOutcome, String> {
    if claim.run.profile_key == "task_execution"
        && claim.run.owner_entity_type == "task"
        && !task_outcome_reported(claim.run.continuation_input.as_ref())
    {
        return match outcome {
            AiSingleStepOutcome::Final(response) => Ok(LocalAgentStepOutcome::Continue {
                checkpoint: response_checkpoint(
                    response,
                    Some(json!({
                        "reason": "task_outcome_required",
                        "input_items": [task_outcome_report_required_message()]
                    })),
                ),
            }),
            outcome => reduce_ai_step_outcome(outcome, now_unix_ms, tool_safety),
        };
    }
    reduce_ai_step_outcome(outcome, now_unix_ms, tool_safety)
}

fn task_outcome_reported(continuation: Option<&Value>) -> bool {
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

fn task_outcome_report_required_message() -> Value {
    message_item(
        "system",
        Value::String(
            "[Task Outcome Required]\nThe previous response cannot finish this run because no explicit task outcome was reported. Call `task_run_process_report_outcome` now exactly once with `status` set to `succeeded`, `failed`, or `blocked` and a short concrete `reason`. Do not call any other tool. After that tool succeeds, provide the final user-facing response without doing more work."
                .to_string(),
        ),
    )
}

pub fn reduce_ai_step_outcome(
    outcome: AiSingleStepOutcome,
    now_unix_ms: i64,
    tool_safety: &dyn ToolSafetyPolicy,
) -> Result<LocalAgentStepOutcome, String> {
    match outcome {
        AiSingleStepOutcome::Final(response) => Ok(LocalAgentStepOutcome::Succeed {
            output: response_value(response),
        }),
        AiSingleStepOutcome::ToolCommand {
            response,
            tool_calls,
        } => {
            let calls = decode_tool_calls(&tool_calls, tool_safety)?;
            Ok(LocalAgentStepOutcome::WaitForTool {
                batch_id: Uuid::new_v4().to_string(),
                tool_calls: calls,
                checkpoint: response_checkpoint(response, None),
            })
        }
        AiSingleStepOutcome::Continue {
            response,
            input_items,
            reason,
        } => Ok(LocalAgentStepOutcome::Continue {
            checkpoint: response_checkpoint(
                response,
                Some(json!({"reason": reason, "input_items": input_items})),
            ),
        }),
        AiSingleStepOutcome::Retry {
            error,
            retry_kind,
            next_model_attempt,
            backoff_ms,
        } => {
            let backoff = i64::try_from(backoff_ms)
                .map_err(|_| "model retry backoff exceeds i64".to_string())?;
            let resume_at_unix_ms = now_unix_ms
                .checked_add(backoff)
                .ok_or_else(|| "model retry timestamp overflow".to_string())?;
            Ok(LocalAgentStepOutcome::Retry {
                resume_at_unix_ms,
                next_model_attempt: u32::try_from(next_model_attempt)
                    .map_err(|_| "model attempt exceeds u32".to_string())?,
                reason: json!({
                    "error": error,
                    "retry_kind": retry_kind,
                    "next_model_attempt": next_model_attempt
                })
                .to_string(),
            })
        }
        AiSingleStepOutcome::Failed { error } => Ok(LocalAgentStepOutcome::Fail {
            error,
            detail: json!({"phase": "model_step"}),
        }),
        AiSingleStepOutcome::Cancelled => Ok(LocalAgentStepOutcome::Pause {
            reason: "model step was cancelled before the Run cancellation committed".to_string(),
        }),
    }
}

fn decode_tool_calls(
    tool_calls: &Value,
    tool_safety: &dyn ToolSafetyPolicy,
) -> Result<Vec<LocalAgentToolCall>, String> {
    let calls = tool_calls
        .as_array()
        .ok_or_else(|| "AI runtime tool_calls must be an array".to_string())?;
    if calls.is_empty() {
        return Err("AI runtime returned an empty tool call batch".to_string());
    }
    calls
        .iter()
        .map(|call| {
            let call_id = extract_tool_call_id(call)
                .ok_or_else(|| "AI runtime tool call is missing call_id".to_string())?;
            let tool_name = extract_tool_call_name(call)
                .ok_or_else(|| "AI runtime tool call is missing name".to_string())?;
            let arguments = decode_arguments(clone_tool_call_arguments(call))?;
            Ok(LocalAgentToolCall {
                call_id: call_id.to_string(),
                tool_name: tool_name.to_string(),
                arguments,
                side_effecting: tool_safety.is_side_effecting(tool_name),
                requires_approval: tool_safety.requires_approval(tool_name),
            })
        })
        .collect()
}

fn decode_arguments(arguments: Value) -> Result<Value, String> {
    match arguments {
        Value::String(value) => {
            if value.trim().is_empty() {
                Ok(json!({}))
            } else {
                serde_json::from_str(&value)
                    .map_err(|error| format!("decode tool call arguments failed: {error}"))
            }
        }
        value => Ok(value),
    }
}

fn response_checkpoint(response: AiRuntimeResult, continuation: Option<Value>) -> Value {
    json!({
        "response": response_value(response),
        "continuation": continuation
    })
}

fn response_value(response: AiRuntimeResult) -> Value {
    json!({
        "content": response.content,
        "reasoning": response.reasoning,
        "tool_calls": response.tool_calls,
        "finish_reason": response.finish_reason,
        "usage": response.usage,
        "response_id": response.response_id,
        "response_output_items": response.response_output_items,
        "request_input_items": response.request_input_items
    })
}

fn system_now_unix_ms() -> Result<i64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?;
    i64::try_from(duration.as_millis()).map_err(|_| "Unix time overflow".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(tool_calls: Option<Value>) -> AiRuntimeResult {
        AiRuntimeResult {
            content: "done".to_string(),
            reasoning: Some("reason".to_string()),
            tool_calls,
            finish_reason: Some("stop".to_string()),
            usage: Some(json!({"input_tokens": 12})),
            response_id: Some("response-1".to_string()),
            response_output_items: vec![json!({"type": "message"})],
            request_input_items: vec![json!({"role": "user", "content": "hello"})],
        }
    }

    #[test]
    fn tool_outcome_maps_calls_and_uses_conservative_safety() {
        let tool_calls = json!([
            {
                "id": "call-read",
                "type": "function",
                "function": {"name": "read_file", "arguments": "{\"path\":\"README.md\"}"}
            },
            {
                "call_id": "call-write",
                "name": "write_file",
                "arguments": {"path": "result.txt"}
            }
        ]);
        let outcome = reduce_ai_step_outcome(
            AiSingleStepOutcome::ToolCommand {
                response: response(Some(tool_calls.clone())),
                tool_calls,
            },
            10_000,
            &NamedReadOnlyTools::new(["read_file"]),
        )
        .expect("reduce");
        let LocalAgentStepOutcome::WaitForTool { tool_calls, .. } = outcome else {
            panic!("expected tool outcome")
        };
        assert_eq!(tool_calls.len(), 2);
        assert!(!tool_calls[0].side_effecting);
        assert!(!tool_calls[0].requires_approval);
        assert!(tool_calls[1].side_effecting);
        assert!(tool_calls[1].requires_approval);
        assert_eq!(tool_calls[0].arguments["path"], "README.md");

        let exempt = NamedReadOnlyTools::new(["read_file"]).with_approval_exempt(["create_task"]);
        assert!(exempt.is_side_effecting("create_task"));
        assert!(!exempt.requires_approval("create_task"));
    }

    #[test]
    fn retry_is_scheduled_without_sleeping() {
        let outcome = reduce_ai_step_outcome(
            AiSingleStepOutcome::Retry {
                error: "busy".to_string(),
                retry_kind: "provider_busy".to_string(),
                next_model_attempt: 2,
                backoff_ms: 750,
            },
            10_000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("reduce");
        assert!(matches!(
            outcome,
            LocalAgentStepOutcome::Retry {
                resume_at_unix_ms: 10_750,
                ..
            }
        ));
    }

    #[test]
    fn final_response_keeps_provider_output_for_terminal_audit() {
        let outcome = reduce_ai_step_outcome(
            AiSingleStepOutcome::Final(response(None)),
            10_000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("reduce");
        let LocalAgentStepOutcome::Succeed { output } = outcome else {
            panic!("expected final outcome")
        };
        assert_eq!(output["content"], "done");
        assert_eq!(output["response_id"], "response-1");
        assert_eq!(output["request_input_items"][0]["role"], "user");
    }

    #[test]
    fn task_final_without_reported_outcome_continues_with_required_guidance() {
        let claim = LocalAgentRunClaim {
            worker_id: "worker".to_string(),
            claim_token: "token".to_string(),
            run: chatos_local_agent_protocol::LocalAgentRunRecord {
                run_id: "run-1".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "task".to_string(),
                owner_entity_id: "task-1".to_string(),
                profile_key: "task_execution".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"prompt": "do the work"}),
                status: chatos_local_agent_protocol::LocalAgentRunStatus::ModelRunning,
                iteration: 1,
                model_attempt: 1,
                max_iterations: 8,
                version: 2,
                claim_token: Some("token".to_string()),
                claim_until_unix_ms: Some(20_000),
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                checkpoint: Value::Null,
                continuation_input: None,
                terminal_outcome: None,
                created_at_unix_ms: 1,
                updated_at_unix_ms: 2,
            },
        };

        let outcome = reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(response(None)),
            &claim,
            10_000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("reduce Task final");

        let LocalAgentStepOutcome::Continue { checkpoint } = outcome else {
            panic!("Task must continue until its outcome is reported")
        };
        assert_eq!(
            checkpoint["continuation"]["reason"],
            "task_outcome_required"
        );
        assert!(checkpoint["continuation"]["input_items"][0]["content"]
            .as_str()
            .is_some_and(|text| text.contains("[Task Outcome Required]")));
    }

    #[test]
    fn successful_task_outcome_report_is_detected() {
        assert!(task_outcome_reported(Some(&json!({
            "type": "tool_results",
            "invocations": [{
                "tool_name": "task_run_process_report_outcome",
                "status": "succeeded",
                "result": {"reported": true}
            }]
        }))));
    }
}
