// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
#[cfg(target_os = "macos")]
use crate::infrastructure::MacOsSeatbeltHookRunner;
#[cfg(windows)]
use crate::infrastructure::WindowsAppContainerHookRunner;
use crate::{
    HostRequestHandler, LocalAgentHostCoordinator, LocalAgentScheduler, LocalToolExecutor,
    LocalToolRegistry, LocalToolScheduler, SchedulerTick, ToolSchedulerTick,
};
use chatos_agent_profiles::MAIN_CHAT_PROFILE_KEY;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_ports::{IdempotentCommand, LocalAgentRunStore, LocalPluginHookBundleStore};
use chatos_local_agent_protocol::{
    CancelRunCommand, ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand,
    CommitToolCommand, CreateConversationCommand, CreateRunCommand, GetConversationCommand,
    HostCommand, HostRequestEnvelope, HostResult, LocalAgentRunStatus, LocalAgentStepOutcome,
    LocalAgentToolApprovalStatus, LocalAgentToolCall, LocalAgentToolExecutionRoute,
    LocalAgentToolOutcome, LocalAgentToolStatus, LocalPluginHookBundleSpec,
    LocalPluginPlanningTaskTemplate, LocalPluginTaskPolicy, LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileRegistry};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};

#[derive(Clone, Default)]
struct FakeHookRunner {
    calls: Arc<Mutex<Vec<String>>>,
    payloads: Arc<Mutex<Vec<Value>>>,
    session_sources: Arc<Mutex<Vec<String>>>,
    active: Arc<AtomicUsize>,
    maximum_active: Arc<AtomicUsize>,
}

struct NeverModelProfile;

struct FinalModelProfile;

struct CountingToolExecutor {
    executions: Arc<AtomicUsize>,
}

#[async_trait]
impl LocalToolExecutor for CountingToolExecutor {
    async fn execute_tool(
        &self,
        _invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        Ok(LocalAgentToolOutcome::Succeeded {
            output: json!({"executed": true}),
        })
    }
}

#[async_trait]
impl LocalAgentProfile for NeverModelProfile {
    async fn execute_step(
        &self,
        _claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        Err("model must not run for a blocked prompt".to_string())
    }
}

#[async_trait]
impl LocalAgentProfile for FinalModelProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        Ok(LocalAgentStepOutcome::Succeed {
            output: json!({
                "content": format!("candidate {}", claim.run.iteration),
                "reasoning": null,
                "tool_calls": null,
                "finish_reason": "stop",
                "usage": null,
                "response_id": format!("response-{}", claim.run.iteration),
                "response_output_items": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "candidate"}]
                }],
                "request_input_items": [{"role": "user", "content": "hello"}]
            }),
        })
    }
}

impl FakeHookRunner {
    fn call_count(&self) -> usize {
        self.calls.lock().expect("calls").len()
    }

    fn events(&self) -> Vec<String> {
        self.calls.lock().expect("calls").clone()
    }

    fn payloads(&self) -> Vec<Value> {
        self.payloads.lock().expect("payloads").clone()
    }

    fn session_sources(&self) -> Vec<String> {
        self.session_sources.lock().expect("sources").clone()
    }
}

