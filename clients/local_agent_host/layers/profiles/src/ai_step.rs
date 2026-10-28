// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_ai_runtime::{
    message_item, prune_items_before_latest_compaction,
    tool_call::{clone_tool_call_arguments, extract_tool_call_id, extract_tool_call_name},
    AiRuntimeResult, AiSingleStepOutcome, ContextualTurnRequest, ContextualTurnRunner,
};
use chatos_local_agent_protocol::{
    task_acceptance_report_for_run, validate_task_acceptance_report, LocalAgentRunClaim,
    LocalAgentStepOutcome, LocalAgentToolCall,
};
use chatos_local_agent_runtime::LocalAgentProfile;
use chatos_mcp_runtime::ToolResult;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

#[async_trait]
pub trait LocalAiStepExecutor: Send + Sync {
    async fn execute_ai_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<ExecutedLocalAiStep, String>;
}

pub struct ExecutedLocalAiStep {
    pub outcome: AiSingleStepOutcome,
    pub tool_definitions: Vec<Value>,
}

#[async_trait]
pub trait LocalToolHookGate: Send + Sync {
    async fn gate_tool_outcome(
        &self,
        claim: &LocalAgentRunClaim,
        outcome: LocalAgentStepOutcome,
        tool_definitions: &[Value],
    ) -> Result<LocalAgentStepOutcome, String>;
}

#[async_trait]
impl<T> LocalToolHookGate for Arc<T>
where
    T: LocalToolHookGate + ?Sized,
{
    async fn gate_tool_outcome(
        &self,
        claim: &LocalAgentRunClaim,
        outcome: LocalAgentStepOutcome,
        tool_definitions: &[Value],
    ) -> Result<LocalAgentStepOutcome, String> {
        (**self)
            .gate_tool_outcome(claim, outcome, tool_definitions)
            .await
    }
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
    ) -> Result<ExecutedLocalAiStep, String> {
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
            .map_err(|_| "model attempt exceeds usize".to_string())?;
        let tool_definitions = prepared.request.model_request.tools.clone();
        let outcome = prepared
            .runner
            .execute_once(prepared.request, iteration, prepared.reason, model_attempt)
            .await?;
        Ok(ExecutedLocalAiStep {
            outcome,
            tool_definitions,
        })
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
    tool_hook_gate: Option<Arc<dyn LocalToolHookGate>>,
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
            tool_hook_gate: None,
        }
    }

    pub fn with_tool_hook_gate<G>(mut self, gate: G) -> Self
    where
        G: LocalToolHookGate + 'static,
    {
        self.tool_hook_gate = Some(Arc::new(gate));
        self
    }
}

#[async_trait]
impl LocalAgentProfile for DurableAiProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        let executed = self.executor.execute_ai_step(claim).await?;
        let outcome = reduce_ai_step_outcome_for_claim(
            executed.outcome,
            claim,
            (self.clock)()?,
            self.tool_safety.as_ref(),
        )?;
        match self.tool_hook_gate.as_ref() {
            Some(gate) => match gate
                .gate_tool_outcome(claim, outcome, &executed.tool_definitions)
                .await
            {
                Ok(outcome) => Ok(outcome),
                Err(error) => Ok(LocalAgentStepOutcome::NeedsReview {
                    reason: "trusted tool Hook gate failed closed".to_string(),
                    detail: json!({
                        "phase": "trusted_plugin_hook",
                        "error": error,
                    }),
                }),
            },
            None => Ok(outcome),
        }
    }
}

