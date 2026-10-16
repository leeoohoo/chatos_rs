// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::LocalToolExecutor;
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    validate_task_acceptance_report, LocalAgentRunRecord, LocalAgentToolInvocationRecord,
    LocalAgentToolOutcome,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub const TASK_PROCESS_RECORD_TOOL: &str = "task_run_process_record_process";
pub const TASK_OUTCOME_REPORT_TOOL: &str = "task_run_process_report_outcome";
pub const TASK_PROCESS_TOOL_NAMES: [&str; 2] = [TASK_PROCESS_RECORD_TOOL, TASK_OUTCOME_REPORT_TOOL];

const PROCESS_ENTRY_MAX_CHARS: usize = 16_000;
const OUTCOME_REASON_MAX_CHARS: usize = 2_000;

pub fn task_process_model_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": TASK_PROCESS_RECORD_TOOL,
            "description": "Record a concise, user-visible Task execution milestone. Update it at meaningful phase changes; do not log every low-level tool call and never include secrets or hidden reasoning.",
            "parameters": {
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["append", "replace", "clear"],
                        "default": "append"
                    },
                    "heading": {"type": "string", "maxLength": 240},
                    "content": {"type": "string", "maxLength": PROCESS_ENTRY_MAX_CHARS}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": TASK_OUTCOME_REPORT_TOOL,
            "description": "Report the Task outcome after checking its own bound acceptance criteria. Version 1 Tasks require acceptance_checks and final_response. Rejected reports may be corrected; exactly one accepted report is allowed. This must be the final tool call. The Host delivers the checked final_response unchanged.",
            "parameters": {
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["succeeded", "failed", "blocked"]
                    },
                    "final_response": {"type": "string", "minLength": 1, "maxLength": 32000, "description": "Exact user-facing final delivery, including all Markdown and whitespace; obey the frozen final_response_max_chars if present."},
                    "acceptance_checks": {
                        "type": "array", "maxItems": 50,
                        "items": {
                            "type": "object",
                            "properties": {
                                "criterion_index": {"type":"integer", "minimum":0},
                                "status": {"type":"string", "enum":["passed","failed","not_verified"]},
                                "evidence": {"type":"string", "minLength":1, "maxLength":2000}
                            },
                            "required":["criterion_index","status","evidence"],
                            "additionalProperties":false
                        }
                    },
                    "reason": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": OUTCOME_REASON_MAX_CHARS
                    }
                },
                "required": ["status", "reason"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn task_process_prompt_item() -> Value {
    json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": format!(
                "[Task Execution Process]\nThe local run-scoped tools `{}` and `{}` are available for this Task. Keep the user-visible execution process updated at meaningful milestones: task start, approach or root cause, completion of a major phase or artifact, important verification results, changed path after failure, blockers, and next step. Do not record every tool call, file read, search, or edit. Keep entries concise and never record hidden reasoning, credentials, secrets, raw dumps, or unrelated drafts. After all implementation and verification work is finished, call `{}` exactly once with succeeded, failed, or blocked and a concrete reason. That outcome report must be the final tool call immediately before the user-facing final response.",
                TASK_PROCESS_RECORD_TOOL,
                TASK_OUTCOME_REPORT_TOOL,
                TASK_OUTCOME_REPORT_TOOL
            )
        }]
    })
}

#[derive(Clone)]
pub struct LocalTaskProcessToolExecutor {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
}