#[async_trait]
impl LocalHookRunner for FakeHookRunner {
    async fn run(
        &self,
        invocation: LocalHookInvocation,
    ) -> Result<LocalHookInvocationResult, String> {
        let payload: Value = serde_json::from_slice(&invocation.stdin_json).expect("payload");
        let event = payload["hook_event_name"]
            .as_str()
            .expect("event")
            .to_string();
        self.calls.lock().expect("calls").push(event.clone());
        self.payloads
            .lock()
            .expect("payloads")
            .push(payload.clone());
        if event == "SessionStart" {
            self.session_sources
                .lock()
                .expect("sources")
                .push(payload["source"].as_str().expect("source").to_string());
        }
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum_active.fetch_max(active, Ordering::SeqCst);
        if invocation.hook.command.contains("long-delay") {
            tokio::time::sleep(Duration::from_millis(250)).await;
        } else if invocation.hook.command.contains("delay") {
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        let (exit_code, stdout, stderr) = match invocation.hook.command.as_str() {
            "block" => (
                Some(0),
                br#"{"continue":false,"stopReason":"blocked by fixture"}"#.to_vec(),
                Vec::new(),
            ),
            "block-changing" => (
                Some(0),
                serde_json::to_vec(&json!({
                    "continue": false,
                    "stopReason": format!(
                        "changing fixture reason {}",
                        payload["stop_block_count"].as_u64().unwrap_or_default() + 1
                    )
                }))
                .expect("changing block output"),
                Vec::new(),
            ),
            "invalid" => (Some(0), b"not-json".to_vec(), b"private failure".to_vec()),
            "deny-tool" => (
                Some(0),
                serde_json::to_vec(&json!({
                    "hookSpecificOutput": {
                        "hookEventName": event,
                        "permissionDecision": "deny",
                        "permissionDecisionReason": "blocked by tool policy",
                    }
                }))
                .expect("deny output"),
                Vec::new(),
            ),
            "update-tool" => (
                Some(0),
                serde_json::to_vec(&json!({
                    "hookSpecificOutput": {
                        "hookEventName": event,
                        "permissionDecision": "allow",
                        "updatedInput": {"path": "rewritten.txt"},
                    }
                }))
                .expect("update output"),
                Vec::new(),
            ),
            "invalid-update-tool" => (
                Some(0),
                serde_json::to_vec(&json!({
                    "hookSpecificOutput": {
                        "hookEventName": event,
                        "permissionDecision": "allow",
                        "updatedInput": {"path": 42},
                    }
                }))
                .expect("invalid update output"),
                Vec::new(),
            ),
            "permission-deny" => (
                Some(0),
                serde_json::to_vec(&json!({
                    "decision": {
                        "behavior": "deny",
                        "message": "approval Hook denied this call",
                    }
                }))
                .expect("permission output"),
                Vec::new(),
            ),
            command => {
                let context = command
                    .strip_prefix("long-delay:")
                    .or_else(|| command.strip_prefix("delay:"))
                    .unwrap_or(command);
                (
                    Some(0),
                    serde_json::to_vec(&json!({
                        "hookSpecificOutput": {
                            "hookEventName": event,
                            "additionalContext": context,
                        }
                    }))
                    .expect("output"),
                    Vec::new(),
                )
            }
        };
        Ok(LocalHookInvocationResult {
            exit_code,
            stdout,
            stderr,
            stdout_overflow: false,
            stderr_overflow: false,
            timed_out: false,
            duration_ms: 1,
        })
    }
}

async fn fixture() -> (
    Arc<SqliteClientStorage>,
    Arc<LocalAgentRuntime>,
    FakeHookRunner,
) {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::with_clock(
        storage.clone(),
        Arc::new(|| Ok(10_000)),
    ));
    runtime.initialize("user-1").await.expect("initialize");
    (storage, runtime, FakeHookRunner::default())
}

async fn activate_bundle(
    storage: &SqliteClientStorage,
    bundle_id: &str,
    hooks: Vec<LocalPluginHookDefinition>,
) {
    activate_bundle_with_policy(storage, bundle_id, hooks, None).await;
}

