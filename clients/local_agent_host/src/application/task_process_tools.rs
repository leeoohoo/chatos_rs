// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::LocalToolExecutor;
use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalAgentToolInvocationRecord, LocalAgentToolOutcome};
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
            "description": "Report the Task outcome exactly once after implementation and verification. This must be the final tool call immediately before the user-facing final response.",
            "parameters": {
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["succeeded", "failed", "blocked"]
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

    async fn validate_run(&self, run_id: &str) -> Result<(), String> {
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
        Ok(())
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

#[derive(Debug, Deserialize)]
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
}

#[async_trait]
impl LocalToolExecutor for LocalTaskProcessToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        self.validate_run(&invocation.run_id).await?;
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
                json!({
                    "reported": true,
                    "status": status,
                    "reason": reason
                })
            }
            tool_name => return Err(format!("unsupported Task process tool: {tool_name}")),
        };
        Ok(LocalAgentToolOutcome::Succeeded { output })
    }
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
    fn definitions_restore_the_original_process_and_outcome_contract() {
        let tools = task_process_model_tools();
        assert_eq!(tools[0]["name"], TASK_PROCESS_RECORD_TOOL);
        assert_eq!(tools[1]["name"], TASK_OUTCOME_REPORT_TOOL);
        let prompt = task_process_prompt_item().to_string();
        assert!(prompt.contains("meaningful milestones"));
        assert!(prompt.contains("final tool call"));
    }

    async fn runtime_with_run(
        profile_key: &str,
        owner_entity_type: &str,
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
                    input: json!({"prompt": "do work"}),
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
}