impl LocalTaskProcessToolExecutor {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into().trim().to_string();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("Task process tool owner must be 1..=256 characters".to_string());
        }
        Ok(Self {
            runtime,
            owner_user_id,
        })
    }

    async fn validate_run(&self, run_id: &str) -> Result<LocalAgentRunRecord, String> {
        let run = self
            .runtime
            .get_run_for_host_worker(run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Task Run not found: {run_id}"))?;
        if run.owner_user_id != self.owner_user_id
            || run.profile_key != "task_execution"
            || run.owner_entity_type != "task"
        {
            return Err(
                "Task process tools are available only to the active local Task execution"
                    .to_string(),
            );
        }
        Ok(run)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProcessOperation {
    Append,
    Replace,
    Clear,
}

fn default_process_operation() -> ProcessOperation {
    ProcessOperation::Append
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordProcessArgs {
    #[serde(default = "default_process_operation")]
    operation: ProcessOperation,
    #[serde(default)]
    heading: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReportedOutcomeStatus {
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportOutcomeArgs {
    status: ReportedOutcomeStatus,
    reason: String,
    #[serde(default)]
    final_response: Option<String>,
    #[serde(default)]
    acceptance_checks: Option<Vec<AcceptanceCheck>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptanceCheck {
    criterion_index: u64,
    status: String,
    evidence: String,
}

#[async_trait]
impl LocalToolExecutor for LocalTaskProcessToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        let run = self.validate_run(&invocation.run_id).await?;
        let output = match invocation.tool_name.as_str() {
            TASK_PROCESS_RECORD_TOOL => {
                let args: RecordProcessArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid Task process input: {error}"))?;
                let operation = match args.operation {
                    ProcessOperation::Append => "append",
                    ProcessOperation::Replace => "replace",
                    ProcessOperation::Clear => "clear",
                };
                let heading = normalized(args.heading, 240, "heading")?;
                let content = normalized(args.content, PROCESS_ENTRY_MAX_CHARS, "content")?;
                if operation != "clear" && content.is_none() {
                    return Err(
                        "Task process content is required unless operation is clear".to_string()
                    );
                }
                json!({
                    "recorded": true,
                    "operation": operation,
                    "heading": heading,
                    "content_chars": content.as_deref().map(str::chars).map(Iterator::count).unwrap_or(0)
                })
            }
            TASK_OUTCOME_REPORT_TOOL => {
                if self
                    .runtime
                    .successful_tool_invocation_count_for_host_worker(
                        &invocation.run_id,
                        TASK_OUTCOME_REPORT_TOOL,
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    > 0
                {
                    return Err("Task outcome was already reported for this Run".to_string());
                }
                let args: ReportOutcomeArgs = serde_json::from_value(invocation.arguments.clone())
                    .map_err(|error| format!("invalid Task outcome input: {error}"))?;
                if matches!(args.status, ReportedOutcomeStatus::Succeeded)
                    && task_has_selected_plugins(&run.input)
                    && self
                        .runtime
                        .successful_tool_invocation_count_for_host_worker(
                            &invocation.run_id,
                            "capability_invoke",
                        )
                        .await
                        .map_err(|error| error.to_string())?
                        == 0
                {
                    return Err(
                        "selected_plugin_delivery_required: this Task has a frozen selected Plugin but the current Run has no successful Plugin invocation. Discover, describe, activate, and invoke the selected Plugin, persist and verify the requested deliverable there, then report the outcome. Notepad or a model response is not a substitute for the selected Plugin delivery."
                            .to_string(),
                    );
                }
                validate_task_acceptance_report(&run.input, &invocation.arguments)?;
                let reason = args.reason.trim();
                if reason.is_empty() || reason.chars().count() > OUTCOME_REASON_MAX_CHARS {
                    return Err(format!(
                        "Task outcome reason must be 1..={OUTCOME_REASON_MAX_CHARS} characters"
                    ));
                }
                let status = match args.status {
                    ReportedOutcomeStatus::Succeeded => "succeeded",
                    ReportedOutcomeStatus::Failed => "failed",
                    ReportedOutcomeStatus::Blocked => "blocked",
                };
                let checks = args.acceptance_checks.map(|checks| checks.into_iter().map(|check| json!({
                    "criterion_index": check.criterion_index, "status": check.status, "evidence": check.evidence
                })).collect::<Vec<_>>());
                json!({
                    "reported": true,
                    "status": status,
                    "reason": reason,
                    "acceptance_checks": checks,
                    "final_response_chars": args.final_response.as_ref().map(|text| text.chars().count()),
                    "final_response": args.final_response,
                    "acceptance_report_version": run.input.get("acceptance_report_version")
                })
            }
            tool_name => return Err(format!("unsupported Task process tool: {tool_name}")),
        };
        Ok(LocalAgentToolOutcome::Succeeded { output })
    }
}

fn task_has_selected_plugins(input: &Value) -> bool {
    input
        .pointer("/tool_options/plugin_hints")
        .and_then(Value::as_array)
        .is_some_and(|plugins| !plugins.is_empty())
}

fn normalized(
    value: Option<String>,
    maximum_chars: usize,
    field: &str,
) -> Result<Option<String>, String> {
    let value = value.map(|value| value.trim().to_string());
    let value = value.filter(|value| !value.is_empty());
    if value
        .as_deref()
        .is_some_and(|value| value.chars().count() > maximum_chars)
    {
        return Err(format!(
            "Task process {field} cannot exceed {maximum_chars} characters"
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CreateRunCommand, HostCommand, HostRequestEnvelope, LocalAgentToolApprovalStatus,
        LocalAgentToolStatus, LOCAL_AGENT_PROTOCOL_VERSION,
    };

    #[test]
    fn definitions_preserve_tool_names_and_expose_structured_acceptance() {
        let tools = task_process_model_tools();
        assert_eq!(tools[0]["name"], TASK_PROCESS_RECORD_TOOL);
        assert_eq!(tools[1]["name"], TASK_OUTCOME_REPORT_TOOL);
        assert_eq!(
            tools[1]["parameters"]["properties"]["acceptance_checks"]["maxItems"],
            50
        );
        assert_eq!(
            tools[1]["parameters"]["properties"]["final_response"]["maxLength"],
            32000
        );
        let prompt = task_process_prompt_item().to_string();
        assert!(prompt.contains("meaningful milestones"));
        assert!(prompt.contains("final tool call"));
    }

    async fn runtime_with_run(
        profile_key: &str,
        owner_entity_type: &str,
    ) -> Arc<LocalAgentRuntime> {
        runtime_with_input(profile_key, owner_entity_type, json!({"prompt":"do work"})).await
    }

    async fn runtime_with_input(
        profile_key: &str,
        owner_entity_type: &str,
        input: Value,
    ) -> Arc<LocalAgentRuntime> {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = Arc::new(LocalAgentRuntime::new(storage));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: "create-run".to_string(),
                command: HostCommand::CreateRun(CreateRunCommand {
                    run_id: "run-1".to_string(),
                    owner_user_id: "user-1".to_string(),
                    owner_entity_type: owner_entity_type.to_string(),
                    owner_entity_id: "task-1".to_string(),
                    profile_key: profile_key.to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input,
                    max_iterations: 8,
                }),
            })
            .await
            .expect("create run");
        runtime
    }

    fn invocation(tool_name: &str, arguments: Value) -> LocalAgentToolInvocationRecord {
        LocalAgentToolInvocationRecord {
            invocation_id: "invocation-1".to_string(),
            run_id: "run-1".to_string(),
            batch_id: "batch-1".to_string(),
            call_id: "call-1".to_string(),
            tool_name: tool_name.to_string(),
            execution_route: chatos_local_agent_protocol::LocalAgentToolExecutionRoute::Shared,
            arguments,
            side_effecting: false,
            requires_approval: false,
            approval_status: LocalAgentToolApprovalStatus::NotRequired,
            approval_decided_by: None,
            approval_reason: None,
            approval_decided_at_unix_ms: None,
            status: LocalAgentToolStatus::Running,
            result: None,
            error: None,
            version: 2,
            claim_token: Some("claim-1".to_string()),
            claim_until_unix_ms: Some(i64::MAX),
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        }
    }

    #[tokio::test]
    async fn process_tools_execute_only_for_task_owned_task_execution_runs() {
        let executor = LocalTaskProcessToolExecutor::new(
            runtime_with_run("task_execution", "task").await,
            "user-1",
        )
        .expect("executor");
        let outcome = executor
            .execute_tool(&invocation(
                TASK_PROCESS_RECORD_TOOL,
                json!({"operation": "append", "heading": "Verify", "content": "Tests pass"}),
            ))
            .await
            .expect("record process");
        let LocalAgentToolOutcome::Succeeded { output } = outcome else {
            panic!("expected successful process record")
        };
        assert_eq!(output["recorded"], true);

        let wrong_scope = LocalTaskProcessToolExecutor::new(
            runtime_with_run("main_chat", "conversation_turn").await,
            "user-1",
        )
        .expect("executor");
        let error = wrong_scope
            .execute_tool(&invocation(
                TASK_OUTCOME_REPORT_TOOL,
                json!({"status": "succeeded", "reason": "done"}),
            ))
            .await
            .expect_err("Main Chat must not use Task process tools");
        assert!(error.contains("active local Task execution"));
    }

    #[tokio::test]
    async fn outcome_executor_rejects_unverified_and_over_limit_then_accepts_corrected_report() {
        let executor = LocalTaskProcessToolExecutor::new(runtime_with_input("task_execution","task",json!({"acceptance_report_version":1,"acceptance_criteria":["Short response"],"final_response_max_chars":4})).await,"user-1").unwrap();
        let mut report = json!({"status":"succeeded","reason":"done","final_response":"中文\n!!","acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Checked actual final response"}]});
        let error = executor
            .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
            .await
            .unwrap_err();
        assert!(error.contains("5 characters"));
        report["final_response"] = json!("中文");
        report["acceptance_checks"][0]["status"] = json!("not_verified");
        assert!(executor
            .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
            .await
            .is_err());
        report["status"] = json!("blocked");
        let LocalAgentToolOutcome::Succeeded { output } = executor
            .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
            .await
            .unwrap()
        else {
            panic!("accepted blocked report")
        };
        assert_eq!(output["status"], "blocked");
        assert_eq!(output["final_response"], "中文");
        assert_eq!(output["final_response_chars"], 2);
        report["status"] = json!("succeeded");
        report["acceptance_checks"][0]["status"] = json!("passed");
        assert!(executor
            .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn successful_outcome_requires_the_frozen_selected_plugin_to_be_invoked() {
        let executor = LocalTaskProcessToolExecutor::new(
            runtime_with_input(
                "task_execution",
                "task",
                json!({
                    "tool_options": {
                        "requires_execution": true,
                        "enabled_builtin_kinds": ["Notepad"],
                        "plugin_hints": [{"plugin_key":"solution-studio","component_key":"solution-authoring-mcp","reason":"planning"}]
                    }
                }),
            )
            .await,
            "user-1",
        )
        .unwrap();
        let error = executor
            .execute_tool(&invocation(
                TASK_OUTCOME_REPORT_TOOL,
                json!({"status":"succeeded","reason":"saved the plan to Notepad"}),
            ))
            .await
            .expect_err("Notepad must not satisfy a selected Plugin Task");
        assert!(error.contains("selected_plugin_delivery_required"));
        assert!(error.contains("Notepad"));
    }
}