fn reduce_ai_step_outcome_for_claim(
    outcome: AiSingleStepOutcome,
    claim: &LocalAgentRunClaim,
    now_unix_ms: i64,
    tool_safety: &dyn ToolSafetyPolicy,
) -> Result<LocalAgentStepOutcome, String> {
    let outcome = if pending_task_replacement(claim) {
        match outcome {
            AiSingleStepOutcome::Final(response) => {
                return Ok(LocalAgentStepOutcome::Continue {
                    checkpoint: response_checkpoint(
                        response,
                        Some(json!({
                            "reason": "task_replacement_required",
                            "input_items": [task_replacement_required_message()]
                        })),
                    ),
                });
            }
            outcome => outcome,
        }
    } else {
        outcome
    };
    let outcome = if pending_task_followup_review(claim) {
        match outcome {
            AiSingleStepOutcome::Final(response) => {
                return Ok(LocalAgentStepOutcome::Continue {
                    checkpoint: response_checkpoint(
                        response,
                        Some(json!({
                            "reason": "task_followup_review_required",
                            "input_items": [task_followup_review_required_message()]
                        })),
                    ),
                });
            }
            outcome => outcome,
        }
    } else {
        outcome
    };
    if claim.run.profile_key == "task_execution"
        && claim.run.owner_entity_type == "task"
        && claim
            .run
            .input
            .get("acceptance_report_version")
            .and_then(Value::as_u64)
            == Some(1)
    {
        if let (AiSingleStepOutcome::Final(response), Some(report)) =
            (&outcome, task_acceptance_report_for_run(&claim.run))
        {
            validate_task_acceptance_report(&claim.run.input, report)?;
            // Do not let a second model response rewrite an already-checked deliverable.
            let mut output = response_value(response.clone());
            output["content"] = report["final_response"].clone();
            output["task_acceptance"] = report.clone();
            return Ok(LocalAgentStepOutcome::Succeed { output });
        }
    }
    if selected_plugin_task_handoff_required(claim) {
        if let AiSingleStepOutcome::Final(response) = outcome {
            return Ok(LocalAgentStepOutcome::Continue {
                checkpoint: response_checkpoint(
                    response,
                    Some(json!({
                        "reason": "selected_plugin_task_handoff_required",
                        "input_items": [selected_plugin_task_handoff_required_message()]
                    })),
                ),
            });
        }
    }
    if claim.run.profile_key == "task_execution"
        && claim.run.owner_entity_type == "task"
        && !task_outcome_reported(claim.run.continuation_input.as_ref())
        && task_acceptance_report_for_run(&claim.run).is_none()
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

fn pending_task_replacement(claim: &LocalAgentRunClaim) -> bool {
    claim.run.profile_key == "main_chat"
        && claim
            .run
            .input
            .pointer("/task_followup_review/replacement_pending")
            .and_then(Value::as_bool)
            == Some(true)
}

fn task_replacement_required_message() -> Value {
    message_item(
        "system",
        Value::String(
            "[Task Replacement Required]\nThe follow-up review marked one or more outdated Tasks for replacement. Do not finish this turn or revive the old contract. Create the corrected Task or Task graph now and include every held replacement candidate in supersedes_task_ids. Graph creation atomically cancels the held old Tasks, inserts the replacements, and records the replacement chain."
                .to_string(),
        ),
    )
}

fn pending_task_followup_review(claim: &LocalAgentRunClaim) -> bool {
    claim.run.profile_key == "main_chat"
        && claim
            .run
            .input
            .pointer("/task_followup_review/pending")
            .and_then(Value::as_bool)
            == Some(true)
}

fn task_followup_review_required_message() -> Value {
    message_item(
        "system",
        Value::String(
            "[Task Follow-up Review Required]\nThe previous response cannot finish this Main Chat turn because the current task_followup_review is still pending. Do not repeat or defend that response. Call `resolve_task_followup` now with the authoritative current review_message_id and exactly one decision for every pending candidate. Use the user's latest message and current Task facts to choose continue, cancel, or clarify; do not create replacement work until the review succeeds."
                .to_string(),
        ),
    )
}

fn selected_plugin_task_handoff_required(claim: &LocalAgentRunClaim) -> bool {
    claim.run.profile_key == "main_chat"
        && claim
            .run
            .input
            .pointer("/explicit_capability_selection/kind")
            .and_then(Value::as_str)
            == Some("plugin")
        && !claim
            .run
            .continuation_input
            .as_ref()
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

fn selected_plugin_task_handoff_required_message() -> Value {
    message_item(
        "system",
        Value::String(
            "[Selected Plugin Task Handoff Required]\nThe previous response cannot finish this Main Chat turn because the user explicitly selected a Plugin and no durable Task handoff has been accepted. Call `create_task` now for the user's requested deliverable; the Host will bind the exact selected Plugin. Do not answer the request in Main Chat and do not ask clarification questions here. Enable Ask User on the Task when task-local blocking details may be needed, so the Task can ask there. After arranging the Task, call `wait_for_task_completion` exactly once."
                .to_string(),
        ),
    )
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
            "[Task Outcome Required]\nThe previous response cannot finish this run because no explicit task outcome was reported. Check the bound Task criteria before calling `task_run_process_report_outcome`. For acceptance_report_version=1 include one acceptance_checks entry per criterion (zero-based index, passed/failed/not_verified, concrete evidence) and the exact final_response. `result_status` is only the internal result and dependency gate: succeeded unlocks dependent Tasks, while failed or blocked prevents them from running. It does not set execution status; the Host owns execution status from the Run lifecycle. Obey final_response_max_chars including Markdown and whitespace. After an accepted report, use no more tools; the Host delivers its checked final_response unchanged."
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
        "response": checkpoint_response_value(response),
        "continuation": continuation
    })
}

