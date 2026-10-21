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
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    sync::{Arc, Mutex},
};

pub const TASK_PROCESS_RECORD_TOOL: &str = "task_run_process_record_process";
pub const TASK_PROCESS_ALLOCATE_SERVICE_TOOL: &str = "task_run_process_allocate_service";
pub const TASK_OUTCOME_REPORT_TOOL: &str = "task_run_process_report_outcome";
pub const TASK_PROCESS_TOOL_NAMES: [&str; 3] = [
    TASK_PROCESS_ALLOCATE_SERVICE_TOOL,
    TASK_PROCESS_RECORD_TOOL,
    TASK_OUTCOME_REPORT_TOOL,
];

const PROCESS_ENTRY_MAX_CHARS: usize = 16_000;
const OUTCOME_REASON_MAX_CHARS: usize = 2_000;

pub fn task_process_model_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": TASK_PROCESS_ALLOCATE_SERVICE_TOOL,
            "description": "Allocate or release a Run-scoped loopback service address before starting a local server. Allocate returns the actual port and service_url; pass that port to the backend and the returned URL to dependent frontend processes. A busy preferred_port is never killed or taken over and automatically falls back to another port.",
            "parameters": {
                "type": "object",
                "properties": {
                    "operation": {"type": "string", "enum": ["allocate", "release"], "default": "allocate"},
                    "service_name": {"type": "string", "minLength": 1, "maxLength": 64},
                    "preferred_port": {"type": "integer", "minimum": 1024, "maximum": 65535},
                    "scheme": {"type": "string", "enum": ["http", "https"], "default": "http"}
                },
                "required": ["service_name"],
                "additionalProperties": false
            }
        }),
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
            "description": "Report the Task's internal result and acceptance evidence. result_status controls whether dependent Tasks may run: succeeded unlocks dependencies; failed or blocked prevents them. It never controls the worker/Task execution status, which is owned by the Host and comes only from the Run lifecycle. Version 1 Tasks require acceptance_checks and final_response. Rejected reports may be corrected; exactly one accepted report is allowed. This must be the final tool call. The Host delivers the checked final_response unchanged.",
            "parameters": {
                "type": "object",
                "properties": {
                    "result_status": {
                        "type": "string",
                        "enum": ["succeeded", "failed", "blocked"],
                        "description": "Internal result only. succeeded means downstream prerequisites are satisfied; failed means the requested result did not meet its contract; blocked means an external prerequisite or decision is still missing."
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
                "required": ["result_status", "reason"],
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
                "[Task Execution Process]\nThe local run-scoped tools `{}`, `{}`, and `{}` are available for this Task. Before starting a local HTTP service, allocate its address with `{}`; pass the returned port to the server and service_url to every dependent process, and release the registration after the service stops. Keep the user-visible execution process updated at meaningful milestones: task start, approach or root cause, completion of a major phase or artifact, important verification results, changed path after failure, blockers, and next step. Do not record every tool call, file read, search, or edit. Keep entries concise and never record hidden reasoning, credentials, secrets, raw dumps, or unrelated drafts. After all implementation and verification work is finished, call `{}` exactly once with a concrete reason. `result_status` describes only the internal result and dependency gate: succeeded allows dependent Tasks to run; failed or blocked keeps them from running. It never sets the execution status; the Host derives that from whether this Run finishes normally, fails abnormally, or is cancelled. That outcome report must be the final tool call immediately before the user-facing final response.",
                TASK_PROCESS_RECORD_TOOL,
                TASK_PROCESS_ALLOCATE_SERVICE_TOOL,
                TASK_OUTCOME_REPORT_TOOL,
                TASK_PROCESS_ALLOCATE_SERVICE_TOOL,
                TASK_OUTCOME_REPORT_TOOL
            )
        }]
    })
}

