// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_agent_profiles::{TaskRunnerExecutionTool, TaskRunnerProjectSnapshot};
use chatos_local_agent_host::{
    FrozenMcpExecutorProvider, FrozenMcpExecutorRequest, LocalTaskCapabilityRequest,
    LocalTaskCapabilityResolver, RegisteredLocalCapabilityBundle, RegisteredLocalCapabilityRuntime,
};
use chatos_local_agent_protocol::ToolEffect;
use chatos_mcp_runtime::{
    BuiltinToolProvider, McpBuiltinServer, McpExecutor, ToolCallContext, ToolStreamChunkCallback,
};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

struct ToolsProvider {
    include_extra: bool,
}

#[async_trait]
impl BuiltinToolProvider for ToolsProvider {
    fn server_name(&self) -> &str {
        "fixture"
    }

    fn list_tools(&self) -> Vec<Value> {
        let mut tools = vec![json!({
            "name": "render",
            "description": "Render the approved design",
            "inputSchema": {
                "type": "object",
                "properties": {"target": {"type": "string"}},
                "required": ["target"],
                "additionalProperties": false
            }
        })];
        if self.include_extra {
            tools.push(json!({
                "name": "unfrozen",
                "description": "Must not leak into a frozen runtime",
                "inputSchema": {"type": "object"}
            }));
        }
        tools
    }

    async fn call_tool(
        &self,
        _name: &str,
        _args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        Ok(json!({"content": [{"type": "text", "text": "ok"}]}))
    }
}

fn executor(include_extra: bool) -> Arc<McpExecutor> {
    Arc::new(
        McpExecutor::builder()
            .with_builtin_server(McpBuiltinServer {
                name: "fixture".to_string(),
                kind: "Fixture".to_string(),
                workspace_dir: String::new(),
                user_id: Some("user-1".to_string()),
                project_id: Some("project-1".to_string()),
                remote_connection_id: None,
                contact_agent_id: None,
                auto_create_task: false,
                allow_writes: true,
                max_file_bytes: 1_000,
                max_write_bytes: 1_000,
                search_limit: 10,
            })
            .with_builtin_provider(ToolsProvider { include_extra })
            .build_builtin_only()
            .unwrap(),
    )
}

fn release_snapshot() -> Value {
    json!({
        "plugins": [{
            "plugin_id": "design-tools",
            "release_id": "release-1",
            "artifact_sha256": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "permission_snapshot": ["workspace.write"]
        }]
    })
}

fn bundle(executor: Arc<McpExecutor>) -> RegisteredLocalCapabilityBundle {
    let schema = executor.available_tools().remove(0);
    RegisteredLocalCapabilityBundle {
        owner_user_id: "user-1".to_string(),
        project_id: "project-1".to_string(),
        resolution_revision: "resolution-1".to_string(),
        plugin_release_snapshot: release_snapshot(),
        execution_tools: vec![TaskRunnerExecutionTool {
            name: "fixture_render".to_string(),
            effect: ToolEffect::Write,
            schema,
        }],
        executor,
    }
}

fn planning_request(project_id: &str) -> LocalTaskCapabilityRequest {
    LocalTaskCapabilityRequest {
        task_id: "task-1".to_string(),
        owner_user_id: "user-1".to_string(),
        project_snapshot: TaskRunnerProjectSnapshot {
            project_id: project_id.to_string(),
            snapshot_revision: "project-revision-1".to_string(),
            working_directory_ref: "workspace-grant-1".to_string(),
            authority_snapshot: json!({"device_id": "device-1"}),
        },
        objective: "Render the approved design".to_string(),
        acceptance_criteria: vec!["Visual verification passes".to_string()],
        parent_capability_snapshot_ref: "main-capabilities-1".to_string(),
    }
}

#[tokio::test]
async fn planning_and_execution_resolve_the_same_project_scoped_bundle() {
    let executor = executor(false);
    let runtime = RegisteredLocalCapabilityRuntime::new();
    runtime.register(bundle(executor.clone())).unwrap();

    let resolution = runtime
        .resolve_capabilities(&planning_request("project-1"), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(resolution.resolution_revision, "resolution-1");
    assert_eq!(resolution.plugin_release_snapshot, release_snapshot());
    assert_eq!(resolution.execution_tools.len(), 1);

    let resolved = runtime
        .resolve(
            &FrozenMcpExecutorRequest {
                owner_user_id: "user-1".to_string(),
                project_id: "project-1".to_string(),
                plugin_release_snapshot: resolution.plugin_release_snapshot,
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&resolved.executor, &executor));
}

#[tokio::test]
async fn owner_project_or_release_drift_fails_closed() {
    let runtime = RegisteredLocalCapabilityRuntime::new();
    runtime.register(bundle(executor(false))).unwrap();

    assert!(runtime
        .resolve_capabilities(&planning_request("project-2"), CancellationToken::new())
        .await
        .is_err());
    for request in [
        FrozenMcpExecutorRequest {
            owner_user_id: "user-2".to_string(),
            project_id: "project-1".to_string(),
            plugin_release_snapshot: release_snapshot(),
        },
        FrozenMcpExecutorRequest {
            owner_user_id: "user-1".to_string(),
            project_id: "project-2".to_string(),
            plugin_release_snapshot: release_snapshot(),
        },
        FrozenMcpExecutorRequest {
            owner_user_id: "user-1".to_string(),
            project_id: "project-1".to_string(),
            plugin_release_snapshot: json!({"plugins": []}),
        },
    ] {
        assert!(runtime
            .resolve(&request, CancellationToken::new())
            .await
            .is_err());
    }
}

#[test]
fn registration_rejects_schema_drift_and_unfrozen_executor_tools() {
    let runtime = RegisteredLocalCapabilityRuntime::new();
    let extra_executor = executor(true);
    assert!(runtime.register(bundle(extra_executor)).is_err());

    let exact_executor = executor(false);
    let mut drifted = bundle(exact_executor);
    drifted.execution_tools[0].schema["description"] = Value::String("changed".to_string());
    assert!(runtime.register(drifted).is_err());
}