const MAXIMUM_DURABLE_REQUEST_INPUT_BYTES: usize = 768 * 1_024;
const MAXIMUM_PERSISTED_INLINE_IMAGE_BYTES: usize = 384 * 1_024;
const MAXIMUM_OLD_TOOL_OUTPUT_CHARS: usize = 4_000;
const MAXIMUM_SKILL_OUTPUT_CHARS: usize = 32_000;
const RECENT_CHECKPOINT_ITEM_COUNT: usize = 32;

#[derive(Default)]
struct CheckpointCompaction {
    original_bytes: usize,
    stored_bytes: usize,
    omitted_inline_images: usize,
    shortened_tool_outputs: usize,
    omitted_history_items: usize,
    omitted_history_sha256: Option<String>,
}

impl CheckpointCompaction {
    fn changed(&self) -> bool {
        self.omitted_inline_images > 0
            || self.shortened_tool_outputs > 0
            || self.omitted_history_items > 0
    }

    fn value(&self) -> Value {
        json!({
            "version": 1,
            "original_request_input_bytes": self.original_bytes,
            "stored_request_input_bytes": self.stored_bytes,
            "omitted_inline_images": self.omitted_inline_images,
            "shortened_tool_outputs": self.shortened_tool_outputs,
            "omitted_history_items": self.omitted_history_items,
            "omitted_history_sha256": self.omitted_history_sha256,
        })
    }
}

fn checkpoint_response_value(response: AiRuntimeResult) -> Value {
    let mut value = response_value(response);
    let request_items = value
        .get_mut("request_input_items")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
        .unwrap_or_default();
    let (request_items, compaction) = bounded_checkpoint_input_items(request_items);
    value["request_input_items"] = Value::Array(request_items);
    if compaction.changed() {
        value["checkpoint_compaction"] = compaction.value();
    }
    value
}

fn bounded_checkpoint_input_items(items: Vec<Value>) -> (Vec<Value>, CheckpointCompaction) {
    let mut items = prune_items_before_latest_compaction(items);
    let mut compaction = CheckpointCompaction {
        original_bytes: encoded_bytes(&items),
        ..CheckpointCompaction::default()
    };
    omit_stale_inline_images(&mut items, &mut compaction);
    if encoded_bytes(&items) > MAXIMUM_DURABLE_REQUEST_INPUT_BYTES {
        shorten_old_tool_outputs(&mut items, &mut compaction);
    }
    if encoded_bytes(&items) > MAXIMUM_DURABLE_REQUEST_INPUT_BYTES {
        omit_old_tool_exchanges(&mut items, &mut compaction);
    }
    compaction.stored_bytes = encoded_bytes(&items);
    (items, compaction)
}

fn omit_stale_inline_images(items: &mut [Value], compaction: &mut CheckpointCompaction) {
    let images = items
        .iter()
        .enumerate()
        .flat_map(|(item_index, item)| {
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(move |(content_index, content)| {
                    content
                        .get("image_url")
                        .and_then(Value::as_str)
                        .filter(|url| url.starts_with("data:"))
                        .map(|url| (item_index, content_index, url.len()))
                })
        })
        .collect::<Vec<_>>();
    let retained = images
        .last()
        .filter(|(_, _, bytes)| *bytes <= MAXIMUM_PERSISTED_INLINE_IMAGE_BYTES)
        .map(|(item_index, content_index, _)| (*item_index, *content_index));
    for (item_index, content_index, _) in images {
        if retained == Some((item_index, content_index)) {
            continue;
        }
        let Some(content) = items[item_index]
            .get_mut("content")
            .and_then(Value::as_array_mut)
            .and_then(|content| content.get_mut(content_index))
        else {
            continue;
        };
        let Some(url) = content.get("image_url").and_then(Value::as_str) else {
            continue;
        };
        let digest = format!("{:x}", Sha256::digest(url.as_bytes()));
        let encoded_bytes = url.len();
        *content = json!({
            "type": "input_text",
            "text": format!(
                "[Durable visual reference omitted to keep this Task responsive. sha256={digest}; encoded_bytes={encoded_bytes}. Re-observe or re-read the source if exact pixels are still required.]"
            )
        });
        compaction.omitted_inline_images += 1;
    }
}