#[derive(Clone)]
pub struct LocalTaskProcessToolExecutor {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
    services: Arc<Mutex<RunServicePortRegistry>>,
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
            services: Arc::new(Mutex::new(RunServicePortRegistry::default())),
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ServiceOperation {
    Allocate,
    Release,
}

fn default_service_operation() -> ServiceOperation {
    ServiceOperation::Allocate
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AllocateServiceArgs {
    #[serde(default = "default_service_operation")]
    operation: ServiceOperation,
    service_name: String,
    #[serde(default)]
    preferred_port: Option<u16>,
    #[serde(default = "default_service_scheme")]
    scheme: String,
}

fn default_service_scheme() -> String {
    "http".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServiceAllocation {
    port: u16,
    scheme: String,
}

#[derive(Default)]
struct RunServicePortRegistry {
    allocations: BTreeMap<(String, String), ServiceAllocation>,
}

impl RunServicePortRegistry {
    fn allocate(
        &mut self,
        run_id: &str,
        service_name: &str,
        preferred_port: Option<u16>,
        scheme: &str,
    ) -> Result<ServiceAllocation, String> {
        let key = (run_id.to_string(), service_name.to_string());
        if let Some(existing) = self.allocations.get(&key) {
            return Ok(existing.clone());
        }
        let reserved = self
            .allocations
            .values()
            .map(|allocation| allocation.port)
            .collect::<BTreeSet<_>>();
        let port = match preferred_port.filter(|port| !reserved.contains(port)) {
            Some(port) if loopback_port_is_available(port) => port,
            _ => allocate_unreserved_loopback_port(&reserved)?,
        };
        let allocation = ServiceAllocation {
            port,
            scheme: scheme.to_string(),
        };
        self.allocations.insert(key, allocation.clone());
        Ok(allocation)
    }

    fn release(&mut self, run_id: &str, service_name: &str) -> Option<ServiceAllocation> {
        self.allocations
            .remove(&(run_id.to_string(), service_name.to_string()))
    }
}

fn loopback_port_is_available(port: u16) -> bool {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).is_ok()
}

fn allocate_unreserved_loopback_port(reserved: &BTreeSet<u16>) -> Result<u16, String> {
    for _ in 0..32 {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("failed to allocate loopback service port: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("failed to inspect loopback service port: {error}"))?
            .port();
        if !reserved.contains(&port) {
            return Ok(port);
        }
    }
    Err("failed to allocate an unreserved loopback service port".to_string())
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReportedResultStatus {
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportOutcomeArgs {
    #[serde(alias = "status")]
    result_status: ReportedResultStatus,
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
            TASK_PROCESS_ALLOCATE_SERVICE_TOOL => {
                let args: AllocateServiceArgs =
                    serde_json::from_value(invocation.arguments.clone()).map_err(|error| {
                        format!("invalid Task service allocation input: {error}")
                    })?;
                let service_name = args.service_name.trim();
                if service_name.is_empty()
                    || service_name.len() > 64
                    || !service_name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
                {
                    return Err(
                        "service_name must be 1..=64 ASCII letters, digits, '.', '_' or '-'"
                            .to_string(),
                    );
                }
                if !matches!(args.scheme.as_str(), "http" | "https") {
                    return Err("scheme must be http or https".to_string());
                }
                let mut services = self
                    .services
                    .lock()
                    .map_err(|_| "Task service allocation registry is unavailable".to_string())?;
                match args.operation {
                    ServiceOperation::Allocate => {
                        let allocation = services.allocate(
                            &invocation.run_id,
                            service_name,
                            args.preferred_port,
                            &args.scheme,
                        )?;
                        json!({
                            "allocated": true,
                            "run_id": invocation.run_id,
                            "service_name": service_name,
                            "host": "127.0.0.1",
                            "port": allocation.port,
                            "service_url": format!("{}://127.0.0.1:{}", allocation.scheme, allocation.port),
                            "preferred_port": args.preferred_port,
                            "preferred_port_honored": args.preferred_port == Some(allocation.port),
                            "environment": {
                                "CHATOS_SERVICE_HOST": "127.0.0.1",
                                "CHATOS_SERVICE_PORT": allocation.port.to_string(),
                                "CHATOS_SERVICE_URL": format!("{}://127.0.0.1:{}", allocation.scheme, allocation.port)
                            }
                        })
                    }
                    ServiceOperation::Release => {
                        let released = services.release(&invocation.run_id, service_name);
                        json!({
                            "released": released.is_some(),
                            "run_id": invocation.run_id,
                            "service_name": service_name,
                            "port": released.map(|allocation| allocation.port)
                        })
                    }
                }
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
                // A successful Plugin call is neither necessary (an authorized fallback
                // may satisfy research) nor sufficient (opening a session is not delivery).
                // Validate the frozen acceptance contract instead of inventing a global
                // Plugin artifact requirement. This checks report structure, not evidence truth.
                validate_task_acceptance_report(&run.input, &invocation.arguments)?;
                let reason = args.reason.trim();
                if reason.is_empty() || reason.chars().count() > OUTCOME_REASON_MAX_CHARS {
                    return Err(format!(
                        "Task outcome reason must be 1..={OUTCOME_REASON_MAX_CHARS} characters"
                    ));
                }
                let result_status = match args.result_status {
                    ReportedResultStatus::Succeeded => "succeeded",
                    ReportedResultStatus::Failed => "failed",
                    ReportedResultStatus::Blocked => "blocked",
                };
                let checks = args.acceptance_checks.map(|checks| checks.into_iter().map(|check| json!({
                    "criterion_index": check.criterion_index, "status": check.status, "evidence": check.evidence
                })).collect::<Vec<_>>());
                json!({
                    "reported": true,
                    "result_status": result_status,
                    // Compatibility alias for already-installed clients. This is an
                    // internal result, never the Host-owned execution status.
                    "status": result_status,
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
        assert_eq!(tools[0]["name"], TASK_PROCESS_ALLOCATE_SERVICE_TOOL);
        assert_eq!(tools[1]["name"], TASK_PROCESS_RECORD_TOOL);
        assert_eq!(tools[2]["name"], TASK_OUTCOME_REPORT_TOOL);
        assert_eq!(
            tools[2]["parameters"]["properties"]["acceptance_checks"]["maxItems"],
            50
        );
        assert_eq!(
            tools[2]["parameters"]["properties"]["final_response"]["maxLength"],
            32000
        );
        let prompt = task_process_prompt_item().to_string();
        assert!(prompt.contains("meaningful milestones"));
        assert!(prompt.contains(TASK_PROCESS_ALLOCATE_SERVICE_TOOL));
        assert!(prompt.contains("final tool call"));
    }

    #[test]
    fn service_registry_avoids_busy_and_run_reserved_ports() {
        let occupied = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .expect("occupied listener");
        let occupied_port = occupied.local_addr().unwrap().port();
        let mut registry = RunServicePortRegistry::default();
        let first = registry
            .allocate("run-1", "backend", Some(occupied_port), "http")
            .expect("first allocation");
        assert_ne!(first.port, occupied_port);
        let same = registry
            .allocate("run-1", "backend", Some(18080), "http")
            .expect("idempotent allocation");
        assert_eq!(same, first);
        let second = registry
            .allocate("run-1", "frontend", Some(first.port), "http")
            .expect("second allocation");
        assert_ne!(second.port, first.port);
        assert_eq!(registry.release("run-1", "backend"), Some(first));
        assert!(registry.release("run-1", "backend").is_none());
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
    async fn outcome_executor_separates_unverified_acceptance_from_execution_and_enforces_limit() {
        let executor = LocalTaskProcessToolExecutor::new(runtime_with_input("task_execution","task",json!({"acceptance_report_version":1,"acceptance_criteria":["Short response"],"final_response_max_chars":4})).await,"user-1").unwrap();
        let mut report = json!({"status":"succeeded","reason":"done","final_response":"中文\n!!","acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Checked actual final response"}]});
        let error = executor
            .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
            .await
            .unwrap_err();
        assert!(error.contains("5 characters"));
        report["final_response"] = json!("中文");
        report["acceptance_checks"][0]["status"] = json!("not_verified");
        let LocalAgentToolOutcome::Succeeded { output } = executor
            .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
            .await
            .expect("completed evaluation may have an unverified finding")
        else {
            panic!("accepted incomplete acceptance report")
        };
        assert_eq!(output["status"], "succeeded");
        assert_eq!(output["acceptance_checks"][0]["status"], "not_verified");
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
    async fn selected_research_plugin_does_not_require_successful_plugin_delivery() {
        let executor = LocalTaskProcessToolExecutor::new(
            runtime_with_input(
                "task_execution",
                "task",
                json!({
                    "acceptance_report_version": 1,
                    "acceptance_criteria": ["Attempt browser research; HTTP fallback may supply the sourced text report"],
                    "tool_options": {
                        "requires_execution": true,
                        "enabled_builtin_kinds": ["TerminalController"],
                        "plugin_hints": [{"plugin_key":"browser-cdp","component_key":"browser-cdp-mcp","reason":"research; fall back to HTTP when unavailable"}]
                    }
                }),
            )
            .await,
            "user-1",
        )
        .unwrap();
        // No successful capability_invoke exists in this Run. The report still must
        // cover the actual criterion and disclose the fallback, not invent an artifact.
        let outcome = executor
            .execute_tool(&invocation(
                TASK_OUTCOME_REPORT_TOOL,
                json!({
                    "status":"succeeded",
                    "reason":"Research delivered using the allowed fallback; screenshot unavailable",
                    "final_response":"HTTP sources and research findings; no visual layout claims.",
                    "acceptance_checks":[{"criterion_index":0,"status":"passed","evidence":"Browser unavailable; HTTP GET returned 200 and report cites the retrieved content"}]
                }),
            ))
            .await
            .expect("allowed fallback must not require a successful Plugin invocation");
        let LocalAgentToolOutcome::Succeeded { output } = outcome else {
            panic!("expected accepted research report")
        };
        assert_eq!(output["status"], "succeeded");
    }

    #[tokio::test]
    async fn completed_plugin_attempt_can_report_an_unverified_deliverable() {
        let executor = LocalTaskProcessToolExecutor::new(
            runtime_with_input(
                "task_execution",
                "task",
                json!({
                    "acceptance_report_version":1,
                    "acceptance_criteria":["Persist and verify the requested plan in Solution Studio"],
                    "tool_options":{"plugin_hints":[{"plugin_key":"solution-studio","component_key":"solution-authoring-mcp","reason":"persist the plan"}]}
                }),
            ).await,
            "user-1",
        ).unwrap();
        let mut report = json!({
            "status":"succeeded",
            "reason":"Only a text draft exists; Plugin persistence unavailable",
            "final_response":"Draft plan; no verified Plugin deliverable.",
            "acceptance_checks":[{"criterion_index":0,"status":"not_verified","evidence":"No persisted plan could be verified in Solution Studio"}]
        });
        let outcome = executor
            .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report.clone()))
            .await
            .expect("execution completion and acceptance findings are separate");
        let LocalAgentToolOutcome::Succeeded { output } = outcome else {
            panic!("expected accepted report with incomplete acceptance")
        };
        assert_eq!(output["status"], "succeeded");
        assert_eq!(output["acceptance_checks"][0]["status"], "not_verified");
        report["status"] = json!("blocked");
        let outcome = executor
            .execute_tool(&invocation(TASK_OUTCOME_REPORT_TOOL, report))
            .await
            .unwrap();
        let LocalAgentToolOutcome::Succeeded { output } = outcome else {
            panic!("expected accepted blocked report")
        };
        assert_eq!(output["status"], "blocked");
    }

    #[tokio::test]
    async fn result_status_is_preserved_only_as_the_internal_dependency_gate() {
        let executor = LocalTaskProcessToolExecutor::new(
            runtime_with_input(
                "task_execution",
                "task",
                json!({
                    "acceptance_report_version":1,
                    "acceptance_criteria":["Inspect the host"]
                }),
            )
            .await,
            "user-1",
        )
        .unwrap();
        let LocalAgentToolOutcome::Succeeded { output } = executor
            .execute_tool(&invocation(
                TASK_OUTCOME_REPORT_TOOL,
                json!({
                    "result_status":"blocked",
                    "reason":"The required host connection is unavailable",
                    "final_response":"No inspection could be performed because the required connection is unavailable.",
                    "acceptance_checks":[
                        {"criterion_index":0,"status":"not_verified","evidence":"No authorized connection was available, so no host evidence could be collected"}
                    ]
                }),
            ))
            .await
            .expect("the internal result may block downstream work")
        else {
            panic!("expected accepted blocked report")
        };
        assert_eq!(output["result_status"], "blocked");
        assert_eq!(
            output["status"], "blocked",
            "legacy alias remains internal only"
        );
    }
}
