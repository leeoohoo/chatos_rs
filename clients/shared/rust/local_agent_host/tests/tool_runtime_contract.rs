// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chatos_agent_profiles::{
    TaskRunnerCapabilitySnapshot, TaskRunnerExecutionTool, TaskRunnerProjectSnapshot,
    TaskRunnerPromptSnapshot,
};
use chatos_client_storage::{
    ClientStorage, RecordScope, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageEncryptionKey,
};
use chatos_local_agent_host::{
    FrozenCapabilityLocalToolRuntime, FrozenMcpExecutor, FrozenMcpExecutorProvider,
    FrozenMcpExecutorRequest,
};
use chatos_local_agent_protocol::{
    ContextStrategy, FrozenSnapshot, ModelProtocol, ModelRuntimeDescriptor, ToolEffect,
    ToolExecutionStatus,
};
use chatos_local_agent_runtime::{
    create_local_agent_task, CreateLocalAgentRunRequest, CreateLocalAgentTaskRequest,
    InitialRunMessage, LocalToolInvocation, LocalToolRuntime,
};
use chatos_mcp_client::{LocalMcpExecutor, LocalMcpToolCall, LocalMcpToolResult};
use chrono::Utc;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

const TOOL_NAME: &str = "fixture_execute";

struct FixtureExecutor {
    calls: Arc<AtomicUsize>,
    fail: bool,
}

#[async_trait]
impl LocalMcpExecutor for FixtureExecutor {
    fn available_tools(&self) -> Vec<Value> {
        vec![json!({
            "type": "function",
            "name": TOOL_NAME,
            "description": "Execute one frozen local operation",
            "parameters": {
                "type": "object",
                "properties": {"value": {"type": "string"}},
                "required": ["value"],
                "additionalProperties": false
            }
        })]
    }

    async fn execute_tool(
        &self,
        call: LocalMcpToolCall,
        cancellation: CancellationToken,
    ) -> Result<LocalMcpToolResult, String> {
        if cancellation.is_cancelled() {
            return Err("cancelled".to_string());
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err("fixture rejected operation".to_string());
        }
        Ok(LocalMcpToolResult {
            content: format!(
                "saved {} at /Users/alice/private/result.txt",
                call.arguments["value"].as_str().unwrap_or_default()
            ),
            structured_result: Some(json!({
                "path": "/Volumes/private/result.txt",
                "route": "/design-preview"
            })),
            is_error: false,
            fatal_error: false,
        })
    }
}

struct PinnedExecutorProvider {
    release_snapshot: Value,
    executor: Arc<dyn LocalMcpExecutor>,
}

#[async_trait]
impl FrozenMcpExecutorProvider for PinnedExecutorProvider {
    async fn resolve(
        &self,
        _request: &FrozenMcpExecutorRequest,
        cancellation: CancellationToken,
    ) -> Result<FrozenMcpExecutor, String> {
        if cancellation.is_cancelled() {
            return Err("cancelled".to_string());
        }
        Ok(FrozenMcpExecutor {
            plugin_release_snapshot: self.release_snapshot.clone(),
            executor: self.executor.clone(),
        })
    }
}

struct Fixture {
    runtime: FrozenCapabilityLocalToolRuntime,
    invocation: LocalToolInvocation,
    calls: Arc<AtomicUsize>,
}