fn shorten_old_tool_outputs(items: &mut [Value], compaction: &mut CheckpointCompaction) {
    let tool_names = items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .filter_map(|item| {
            item.get("call_id")
                .and_then(Value::as_str)
                .zip(item.get("name").and_then(Value::as_str))
        })
        .map(|(call_id, name)| (call_id.to_string(), name.to_string()))
        .collect::<HashMap<_, _>>();
    let recent_start = items.len().saturating_sub(RECENT_CHECKPOINT_ITEM_COUNT);
    for (index, item) in items.iter_mut().enumerate() {
        if item.get("type").and_then(Value::as_str) != Some("function_call_output") {
            continue;
        }
        let tool_name = item
            .get("call_id")
            .and_then(Value::as_str)
            .and_then(|call_id| tool_names.get(call_id))
            .map(String::as_str)
            .unwrap_or("unknown");
        let is_skill_context = matches!(
            tool_name,
            "agent_skill_activate" | "agent_skill_read_resource" | "capability_describe"
        );
        let limit = if is_skill_context {
            MAXIMUM_SKILL_OUTPUT_CHARS
        } else if index >= recent_start {
            MAXIMUM_SKILL_OUTPUT_CHARS
        } else {
            MAXIMUM_OLD_TOOL_OUTPUT_CHARS
        };
        let Some(output) = item.get("output").and_then(Value::as_str) else {
            continue;
        };
        let Some(shortened) = shortened_text(output, limit, tool_name) else {
            continue;
        };
        item["output"] = Value::String(shortened);
        compaction.shortened_tool_outputs += 1;
    }
}

fn shortened_text(text: &str, maximum_chars: usize, source: &str) -> Option<String> {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= maximum_chars {
        return None;
    }
    let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
    let head = maximum_chars.saturating_mul(3) / 4;
    let tail = maximum_chars.saturating_sub(head);
    Some(format!(
        "{}\n\n[Older {source} output shortened in durable context; original_chars={}; sha256={digest}]\n\n{}",
        chars[..head].iter().collect::<String>(),
        chars.len(),
        chars[chars.len() - tail..].iter().collect::<String>()
    ))
}