async fn activate_bundle_with_policy(
    storage: &SqliteClientStorage,
    bundle_id: &str,
    hooks: Vec<LocalPluginHookDefinition>,
    task_policy: Option<LocalPluginTaskPolicy>,
) {
    let bundle = LocalPluginHookBundleSpec {
        bundle_id: bundle_id.to_string(),
        owner_user_id: "user-1".to_string(),
        plugin_id: format!("plugin-{bundle_id}"),
        release_id: "release-1".to_string(),
        component_key: "hooks-1".to_string(),
        artifact_sha256: "a".repeat(64),
        content_sha256: "b".repeat(64),
        normalized_hook_sha256: "c".repeat(64),
        command_sha256: "d".repeat(64),
        permission_snapshot_sha256: "e".repeat(64),
        runner_policy_version: 1,
        installation_root: "/tmp/chatos-hook-fixture".to_string(),
        permissions: Vec::new(),
        task_policy,
        hooks,
        enabled: true,
        trusted_at_unix_ms: 9_000,
    };
    storage
        .put_plugin_hook_bundle(&command(&format!("put-{bundle_id}")), &bundle, 10_000)
        .await
        .expect("put bundle");
    let mut active = storage
        .list_plugin_hook_bundles("user-1")
        .await
        .expect("active bundles")
        .into_iter()
        .map(|record| record.spec.bundle_id)
        .collect::<Vec<_>>();
    active.push(bundle_id.to_string());
    active.sort();
    active.dedup();
    storage
        .set_active_plugin_hook_bundles(
            &command(&format!("activate-{bundle_id}")),
            "user-1",
            &active,
            10_001,
        )
        .await
        .expect("activate bundle");
}

fn plan_first_policy() -> LocalPluginTaskPolicy {
    LocalPluginTaskPolicy {
        policy_id: "plan-first".to_string(),
        require_main_chat_task_route: true,
        require_confirmed_plan_before_execution: true,
        planning_task: LocalPluginPlanningTaskTemplate {
            required_builtin_kinds: vec![
                "CodeMaintainerRead".to_string(),
                "RequirementSurveyRead".to_string(),
                "RequirementSurveyWrite".to_string(),
            ],
            requires_execution: true,
        },
    }
}

async fn run_with_successful_tool(
    runtime: &LocalAgentRuntime,
    run_id: &str,
    tool_name: &str,
    arguments: Value,
) -> LocalAgentRunRecord {
    runtime
        .try_handle(envelope(
            &format!("create-{run_id}"),
            HostCommand::CreateRun(CreateRunCommand {
                run_id: run_id.to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation_turn".to_string(),
                owner_entity_id: format!("turn-{run_id}"),
                profile_key: MAIN_CHAT_PROFILE_KEY.to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({
                    "conversation_id": format!("conversation-{run_id}"),
                    "turn_id": format!("turn-{run_id}"),
                    "message": "hello",
                }),
                max_iterations: 8,
            }),
        ))
        .await
        .expect("create route Run");
    let HostResult::Claim { claim: Some(claim) } = runtime
        .try_handle(envelope(
            &format!("claim-{run_id}"),
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 30_000,
            }),
        ))
        .await
        .expect("claim route Run")
    else {
        panic!("expected route Run claim")
    };
    runtime
        .try_handle(envelope(
            &format!("request-{run_id}"),
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: run_id.to_string(),
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: format!("batch-{run_id}"),
                    tool_calls: vec![LocalAgentToolCall {
                        call_id: format!("call-{run_id}"),
                        tool_name: tool_name.to_string(),
                        arguments,
                        side_effecting: false,
                        requires_approval: false,
                    }],
                    checkpoint: json!({"response": {"response_output_items": []}}),
                },
            }),
        ))
        .await
        .expect("request route tool");
    let HostResult::ToolClaim { claim: Some(tool) } = runtime
        .try_handle(envelope(
            &format!("claim-tool-{run_id}"),
            HostCommand::ClaimNextTool(ClaimNextToolCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "tool-worker".to_string(),
                lease_duration_ms: 30_000,
                include_tool_names: Some(vec![tool_name.to_string()]),
                exclude_tool_names: Vec::new(),
                execution_route: None,
            }),
        ))
        .await
        .expect("claim route tool")
    else {
        panic!("expected route tool claim")
    };
    let HostResult::ToolCommit { result } = runtime
        .try_handle(envelope(
            &format!("commit-tool-{run_id}"),
            HostCommand::CommitTool(CommitToolCommand {
                owner_user_id: "user-1".to_string(),
                invocation_id: tool.invocation.invocation_id,
                claim_token: tool.claim_token,
                expected_version: tool.invocation.version,
                outcome: LocalAgentToolOutcome::Succeeded {
                    output: json!({"recorded": true}),
                },
            }),
        ))
        .await
        .expect("commit route tool")
    else {
        panic!("expected route tool commit")
    };
    result.run
}