async fn fixture(
    frozen_effect: ToolEffect,
    invocation_effect: ToolEffect,
    mutate_schema: bool,
    resolved_release: Value,
    fail: bool,
) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let storage: Arc<dyn ClientStorage> = Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.keep().join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:tool-runtime-key").unwrap(),
            },
            &StorageEncryptionKey::new([71; 32]),
        )
        .await
        .unwrap(),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let executor: Arc<dyn LocalMcpExecutor> = Arc::new(FixtureExecutor {
        calls: calls.clone(),
        fail,
    });
    let mut schema = executor.available_tools().remove(0);
    assert_eq!(schema["name"], TOOL_NAME);
    if mutate_schema {
        schema["description"] = Value::String("tampered after planning".to_string());
    }
    let plugin_release_snapshot = json!({
        "plugins": [{"plugin_id": "fixture", "release_id": "release-1"}]
    });
    let prompt_snapshot = FrozenSnapshot::new(
        "prompt-snapshot-1",
        "prompt-revision-1",
        serde_json::to_value(TaskRunnerPromptSnapshot {
            prompt_revision: "prompt-revision-1".to_string(),
            base_system_prompt: "Execute the local task carefully.".to_string(),
            task_prompt: "Preserve the approved design.".to_string(),
            skill_snapshot: json!({"skills": []}),
        })
        .unwrap(),
    )
    .unwrap();
    let project_snapshot = FrozenSnapshot::new(
        "project-snapshot-1",
        "project-revision-1",
        serde_json::to_value(TaskRunnerProjectSnapshot {
            project_id: "project-1".to_string(),
            snapshot_revision: "project-revision-1".to_string(),
            working_directory_ref: "workspace-grant-1".to_string(),
            authority_snapshot: json!({"device_id": "device-1"}),
        })
        .unwrap(),
    )
    .unwrap();
    let capability_snapshot = FrozenSnapshot::new(
        "capability-snapshot-1",
        "capability-revision-1",
        serde_json::to_value(TaskRunnerCapabilitySnapshot {
            snapshot_ref: "capability-snapshot-1".to_string(),
            plugin_release_snapshot: plugin_release_snapshot.clone(),
            execution_tools: vec![TaskRunnerExecutionTool {
                name: TOOL_NAME.to_string(),
                effect: frozen_effect,
                schema,
            }],
        })
        .unwrap(),
    )
    .unwrap();
    let now = Utc::now();
    create_local_agent_task(
        storage.as_ref(),
        CreateLocalAgentTaskRequest {
            run: CreateLocalAgentRunRequest {
                scope: scope(),
                run_id: "run-1".to_string(),
                profile_key: "task_runner".to_string(),
                owner_entity_type: "task".to_string(),
                owner_entity_id: "task-1".to_string(),
                project_id: Some("project-1".to_string()),
                model_runtime_snapshot: descriptor(),
                prompt_revision: prompt_snapshot.revision.clone(),
                capability_snapshot_ref: capability_snapshot.snapshot_id.clone(),
                origin_device_id: "device-1".to_string(),
                causation_id: "create-task-1".to_string(),
                deadline_at: None,
                initial_message: Some(InitialRunMessage {
                    record_id: "message-1".to_string(),
                    turn_id: "turn-1".to_string(),
                    content: Some("Execute the fixture".to_string()),
                    structured_payload: Some(json!({"type": "task_objective"})),
                    message_source: "task_creation".to_string(),
                }),
                initial_attachments: Vec::new(),
                now,
            },
            task_id: "task-1".to_string(),
            source_thread_id: "thread-1".to_string(),
            source_turn_id: "turn-1".to_string(),
            project_id: "project-1".to_string(),
            objective: "Execute the fixture".to_string(),
            acceptance_criteria: vec!["The fixture completed".to_string()],
            prompt_snapshot,
            project_snapshot,
            capability_snapshot,
        },
    )
    .await
    .unwrap();
    let runtime = FrozenCapabilityLocalToolRuntime::new(
        storage,
        scope(),
        Arc::new(PinnedExecutorProvider {
            release_snapshot: resolved_release,
            executor,
        }),
    );
    Fixture {
        runtime,
        invocation: LocalToolInvocation {
            invocation_id: "invocation-1".to_string(),
            run_id: "run-1".to_string(),
            batch_id: "batch-1".to_string(),
            source_turn_id: "turn-2".to_string(),
            project_id: Some("project-1".to_string()),
            capability_snapshot_ref: "capability-snapshot-1".to_string(),
            tool_call_id: "call-1".to_string(),
            tool_name: TOOL_NAME.to_string(),
            effect: invocation_effect,
            arguments: json!({"value": "approved-design"}),
        },
        calls,
    }
}

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "user-1".to_string(),
    }
}

