// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalAgentRun, ModelStepResult, ToolEffect};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileStep, ModelGatewayOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::shared::{parse_tool_arguments, validate_context_strategy};

pub const TASK_RUNNER_PROFILE_KEY: &str = "task_runner";
pub const TASK_RUNNER_ASK_USER_TOOL: &str = "task_runner_ask_user";
pub const TASK_RUNNER_REPORT_OUTCOME_TOOL: &str = "task_runner_report_outcome";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunnerProjectSnapshot {
    pub project_id: String,
    pub snapshot_revision: String,
    pub working_directory_ref: String,
    pub authority_snapshot: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunnerPromptSnapshot {
    pub prompt_revision: String,
    pub task_prompt: String,
    pub skill_snapshot: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunnerExecutionTool {
    pub name: String,
    pub effect: ToolEffect,
    pub schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunnerCapabilitySnapshot {
    pub snapshot_ref: String,
    pub plugin_release_snapshot: Value,
    pub execution_tools: Vec<TaskRunnerExecutionTool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunnerToolReceiptStatus {
    Succeeded,
    Failed,
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunnerToolReceipt {
    pub receipt_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub status: TaskRunnerToolReceiptStatus,
    pub verification: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskRunnerStepContext {
    pub base_system_prompt: String,
    pub project_snapshot: TaskRunnerProjectSnapshot,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub prompt_snapshot: TaskRunnerPromptSnapshot,
    pub capability_snapshot: TaskRunnerCapabilitySnapshot,
    pub model_input_items: Vec<Value>,
    pub tool_receipts: Vec<TaskRunnerToolReceipt>,
    pub maximum_output_tokens: u32,
    pub native_compaction_threshold: Option<u64>,
    pub memory_engine_active_threshold: Option<u64>,
    pub maximum_summary_attempts: u8,
}

#[async_trait]
pub trait TaskRunnerContextProvider: Send + Sync {
    /// Loads only durable, run-scoped state. Implementations must never derive a
    /// project or capability set from mutable UI selection.
    async fn load_step_context(&self, run: &LocalAgentRun)
        -> Result<TaskRunnerStepContext, String>;
}

pub struct TaskRunnerAgentProfile {
    context_provider: Arc<dyn TaskRunnerContextProvider>,
}

impl TaskRunnerAgentProfile {
    pub fn new(context_provider: Arc<dyn TaskRunnerContextProvider>) -> Self {
        Self { context_provider }
    }
}

#[async_trait]
impl LocalAgentProfile for TaskRunnerAgentProfile {
    fn profile_key(&self) -> &'static str {
        TASK_RUNNER_PROFILE_KEY
    }

    async fn prepare_model_step(
        &self,
        run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        let context = self.context_provider.load_step_context(run).await?;
        validate_task_runner_context(run, &context)?;
        let instructions = build_task_runner_instructions(&context)?;
        Ok(LocalAgentProfileStep {
            model_input_items: context.model_input_items,
            tools: effective_tool_schemas(&context.capability_snapshot),
            instructions: Some(instructions),
            maximum_output_tokens: context.maximum_output_tokens,
            reasoning_effort: None,
            temperature: None,
            native_compaction_threshold: context.native_compaction_threshold,
            memory_engine_active_threshold: context.memory_engine_active_threshold,
            maximum_summary_attempts: context.maximum_summary_attempts,
        })
    }

    async fn interpret_completed_output(
        &self,
        run: &LocalAgentRun,
        output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        // Reloading durable context is intentional: final acceptance is based
        // on committed receipts, never process-local state captured earlier.
        let context = self.context_provider.load_step_context(run).await?;
        validate_task_runner_context(run, &context)?;
        interpret_task_runner_output(run, output, &context)
    }
}

fn validate_task_runner_context(
    run: &LocalAgentRun,
    context: &TaskRunnerStepContext,
) -> Result<(), String> {
    let project_id = run
        .project_id
        .as_deref()
        .ok_or_else(|| "Task Runner run requires a frozen project_id".to_string())?;
    if context.project_snapshot.project_id != project_id {
        return Err("project snapshot does not match the frozen run project_id".to_string());
    }
    for (field, value) in [
        (
            "project snapshot revision",
            context.project_snapshot.snapshot_revision.as_str(),
        ),
        (
            "working directory reference",
            context.project_snapshot.working_directory_ref.as_str(),
        ),
        ("task objective", context.objective.as_str()),
        ("base system prompt", context.base_system_prompt.as_str()),
        ("task prompt", context.prompt_snapshot.task_prompt.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{field} must not be empty"));
        }
    }
    if context.prompt_snapshot.prompt_revision != run.prompt_revision {
        return Err("prompt snapshot does not match the frozen run revision".to_string());
    }
    if context.capability_snapshot.snapshot_ref != run.capability_snapshot_ref {
        return Err("capability snapshot does not match the frozen run reference".to_string());
    }
    if context.acceptance_criteria.is_empty()
        || context
            .acceptance_criteria
            .iter()
            .any(|criterion| criterion.trim().is_empty())
    {
        return Err("Task Runner requires non-empty acceptance criteria".to_string());
    }
    let mut criteria = HashMap::new();
    for criterion in &context.acceptance_criteria {
        if criteria.insert(criterion.as_str(), ()).is_some() {
            return Err("Task Runner acceptance criteria must be unique".to_string());
        }
    }
    let tools = validate_execution_tools(&context.capability_snapshot.execution_tools)?;
    validate_receipts(&context.tool_receipts, &tools)?;
    validate_context_strategy(
        run,
        context.native_compaction_threshold,
        context.memory_engine_active_threshold,
        context.maximum_summary_attempts,
    )
}

fn validate_execution_tools(
    tools: &[TaskRunnerExecutionTool],
) -> Result<HashMap<&str, ToolEffect>, String> {
    let mut validated = HashMap::new();
    for tool in tools {
        if tool.name.trim().is_empty() {
            return Err("execution tool name must not be empty".to_string());
        }
        if matches!(
            tool.name.as_str(),
            TASK_RUNNER_ASK_USER_TOOL | TASK_RUNNER_REPORT_OUTCOME_TOOL
        ) {
            return Err(format!(
                "capability snapshot cannot replace reserved tool {}",
                tool.name
            ));
        }
        if tool.schema.get("type").and_then(Value::as_str) != Some("function")
            || tool.schema.get("name").and_then(Value::as_str) != Some(tool.name.as_str())
        {
            return Err(format!(
                "execution tool {} schema must be its matching function schema",
                tool.name
            ));
        }
        let properties = tool
            .schema
            .pointer("/parameters/properties")
            .and_then(Value::as_object);
        if properties.is_some_and(|properties| {
            properties.contains_key("project_id") || properties.contains_key("projectId")
        }) {
            return Err(format!(
                "execution tool {} schema must not expose project_id; scope is runtime-injected",
                tool.name
            ));
        }
        if validated.insert(tool.name.as_str(), tool.effect).is_some() {
            return Err(format!("execution tool {} is duplicated", tool.name));
        }
    }
    Ok(validated)
}

fn validate_receipts(
    receipts: &[TaskRunnerToolReceipt],
    tools: &HashMap<&str, ToolEffect>,
) -> Result<(), String> {
    let mut receipt_ids = HashMap::new();
    let mut call_ids = HashMap::new();
    for receipt in receipts {
        for (field, value) in [
            ("receipt_id", receipt.receipt_id.as_str()),
            ("tool_call_id", receipt.tool_call_id.as_str()),
            ("tool_name", receipt.tool_name.as_str()),
            ("receipt summary", receipt.summary.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{field} must not be empty"));
            }
        }
        if !tools.contains_key(receipt.tool_name.as_str()) {
            return Err(format!(
                "tool receipt {} is outside the frozen capability snapshot",
                receipt.receipt_id
            ));
        }
        if receipt_ids
            .insert(receipt.receipt_id.as_str(), ())
            .is_some()
        {
            return Err(format!("tool receipt {} is duplicated", receipt.receipt_id));
        }
        if call_ids.insert(receipt.tool_call_id.as_str(), ()).is_some() {
            return Err(format!(
                "tool call receipt {} is duplicated",
                receipt.tool_call_id
            ));
        }
    }
    Ok(())
}

fn build_task_runner_instructions(context: &TaskRunnerStepContext) -> Result<String, String> {
    let frozen_context = serde_json::to_string(&json!({
        "project": context.project_snapshot,
        "objective": context.objective,
        "acceptance_criteria": context.acceptance_criteria,
        "prompt_snapshot": context.prompt_snapshot,
        "capability_snapshot_ref": context.capability_snapshot.snapshot_ref,
        "plugin_release_snapshot": context.capability_snapshot.plugin_release_snapshot,
        "committed_tool_receipts": context.tool_receipts,
    }))
    .map_err(|error| format!("failed to serialize Task Runner context: {error}"))?;
    Ok([
        context.base_system_prompt.as_str(),
        "You are the local Task Runner. Work only inside the frozen project and capability snapshot below. The runtime injects project_id into execution scope; never include or choose project_id in tool arguments. Use only the supplied execution tools. Use task_runner_ask_user alone only when a missing user decision materially changes the result. Use task_runner_report_outcome alone only after the acceptance criteria are deterministically supported by committed verification receipts. Plain text is progress, not completion. Never claim success from intended work, uncommitted output, or your own description.",
        context.prompt_snapshot.task_prompt.as_str(),
        frozen_context.as_str(),
    ]
    .join("\n\n"))
}

fn effective_tool_schemas(snapshot: &TaskRunnerCapabilitySnapshot) -> Vec<Value> {
    let mut tools = snapshot
        .execution_tools
        .iter()
        .map(|tool| tool.schema.clone())
        .collect::<Vec<_>>();
    tools.push(ask_user_schema());
    tools.push(report_outcome_schema());
    tools
}

fn ask_user_schema() -> Value {
    json!({
        "type": "function",
        "name": TASK_RUNNER_ASK_USER_TOOL,
        "description": "Ask one blocking question that requires a user decision.",
        "parameters": {
            "type": "object",
            "properties": {"question": {"type": "string"}},
            "required": ["question"],
            "additionalProperties": false
        }
    })
}

fn report_outcome_schema() -> Value {
    json!({
        "type": "function",
        "name": TASK_RUNNER_REPORT_OUTCOME_TOOL,
        "description": "Report a verified terminal task outcome.",
        "parameters": {
            "type": "object",
            "properties": {
                "status": {"type": "string", "enum": ["succeeded", "failed"]},
                "summary": {"type": "string"},
                "failure_reason": {"type": ["string", "null"]},
                "unmet_acceptance_criteria": {"type": "array", "items": {"type": "string"}},
                "acceptance_evidence": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "criterion": {"type": "string"},
                            "receipt_ids": {"type": "array", "items": {"type": "string"}},
                            "description": {"type": "string"}
                        },
                        "required": ["criterion", "receipt_ids", "description"],
                        "additionalProperties": false
                    }
                },
                "referenced_paths": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["status", "summary", "failure_reason", "unmet_acceptance_criteria", "acceptance_evidence", "referenced_paths"],
            "additionalProperties": false
        }
    })
}

fn interpret_task_runner_output(
    run: &LocalAgentRun,
    output: &ModelGatewayOutput,
    context: &TaskRunnerStepContext,
) -> Result<ModelStepResult, String> {
    let calls = output
        .terminal
        .output_items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .collect::<Vec<_>>();
    if calls.is_empty() {
        return Ok(ModelStepResult::Continue(json!({
            "reason": "task_runner_requires_an_explicit_tool_or_outcome",
            "progress_text": output.content,
        })));
    }

    if calls.len() == 1 {
        let call = calls[0];
        let name = call.get("name").and_then(Value::as_str).unwrap_or_default();
        if name == TASK_RUNNER_ASK_USER_TOOL {
            let arguments = parse_tool_arguments(call)?;
            let question = arguments
                .get("question")
                .and_then(Value::as_str)
                .filter(|question| !question.trim().is_empty())
                .ok_or_else(|| "task_runner_ask_user requires a non-empty question".to_string())?;
            return Ok(ModelStepResult::AskUser(json!({
                "question": question,
                "project_id": run.project_id,
            })));
        }
        if name == TASK_RUNNER_REPORT_OUTCOME_TOOL {
            let arguments = parse_tool_arguments(call)?;
            return interpret_reported_outcome(arguments, context);
        }
    }

    if calls.iter().any(|call| {
        matches!(
            call.get("name").and_then(Value::as_str),
            Some(TASK_RUNNER_ASK_USER_TOOL | TASK_RUNNER_REPORT_OUTCOME_TOOL)
        )
    }) {
        return Err(
            "Ask User and outcome reporting must each be the only tool call in a model step"
                .to_string(),
        );
    }

    let tools = validate_execution_tools(&context.capability_snapshot.execution_tools)?;
    let mut call_ids = HashMap::new();
    let mut commands = Vec::with_capacity(calls.len());
    for call in calls {
        let name = call.get("name").and_then(Value::as_str).unwrap_or_default();
        let effect = tools
            .get(name)
            .copied()
            .ok_or_else(|| format!("Task Runner emitted unauthorized tool {name}"))?;
        let call_id = call
            .get("call_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("execution tool {name} is missing call_id"))?;
        if call_ids.insert(call_id, ()).is_some() {
            return Err(format!("execution tool call_id {call_id} is duplicated"));
        }
        let arguments = parse_tool_arguments(call)?;
        if arguments.get("project_id").is_some() || arguments.get("projectId").is_some() {
            return Err(format!(
                "execution tool {name} must not choose project_id; the runtime injects it"
            ));
        }
        commands.push(json!({
            "call_id": call_id,
            "name": name,
            "effect": effect,
            "arguments": arguments,
        }));
    }
    Ok(ModelStepResult::ToolCommand(json!({
        "project_id": run.project_id,
        "capability_snapshot_ref": run.capability_snapshot_ref,
        "calls": commands,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReportedOutcomeStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportedAcceptanceEvidence {
    criterion: String,
    receipt_ids: Vec<String>,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportedOutcome {
    status: ReportedOutcomeStatus,
    summary: String,
    failure_reason: Option<String>,
    unmet_acceptance_criteria: Vec<String>,
    acceptance_evidence: Vec<ReportedAcceptanceEvidence>,
    referenced_paths: Vec<String>,
}

fn interpret_reported_outcome(
    arguments: Value,
    context: &TaskRunnerStepContext,
) -> Result<ModelStepResult, String> {
    let outcome: ReportedOutcome = serde_json::from_value(arguments)
        .map_err(|error| format!("Task Runner outcome is invalid: {error}"))?;
    if outcome.summary.trim().is_empty() {
        return Err("Task Runner outcome summary must not be empty".to_string());
    }
    if outcome
        .referenced_paths
        .iter()
        .any(|path| path.trim().is_empty())
    {
        return Err("Task Runner referenced paths must not be empty".to_string());
    }
    match outcome.status {
        ReportedOutcomeStatus::Failed => {
            let failure_reason = outcome
                .failure_reason
                .filter(|reason| !reason.trim().is_empty())
                .ok_or_else(|| "failed Task Runner outcome requires failure_reason".to_string())?;
            Ok(ModelStepResult::Failed(json!({
                "status": "failed",
                "summary": outcome.summary,
                "failure_reason": failure_reason,
                "unmet_acceptance_criteria": outcome.unmet_acceptance_criteria,
                "referenced_paths": outcome.referenced_paths,
            })))
        }
        ReportedOutcomeStatus::Succeeded => validate_success_outcome(outcome, context),
    }
}

fn validate_success_outcome(
    outcome: ReportedOutcome,
    context: &TaskRunnerStepContext,
) -> Result<ModelStepResult, String> {
    if context
        .tool_receipts
        .iter()
        .any(|receipt| receipt.status == TaskRunnerToolReceiptStatus::OutcomeUnknown)
    {
        let receipts = context
            .tool_receipts
            .iter()
            .filter(|receipt| receipt.status == TaskRunnerToolReceiptStatus::OutcomeUnknown)
            .map(|receipt| receipt.receipt_id.clone())
            .collect::<Vec<_>>();
        return Ok(ModelStepResult::Failed(json!({
            "reason": "tool_outcome_unknown_requires_manual_review",
            "needs_review": true,
            "receipt_ids": receipts,
        })));
    }
    let mut issues = Vec::new();
    if outcome
        .failure_reason
        .as_deref()
        .is_some_and(|reason| !reason.trim().is_empty())
    {
        issues.push("a succeeded outcome cannot include failure_reason".to_string());
    }
    if !outcome.unmet_acceptance_criteria.is_empty() {
        issues.push("a succeeded outcome cannot include unmet acceptance criteria".to_string());
    }
    let receipts = context
        .tool_receipts
        .iter()
        .map(|receipt| (receipt.receipt_id.as_str(), receipt))
        .collect::<HashMap<_, _>>();
    let mut evidence_by_criterion = HashMap::new();
    for evidence in &outcome.acceptance_evidence {
        if evidence.description.trim().is_empty() || evidence.receipt_ids.is_empty() {
            issues.push(format!(
                "acceptance evidence for {} is empty",
                evidence.criterion
            ));
            continue;
        }
        if evidence_by_criterion
            .insert(evidence.criterion.as_str(), ())
            .is_some()
        {
            issues.push(format!(
                "acceptance evidence for {} is duplicated",
                evidence.criterion
            ));
        }
        for receipt_id in &evidence.receipt_ids {
            match receipts.get(receipt_id.as_str()) {
                Some(receipt)
                    if receipt.status == TaskRunnerToolReceiptStatus::Succeeded
                        && receipt.verification => {}
                Some(_) => issues.push(format!(
                    "receipt {receipt_id} is not a successful verification receipt"
                )),
                None => issues.push(format!(
                    "receipt {receipt_id} is not committed for this Run"
                )),
            }
        }
    }
    for criterion in &context.acceptance_criteria {
        if !evidence_by_criterion.contains_key(criterion.as_str()) {
            issues.push(format!("acceptance criterion lacks evidence: {criterion}"));
        }
    }
    for criterion in evidence_by_criterion.keys() {
        if !context
            .acceptance_criteria
            .iter()
            .any(|expected| expected == criterion)
        {
            issues.push(format!(
                "reported evidence is outside the frozen acceptance criteria: {criterion}"
            ));
        }
    }
    if !issues.is_empty() {
        return Ok(ModelStepResult::Continue(json!({
            "reason": "reported_success_rejected",
            "issues": issues,
        })));
    }
    Ok(ModelStepResult::Final(json!({
        "status": "succeeded",
        "summary": outcome.summary,
        "acceptance_evidence": outcome.acceptance_evidence.iter().map(|evidence| json!({
            "criterion": evidence.criterion,
            "receipt_ids": evidence.receipt_ids,
            "description": evidence.description,
        })).collect::<Vec<_>>(),
        "referenced_paths": outcome.referenced_paths,
    })))
}

#[cfg(test)]
mod tests {
    use chatos_local_agent_protocol::{
        ContextStrategy, LocalAgentRunStatus, ModelGatewayTerminal, ModelGatewayTerminalSource,
        ModelGatewayTerminalStatus, ModelProtocol, ModelRuntimeDescriptor,
    };
    use chrono::Utc;

    use super::*;

    struct StaticContextProvider(TaskRunnerStepContext);

    #[async_trait]
    impl TaskRunnerContextProvider for StaticContextProvider {
        async fn load_step_context(
            &self,
            _run: &LocalAgentRun,
        ) -> Result<TaskRunnerStepContext, String> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn prepared_step_uses_only_frozen_tools_and_profile_control_tools() {
        let profile = TaskRunnerAgentProfile::new(Arc::new(StaticContextProvider(context())));
        let step = profile.prepare_model_step(&run()).await.unwrap();
        let names = step
            .tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "read_file",
                TASK_RUNNER_ASK_USER_TOOL,
                TASK_RUNNER_REPORT_OUTCOME_TOOL,
            ]
        );
        assert!(step
            .instructions
            .as_deref()
            .unwrap()
            .contains("\"project_id\":\"project-1\""));
    }

    #[test]
    fn project_scope_is_injected_and_cannot_be_selected_by_the_model() {
        let result = interpret_task_runner_output(
            &run(),
            &output(vec![call(
                "call-1",
                "read_file",
                json!({"path": "src/lib.rs"}),
            )]),
            &context(),
        )
        .unwrap();
        let ModelStepResult::ToolCommand(command) = result else {
            panic!("expected tool command");
        };
        assert_eq!(command["project_id"], "project-1");
        assert_eq!(command["calls"][0]["arguments"]["path"], "src/lib.rs");

        let error = interpret_task_runner_output(
            &run(),
            &output(vec![call(
                "call-2",
                "read_file",
                json!({"path": "src/lib.rs", "project_id": "project-2"}),
            )]),
            &context(),
        )
        .unwrap_err();
        assert!(error.contains("runtime injects"));
    }

    #[test]
    fn capability_snapshot_rejects_unknown_tools() {
        let error = interpret_task_runner_output(
            &run(),
            &output(vec![call("call-1", "shell", json!({}))]),
            &context(),
        )
        .unwrap_err();
        assert!(error.contains("unauthorized tool"));
    }

    #[test]
    fn ask_user_cannot_be_mixed_with_execution() {
        let error = interpret_task_runner_output(
            &run(),
            &output(vec![
                call(
                    "call-1",
                    TASK_RUNNER_ASK_USER_TOOL,
                    json!({"question": "Which?"}),
                ),
                call("call-2", "read_file", json!({"path": "src/lib.rs"})),
            ]),
            &context(),
        )
        .unwrap_err();
        assert!(error.contains("only tool call"));
    }

    #[test]
    fn success_requires_committed_verification_for_every_criterion() {
        let valid = json!({
            "status": "succeeded",
            "summary": "Implemented and verified",
            "failure_reason": null,
            "unmet_acceptance_criteria": [],
            "acceptance_evidence": [{
                "criterion": "tests pass",
                "receipt_ids": ["receipt-1"],
                "description": "cargo test passed"
            }],
            "referenced_paths": ["src/lib.rs"]
        });
        let result = interpret_task_runner_output(
            &run(),
            &output(vec![call(
                "outcome-1",
                TASK_RUNNER_REPORT_OUTCOME_TOOL,
                valid,
            )]),
            &context(),
        )
        .unwrap();
        assert!(matches!(result, ModelStepResult::Final(_)));

        let missing = json!({
            "status": "succeeded",
            "summary": "Done",
            "failure_reason": null,
            "unmet_acceptance_criteria": [],
            "acceptance_evidence": [],
            "referenced_paths": []
        });
        let result = interpret_task_runner_output(
            &run(),
            &output(vec![call(
                "outcome-2",
                TASK_RUNNER_REPORT_OUTCOME_TOOL,
                missing,
            )]),
            &context(),
        )
        .unwrap();
        let ModelStepResult::Continue(payload) = result else {
            panic!("unverified success must continue");
        };
        assert_eq!(payload["reason"], "reported_success_rejected");
    }

    #[test]
    fn unknown_side_effect_outcome_requires_manual_review() {
        let mut context = context();
        context.tool_receipts[0].status = TaskRunnerToolReceiptStatus::OutcomeUnknown;
        let reported = json!({
            "status": "succeeded",
            "summary": "Done",
            "failure_reason": null,
            "unmet_acceptance_criteria": [],
            "acceptance_evidence": [{
                "criterion": "tests pass",
                "receipt_ids": ["receipt-1"],
                "description": "claimed evidence"
            }],
            "referenced_paths": []
        });
        let result = interpret_task_runner_output(
            &run(),
            &output(vec![call(
                "outcome-1",
                TASK_RUNNER_REPORT_OUTCOME_TOOL,
                reported,
            )]),
            &context,
        )
        .unwrap();
        let ModelStepResult::Failed(payload) = result else {
            panic!("unknown outcome must fail closed");
        };
        assert_eq!(payload["needs_review"], true);
    }

    #[test]
    fn frozen_project_prompt_and_capability_must_match_the_run() {
        let mut mismatch = context();
        mismatch.project_snapshot.project_id = "project-2".to_string();
        assert!(validate_task_runner_context(&run(), &mismatch)
            .unwrap_err()
            .contains("project snapshot"));

        let mut mismatch = context();
        mismatch.capability_snapshot.snapshot_ref = "capabilities-2".to_string();
        assert!(validate_task_runner_context(&run(), &mismatch)
            .unwrap_err()
            .contains("capability snapshot"));

        let mut mismatch = context();
        mismatch.prompt_snapshot.prompt_revision = "prompt-2".to_string();
        assert!(validate_task_runner_context(&run(), &mismatch)
            .unwrap_err()
            .contains("prompt snapshot"));
    }

    fn context() -> TaskRunnerStepContext {
        TaskRunnerStepContext {
            base_system_prompt: "Perform the task safely.".to_string(),
            project_snapshot: TaskRunnerProjectSnapshot {
                project_id: "project-1".to_string(),
                snapshot_revision: "project-snapshot-1".to_string(),
                working_directory_ref: "workspace-1".to_string(),
                authority_snapshot: json!({"root": "/workspace/project"}),
            },
            objective: "Implement the requested change".to_string(),
            acceptance_criteria: vec!["tests pass".to_string()],
            prompt_snapshot: TaskRunnerPromptSnapshot {
                prompt_revision: "prompt-1".to_string(),
                task_prompt: "Use the repository conventions.".to_string(),
                skill_snapshot: json!({"skills": []}),
            },
            capability_snapshot: TaskRunnerCapabilitySnapshot {
                snapshot_ref: "capabilities-1".to_string(),
                plugin_release_snapshot: json!({"plugins": []}),
                execution_tools: vec![TaskRunnerExecutionTool {
                    name: "read_file".to_string(),
                    effect: ToolEffect::Read,
                    schema: json!({
                        "type": "function",
                        "name": "read_file",
                        "parameters": {
                            "type": "object",
                            "properties": {"path": {"type": "string"}},
                            "required": ["path"],
                            "additionalProperties": false
                        }
                    }),
                }],
            },
            model_input_items: vec![json!({
                "type": "message",
                "role": "user",
                "content": "Continue the task"
            })],
            tool_receipts: vec![TaskRunnerToolReceipt {
                receipt_id: "receipt-1".to_string(),
                tool_call_id: "previous-call-1".to_string(),
                tool_name: "read_file".to_string(),
                status: TaskRunnerToolReceiptStatus::Succeeded,
                verification: true,
                summary: "cargo test passed".to_string(),
            }],
            maximum_output_tokens: 32_000,
            native_compaction_threshold: Some(200_000),
            memory_engine_active_threshold: None,
            maximum_summary_attempts: 0,
        }
    }

    fn run() -> LocalAgentRun {
        let now = Utc::now();
        LocalAgentRun {
            run_id: "run-1".to_string(),
            profile_key: TASK_RUNNER_PROFILE_KEY.to_string(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "task".to_string(),
            owner_entity_id: "task-1".to_string(),
            project_id: Some("project-1".to_string()),
            status: LocalAgentRunStatus::ModelRunning,
            version: 2,
            step_seq: 1,
            iteration: 1,
            retry_count: 0,
            model_config_id: "model-1".to_string(),
            model_config_revision: 7,
            model_runtime_snapshot: ModelRuntimeDescriptor {
                model_config_id: "model-1".to_string(),
                revision: 7,
                provider: "configured-provider".to_string(),
                model: "configured-model".to_string(),
                protocol: ModelProtocol::Responses,
                context_window_tokens: 400_000,
                maximum_output_tokens: 32_000,
                context_strategy: ContextStrategy::ProviderNative,
                supports_streaming: true,
                supports_native_compaction: true,
                supports_input_token_count: true,
            },
            context_strategy: ContextStrategy::ProviderNative,
            prompt_revision: "prompt-1".to_string(),
            capability_snapshot_ref: "capabilities-1".to_string(),
            pending_batch_id: None,
            pending_interaction: None,
            terminal_outcome: None,
            deadline_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn call(call_id: &str, name: &str, arguments: Value) -> Value {
        json!({
            "type": "function_call",
            "call_id": call_id,
            "name": name,
            "arguments": arguments.to_string(),
        })
    }

    fn output(items: Vec<Value>) -> ModelGatewayOutput {
        ModelGatewayOutput {
            content: String::new(),
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