fn command(id: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: id.to_string(),
        request_fingerprint: format!("fingerprint-{id}"),
        persist_receipt: true,
    }
}

fn hook(hook_id: &str, event: &str, command: &str) -> LocalPluginHookDefinition {
    LocalPluginHookDefinition {
        hook_id: hook_id.to_string(),
        event: event.to_string(),
        matcher: None,
        command: command.to_string(),
        is_async: false,
        timeout_ms: 1_000,
        additional_context_limit: 64 * 1_024,
    }
}

fn async_hook(hook_id: &str, event: &str, command: &str) -> LocalPluginHookDefinition {
    let mut hook = hook(hook_id, event, command);
    hook.is_async = true;
    hook
}

fn matched_hook(
    hook_id: &str,
    event: &str,
    matcher: &str,
    command: &str,
) -> LocalPluginHookDefinition {
    let mut hook = hook(hook_id, event, command);
    hook.matcher = Some(matcher.to_string());
    hook
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": "dangerous",
            "parameters": {
                "type": "object",
                "properties": {"value": {"type": "integer"}},
                "required": ["value"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "parameters": {
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "name": "write_file",
            "parameters": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
                "additionalProperties": false
            }
        }),
    ]
}

fn tool_definition(name: &str) -> Value {
    json!({
        "type": "function",
        "name": name,
        "parameters": {
            "type": "object",
            "properties": {},
            "additionalProperties": true
        }
    })
}

fn run(run_id: &str) -> LocalAgentRunRecord {
    LocalAgentRunRecord {
        run_id: run_id.to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation_turn".to_string(),
        owner_entity_id: "turn-1".to_string(),
        profile_key: MAIN_CHAT_PROFILE_KEY.to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({
            "conversation_id": "conversation-1",
            "turn_id": "turn-1",
            "message": "hello",
        }),
        status: LocalAgentRunStatus::ModelRunning,
        iteration: 0,
        model_attempt: 1,
        max_iterations: 8,
        version: 1,
        claim_token: Some("token-1".to_string()),
        claim_until_unix_ms: Some(20_000),
        next_attempt_at_unix_ms: None,
        pending_tool_batch: None,
        checkpoint: Value::Null,
        continuation_input: None,
        terminal_outcome: None,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    }
}

fn claim(run: LocalAgentRunRecord) -> LocalAgentRunClaim {
    LocalAgentRunClaim {
        worker_id: "worker-1".to_string(),
        claim_token: "token-1".to_string(),
        run,
    }
}

async fn create_claimed_run(runtime: &LocalAgentRuntime, run_id: &str) -> LocalAgentRunClaim {
    runtime
        .try_handle(envelope(
            &format!("create-{run_id}"),
            HostCommand::CreateRun(CreateRunCommand {
                run_id: run_id.to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation_turn".to_string(),
                owner_entity_id: format!("turn-{run_id}"),
                profile_key: MAIN_CHAT_PROFILE_KEY.to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({
                    "conversation_id": "conversation-1",
                    "turn_id": format!("turn-{run_id}"),
                    "message": "hello",
                }),
                max_iterations: 8,
            }),
        ))
        .await
        .expect("create run");
    let result = runtime
        .try_handle(envelope(
            &format!("claim-{run_id}"),
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await
        .expect("claim run");
    let HostResult::Claim { claim: Some(claim) } = result else {
        panic!("expected run claim: {result:?}")
    };
    assert_eq!(claim.run.run_id, run_id);
    claim
}

include!("tests/lifecycle.rs");
include!("tests/tool_hooks.rs");
include!("tests/session_hooks.rs");
include!("tests/parsing.rs");
include!("tests/integration.rs");