fn descriptor() -> ModelRuntimeDescriptor {
    ModelRuntimeDescriptor {
        model_config_id: "model-1".to_string(),
        revision: 1,
        provider: "openai".to_string(),
        model: "gpt-test".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: ContextStrategy::ProviderNative,
        supports_streaming: true,
        supports_native_compaction: true,
        supports_input_token_count: true,
    }
}

fn release_snapshot() -> Value {
    json!({"plugins": [{"plugin_id": "fixture", "release_id": "release-1"}]})
}

#[tokio::test]
async fn executes_only_the_exact_frozen_tool_and_builds_a_sanitized_receipt() {
    let fixture = fixture(
        ToolEffect::Write,
        ToolEffect::Write,
        false,
        release_snapshot(),
        false,
    )
    .await;
    let outcome = fixture
        .runtime
        .execute(fixture.invocation, CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(outcome.status, ToolExecutionStatus::Succeeded);
    assert_eq!(outcome.bounded_result["verification"], true);
    assert!(outcome.bounded_result["summary"]
        .as_str()
        .unwrap()
        .contains("[local-path-redacted]"));
    assert_eq!(
        outcome.bounded_result["structured_result"]["path"],
        "[local-path-redacted]"
    );
    assert_eq!(
        outcome.bounded_result["structured_result"]["route"],
        "/design-preview"
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn converts_an_mcp_failure_into_a_durable_failed_outcome() {
    let fixture = fixture(
        ToolEffect::Read,
        ToolEffect::Read,
        false,
        release_snapshot(),
        true,
    )
    .await;
    let outcome = fixture
        .runtime
        .execute(fixture.invocation, CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(outcome.status, ToolExecutionStatus::Failed);
    assert_eq!(outcome.bounded_result["verification"], false);
    assert!(outcome.bounded_result["summary"]
        .as_str()
        .unwrap()
        .contains("fixture rejected operation"));
}

#[tokio::test]
async fn rejects_tampered_effect_project_capability_and_tool_before_execution() {
    let cases = ["effect", "project", "capability", "tool"];
    for case in cases {
        let mut fixture = fixture(
            ToolEffect::Read,
            ToolEffect::Read,
            false,
            release_snapshot(),
            false,
        )
        .await;
        match case {
            "effect" => fixture.invocation.effect = ToolEffect::Write,
            "project" => fixture.invocation.project_id = Some("project-2".to_string()),
            "capability" => fixture.invocation.capability_snapshot_ref = "capability-2".to_string(),
            "tool" => fixture.invocation.tool_name = "fixture_unknown".to_string(),
            _ => unreachable!(),
        }
        assert!(fixture
            .runtime
            .execute(fixture.invocation, CancellationToken::new())
            .await
            .is_err());
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0, "case {case}");
    }
}

#[tokio::test]
async fn rejects_schema_or_plugin_release_drift_before_execution() {
    let schema_drift = fixture(
        ToolEffect::Read,
        ToolEffect::Read,
        true,
        release_snapshot(),
        false,
    )
    .await;
    let error = schema_drift
        .runtime
        .execute(schema_drift.invocation, CancellationToken::new())
        .await
        .unwrap_err();
    assert!(error.contains("schema does not match"));
    assert_eq!(schema_drift.calls.load(Ordering::SeqCst), 0);

    let release_drift = fixture(
        ToolEffect::Read,
        ToolEffect::Read,
        false,
        json!({"plugins": [{"plugin_id": "fixture", "release_id": "release-2"}]}),
        false,
    )
    .await;
    let error = release_drift
        .runtime
        .execute(release_drift.invocation, CancellationToken::new())
        .await
        .unwrap_err();
    assert!(error.contains("do not match the frozen plugin release"));
    assert_eq!(release_drift.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancellation_prevents_dispatch() {
    let fixture = fixture(
        ToolEffect::Read,
        ToolEffect::Read,
        false,
        release_snapshot(),
        false,
    )
    .await;
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = fixture
        .runtime
        .execute(fixture.invocation, cancellation)
        .await
        .unwrap_err();
    assert!(error.contains("cancelled"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
}