fn omit_old_tool_exchanges(items: &mut Vec<Value>, compaction: &mut CheckpointCompaction) {
    let recent_start = items.len().saturating_sub(RECENT_CHECKPOINT_ITEM_COUNT);
    let protected_calls = items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .filter(|item| {
            matches!(
                item.get("name").and_then(Value::as_str),
                Some("agent_skill_activate" | "agent_skill_read_resource" | "capability_describe")
            )
        })
        .filter_map(|item| item.get("call_id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let source = std::mem::take(items);
    let mut candidates = Vec::<(usize, Vec<usize>)>::new();
    let mut grouped_call_ids = HashSet::new();
    for (index, item) in source.iter().enumerate().take(recent_start) {
        match item.get("type").and_then(Value::as_str) {
            Some("function_call" | "function_call_output") => {
                let Some(call_id) = item.get("call_id").and_then(Value::as_str) else {
                    continue;
                };
                if protected_calls.contains(call_id) || !grouped_call_ids.insert(call_id) {
                    continue;
                }
                let indices = source
                    .iter()
                    .enumerate()
                    .take(recent_start)
                    .filter_map(|(candidate_index, candidate)| {
                        (candidate.get("call_id").and_then(Value::as_str) == Some(call_id))
                            .then_some(candidate_index)
                    })
                    .collect::<Vec<_>>();
                candidates.push((index, indices));
            }
            Some("reasoning") => candidates.push((index, vec![index])),
            _ => {}
        }
    }
    candidates.sort_by_key(|(index, _)| *index);
    let mut omitted_indices = HashSet::new();
    let mut retained_bytes = encoded_bytes(&source);
    for (_, indices) in candidates {
        if retained_bytes <= MAXIMUM_DURABLE_REQUEST_INPUT_BYTES.saturating_sub(2_048) {
            break;
        }
        retained_bytes = retained_bytes.saturating_sub(
            indices
                .iter()
                .filter(|&&index| !omitted_indices.contains(&index))
                .map(|&index| encoded_bytes(&source[index]).saturating_add(1))
                .sum::<usize>(),
        );
        omitted_indices.extend(indices);
    }
    let mut omitted = Vec::new();
    let mut retained = Vec::with_capacity(source.len());
    for (index, item) in source.into_iter().enumerate() {
        if omitted_indices.contains(&index) {
            omitted.push(item);
        } else {
            retained.push(item);
        }
    }
    if !omitted.is_empty() {
        let encoded = serde_json::to_vec(&omitted).unwrap_or_default();
        let digest = format!("{:x}", Sha256::digest(&encoded));
        compaction.omitted_history_items += omitted.len();
        compaction.omitted_history_sha256 = Some(digest.clone());
        let marker = message_item(
            "system",
            Value::String(format!(
                "[Durable Context Compacted] {} older reasoning/tool exchange items were omitted after their effects were applied. sha256={digest}. Current Task contract, active Skill context, recent tool exchanges, and pending call IDs remain authoritative. Re-read current sources or re-observe external state before relying on omitted details.",
                omitted.len()
            )),
        );
        let insertion = retained
            .iter()
            .position(|item| item.get("role").and_then(Value::as_str) == Some("user"))
            .unwrap_or(0);
        retained.insert(insertion, marker);
    }
    *items = retained;
}

fn encoded_bytes<T: serde::Serialize + ?Sized>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |encoded| encoded.len())
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
    fn durable_checkpoint_bounds_historical_images_and_tool_outputs() {
        let mut result = response(None);
        let mut items = vec![message_item(
            "user",
            Value::String("Implement the requested client optimization".into()),
        )];
        items.push(json!({
            "type": "function_call", "call_id": "skill-1",
            "name": "agent_skill_activate", "arguments": "{}"
        }));
        items.push(json!({
            "type": "function_call_output", "call_id": "skill-1",
            "output": String::from("S").repeat(40_000)
        }));
        for index in 0..80 {
            items.push(json!({
                "type": "function_call", "call_id": format!("call-{index}"),
                "name": "capability_invoke", "arguments": "{}"
            }));
            items.push(json!({
                "type": "function_call_output", "call_id": format!("call-{index}"),
                "output": String::from("T").repeat(20_000)
            }));
            if index % 16 == 0 {
                items.push(json!({
                    "type": "message", "role": "user",
                    "content": [{
                        "type": "input_image",
                        "image_url": format!("data:image/png;base64,{}", String::from("A").repeat(300_000))
                    }]
                }));
            }
        }
        result.request_input_items = items;
        result.response_output_items = vec![json!({
            "type": "function_call", "call_id": "current-call",
            "name": "read_file", "arguments": "{}"
        })];

        let stored = checkpoint_response_value(result);
        let request_items = stored["request_input_items"]
            .as_array()
            .expect("bounded request items");
        assert!(encoded_bytes(request_items) <= MAXIMUM_DURABLE_REQUEST_INPUT_BYTES);
        let inline_images = request_items
            .iter()
            .flat_map(|item| {
                item.get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter(|content| {
                content
                    .get("image_url")
                    .and_then(Value::as_str)
                    .is_some_and(|url| url.starts_with("data:"))
            })
            .count();
        assert_eq!(inline_images, 1);
        assert!(stored["checkpoint_compaction"]["omitted_inline_images"]
            .as_u64()
            .is_some_and(|count| count >= 4));
        assert!(stored["checkpoint_compaction"]["shortened_tool_outputs"]
            .as_u64()
            .is_some_and(|count| count > 0));
        assert!(request_items
            .iter()
            .any(|item| item["call_id"] == "skill-1" && item["type"] == "function_call"));
        assert!(request_items
            .iter()
            .any(|item| item["call_id"] == "skill-1" && item["type"] == "function_call_output"));
        assert_eq!(
            stored["response_output_items"][0]["call_id"],
            "current-call"
        );
    }

    #[test]
    fn durable_checkpoint_keeps_latest_provider_compaction_marker() {
        let marker = json!({
            "type": "compaction", "id": "cmp-1", "encrypted_content": "opaque"
        });
        let (items, _) = bounded_checkpoint_input_items(vec![
            message_item("user", Value::String(String::from("old").repeat(500_000))),
            marker.clone(),
            message_item("user", Value::String("current".into())),
        ]);
        assert_eq!(items.first(), Some(&marker));
        assert_eq!(items.len(), 2);
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
    fn composer_selected_plugin_cannot_finish_before_task_handoff() {
        let mut claim = task_claim();
        claim.run.owner_entity_type = "conversation_turn".to_string();
        claim.run.owner_entity_id = "turn-1".to_string();
        claim.run.profile_key = "main_chat".to_string();
        claim.run.input = json!({
            "message": "帮我设计咖啡店活动网页首屏",
            "explicit_capability_selection": {
                "kind": "plugin",
                "plugin_key": "web-design-plugin",
                "component_key": "web-design-mcp",
                "display_name": "Web Design Studio"
            }
        });

        let outcome = reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(response(None)),
            &claim,
            10_000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("reduce selected Plugin final");

        let LocalAgentStepOutcome::Continue { checkpoint } = outcome else {
            panic!("selected Plugin must dispatch a Task before Main Chat can finish")
        };
        assert_eq!(
            checkpoint["continuation"]["reason"],
            "selected_plugin_task_handoff_required"
        );
        let guidance = checkpoint["continuation"]["input_items"][0]["content"]
            .as_str()
            .expect("corrective guidance");
        assert!(guidance.contains("create_task"));
        assert!(guidance.contains("Ask User"));
        assert!(!guidance.contains("Requirement Survey"));

        claim.run.continuation_input = Some(json!({
            "invocations": [{
                "tool_name": "wait_for_task_completion",
                "status": "succeeded",
                "result": {"accepted": true}
            }]
        }));
        assert!(matches!(
            reduce_ai_step_outcome_for_claim(
                AiSingleStepOutcome::Final(response(None)),
                &claim,
                10_000,
                &ConservativeToolSafetyPolicy,
            )
            .expect("reduce handed-off Plugin final"),
            LocalAgentStepOutcome::Succeed { .. }
        ));
    }

    #[test]
    fn pending_followup_review_cannot_finish_before_resolution() {
        let mut claim = task_claim();
        claim.run.owner_entity_type = "conversation_turn".to_string();
        claim.run.owner_entity_id = "turn-1".to_string();
        claim.run.profile_key = "main_chat".to_string();
        claim.run.input = json!({
            "message": "Make the running work match this correction",
            "task_followup_review": {
                "type": "task_followup_review",
                "review_message_id": "message-followup",
                "pending": true,
                "tasks": [{
                    "task_id": "task-1",
                    "review_state": "pending"
                }]
            }
        });

        let outcome = reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(response(None)),
            &claim,
            10_000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("reduce pending follow-up final");

        let LocalAgentStepOutcome::Continue { checkpoint } = outcome else {
            panic!("pending follow-up review must continue until it is resolved")
        };
        assert_eq!(
            checkpoint["continuation"]["reason"],
            "task_followup_review_required"
        );
        let guidance = checkpoint["continuation"]["input_items"][0]["content"]
            .as_str()
            .expect("corrective guidance");
        assert!(guidance.contains("resolve_task_followup"));
        assert!(guidance.contains("every pending candidate"));

        claim.run.input["task_followup_review"]["pending"] = json!(false);
        assert!(matches!(
            reduce_ai_step_outcome_for_claim(
                AiSingleStepOutcome::Final(response(None)),
                &claim,
                10_000,
                &ConservativeToolSafetyPolicy,
            )
            .expect("reduce resolved follow-up final"),
            LocalAgentStepOutcome::Succeed { .. }
        ));
    }

    #[test]
    fn held_replacement_cannot_finish_before_corrected_graph_creation() {
        let mut claim = task_claim();
        claim.run.owner_entity_type = "conversation_turn".to_string();
        claim.run.owner_entity_id = "turn-1".to_string();
        claim.run.profile_key = "main_chat".to_string();
        claim.run.input = json!({
            "message": "Make the result match the corrected acceptance criteria",
            "task_followup_review": {
                "type": "task_followup_review",
                "review_message_id": "message-followup",
                "pending": false,
                "replacement_pending": true,
                "tasks": [{
                    "task_id": "task-1",
                    "review_state": "held",
                    "decision_action": "replace"
                }]
            }
        });

        let outcome = reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(response(None)),
            &claim,
            10_000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("reduce held replacement final");
        let LocalAgentStepOutcome::Continue { checkpoint } = outcome else {
            panic!("held replacement must continue until corrected work is created")
        };
        assert_eq!(
            checkpoint["continuation"]["reason"],
            "task_replacement_required"
        );
        let guidance = checkpoint["continuation"]["input_items"][0]["content"]
            .as_str()
            .unwrap();
        assert!(guidance.contains("supersedes_task_ids"));
    }

    fn task_claim() -> LocalAgentRunClaim {
        LocalAgentRunClaim {
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
        }
    }

    #[test]
    fn task_final_without_reported_outcome_continues_with_required_guidance() {
        let claim = task_claim();

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

    #[test]
    fn structured_final_delivers_checked_text_not_a_later_model_rewrite() {
        let mut claim = task_claim();
        claim.run.input = json!({"acceptance_report_version":1,"acceptance_criteria":["Evidence-based review"],"final_response_max_chars":100});
        let report = json!({"reported":true,"status":"succeeded","reason":"Review completed; artifact still needs repairs","final_response":"Review complete: the artifact fails its content requirements.","acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Compared the artifact against the requested criteria"}]});
        claim.run.continuation_input = Some(
            json!({"invocations":[{"tool_name":"task_run_process_report_outcome","status":"succeeded","result":report}]}),
        );
        let mut rewritten = response(None);
        rewritten.content = "x".repeat(500);
        let outcome = reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(rewritten),
            &claim,
            10000,
            &ConservativeToolSafetyPolicy,
        )
        .unwrap();
        let LocalAgentStepOutcome::Succeed { output } = outcome else {
            panic!("expected completed review")
        };
        assert_eq!(output["content"], report["final_response"]);
        assert_eq!(output["task_acceptance"]["status"], "succeeded");
        claim.run.continuation_input = None;
        claim.run.checkpoint = json!({
            "response": {
                "content": report["final_response"],
                "task_acceptance": report,
                "request_input_items": [],
                "response_output_items": []
            },
            "continuation": {
                "reason": "trusted_plugin_hook_stop_block",
                "input_items": [],
                "stop_hook_block": {
                    "count": 1,
                    "reason_sha256": "a".repeat(64),
                    "event": "SubagentStop"
                }
            }
        });
        let LocalAgentStepOutcome::Succeed { output } = reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(response(None)),
            &claim,
            10_000,
            &ConservativeToolSafetyPolicy,
        )
        .expect("retained acceptance must survive SubagentStop") else {
            panic!("retained acceptance should allow final completion")
        };
        assert_eq!(output["content"], report["final_response"]);
        // A review can finish normally while finding failed or unverified
        // criteria. Preserve those findings; do not turn them into execution
        // errors or allow the model to replace the accepted final text.
        for verdict in ["failed", "not_verified"] {
            claim.run.checkpoint["response"]["task_acceptance"]["acceptance_checks"][0]["status"] =
                json!(verdict);
            let LocalAgentStepOutcome::Succeed { output } = reduce_ai_step_outcome_for_claim(
                AiSingleStepOutcome::Final(response(None)),
                &claim,
                10000,
                &ConservativeToolSafetyPolicy,
            )
            .expect("valid review finding must not become an execution failure") else {
                panic!("expected completed review with independent acceptance finding")
            };
            assert_eq!(output["content"], report["final_response"]);
            assert_eq!(
                output["task_acceptance"]["acceptance_checks"][0]["status"],
                verdict
            );
        }
        // Malformed reports still fail validation rather than bypassing it.
        claim.run.checkpoint["response"]["task_acceptance"]["acceptance_checks"][0]["status"] =
            json!("unknown-verdict");
        let error = reduce_ai_step_outcome_for_claim(
            AiSingleStepOutcome::Final(response(None)),
            &claim,
            10000,
            &ConservativeToolSafetyPolicy,
        )
        .unwrap_err();
        assert!(error.contains("acceptance check status must be passed, failed, or not_verified"));
    }
}
