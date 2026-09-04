// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_agent::SystemAgentKey;
use chatos_mcp_management_sdk::{
    McpProviderKind, McpRetryClass, ProjectExecutionContext, ResolvedMcpRoute,
    WorkspaceExecutionTarget, WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::{
    plugin_command_snapshot_sha256, plugin_skill_snapshot_sha256, skill_resource_manifest_sha256,
    PackagedSkillMetadata, PluginComponentDescriptor, PluginComponentKind, PluginPathRef,
    PluginSkillComponentSnapshot, RuntimeSkillResourceDescriptor, SkillActivationPolicy,
    SkillContextMode, SkillResourceKind, SkillRole, SKILL_RUNTIME_PROTOCOL_VERSION,
};
use serde_json::json;

use super::validation::{sha256_text, validate_command_snapshot};
use super::*;
use crate::runtime::{
    PluginLocalToolComponentBinding, PluginToolComponentRuntimeBinding, RuntimeSessionSnapshot,
};

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

fn command_binding() -> PluginToolComponentRuntimeBinding {
    PluginToolComponentRuntimeBinding {
        provider_ref: format!("plugin-tool-binding:{}", "d".repeat(64)),
        resource_id: "plugin_component_review".to_string(),
        plugin_id: "plugin-review".to_string(),
        release_id: "release-review-1".to_string(),
        version: "1.0.0".to_string(),
        artifact_sha256: "a".repeat(64),
        normalized_manifest_sha256: "b".repeat(64),
        component: PluginComponentDescriptor {
            component_key: "review".to_string(),
            kind: PluginComponentKind::Command,
            display_name: "Review".to_string(),
            runtime_kind: "command".to_string(),
            entrypoint: Some(PluginPathRef::new("./commands/review.md")),
            required: false,
            permissions: Vec::new(),
            metadata: BTreeMap::from([
                (
                    "description".to_string(),
                    json!("Review the current change"),
                ),
                ("argument_hint".to_string(), json!("[path]")),
                ("requires_confirmation".to_string(), json!(true)),
                ("target_agent".to_string(), json!(RUN_AGENT_KEY)),
                ("allowed_tools".to_string(), json!(["plugin_snapshot"])),
            ]),
        },
        component_content_sha256: "c".repeat(64),
        skill_snapshot: None,
        installation_device_id: Some("device-1".to_string()),
        permission_snapshot: vec!["workspace.read".to_string()],
        auth_connection_ids: Vec::new(),
        required: true,
        allow_writes: false,
        command_arguments: None,
    }
}

fn skill_binding_base() -> PluginToolComponentRuntimeBinding {
    PluginToolComponentRuntimeBinding {
        provider_ref: format!("plugin-tool-binding:{}", "f".repeat(64)),
        resource_id: "plugin_component_review_skill".to_string(),
        plugin_id: "plugin-review".to_string(),
        release_id: "release-review-1".to_string(),
        version: "1.0.0".to_string(),
        artifact_sha256: "a".repeat(64),
        normalized_manifest_sha256: "b".repeat(64),
        component: PluginComponentDescriptor {
            component_key: "review-skill".to_string(),
            kind: PluginComponentKind::SkillCollection,
            display_name: "Review Skill".to_string(),
            runtime_kind: "prompt".to_string(),
            entrypoint: Some(PluginPathRef::new("./skills/review/SKILL.md")),
            required: false,
            permissions: Vec::new(),
            metadata: BTreeMap::from([(
                "description".to_string(),
                json!("Review the current change"),
            )]),
        },
        component_content_sha256: "c".repeat(64),
        skill_snapshot: None,
        installation_device_id: Some("device-1".to_string()),
        permission_snapshot: vec!["workspace.read".to_string()],
        auth_connection_ids: Vec::new(),
        required: true,
        allow_writes: false,
        command_arguments: None,
    }
}

fn progressive_skill_binding() -> PluginToolComponentRuntimeBinding {
    let metadata = PackagedSkillMetadata {
        name: "review-skill".to_string(),
        description: "Review the current change with specialist guidance".to_string(),
        role: SkillRole::Leaf,
        activation_policy: SkillActivationPolicy::ModelOrUser,
        context_mode: SkillContextMode::Inline,
        required_skills: Vec::new(),
        related_skills: Vec::new(),
        max_output_chars: None,
        extra: BTreeMap::new(),
    };
    let resources = vec![RuntimeSkillResourceDescriptor {
        relative_path: "references/guide.md".to_string(),
        kind: SkillResourceKind::Reference,
        size_bytes: 22,
        sha256: sha256_text("# Guide\nUse evidence.\n"),
    }];
    let resource_manifest_sha256 = skill_resource_manifest_sha256(&resources).unwrap();
    let instructions_sha256 = sha256_text("progressive instructions");
    let snapshot_sha256 = plugin_skill_snapshot_sha256(
        "review-skill",
        "skills/review-skill/SKILL.md",
        &metadata,
        instructions_sha256.as_str(),
        resource_manifest_sha256.as_str(),
    )
    .unwrap();
    let mut binding = skill_binding_base();
    binding.component.entrypoint = Some(PluginPathRef::new("./skills/review-skill/SKILL.md"));
    binding.component_content_sha256 = snapshot_sha256.clone();
    binding.skill_snapshot = Some(PluginSkillComponentSnapshot {
        protocol_version: SKILL_RUNTIME_PROTOCOL_VERSION,
        skill_id: "review-skill".to_string(),
        relative_skill_path: "skills/review-skill/SKILL.md".to_string(),
        metadata,
        instructions_sha256,
        resource_manifest_sha256,
        resources,
        snapshot_sha256,
    });
    binding
}

fn command_snapshot(
    binding: &PluginToolComponentRuntimeBinding,
    arguments: Option<&str>,
    confirmation_approved: bool,
) -> Value {
    let prompt = "Review the current change and report concrete findings.";
    let arguments_sha256 = sha256_text(arguments.unwrap_or_default());
    let snapshot_sha256 = plugin_command_snapshot_sha256(
        binding.plugin_id.as_str(),
        binding.release_id.as_str(),
        binding.component.component_key.as_str(),
        binding.component.entrypoint.as_ref().unwrap().path.as_str(),
        Some("Review the current change"),
        Some("[path]"),
        true,
        Some(RUN_AGENT_KEY),
        &["plugin_snapshot".to_string()],
        binding.component_content_sha256.as_str(),
        prompt,
        arguments_sha256.as_str(),
    )
    .unwrap();
    json!({
        "plugin_id": binding.plugin_id,
        "release_id": binding.release_id,
        "version": binding.version,
        "artifact_sha256": binding.artifact_sha256,
        "component_key": binding.component.component_key,
        "command_name": binding.component.component_key,
        "relative_source_path": binding.component.entrypoint.as_ref().unwrap().path,
        "description": "Review the current change",
        "argument_hint": "[path]",
        "requires_confirmation": true,
        "target_agent": RUN_AGENT_KEY,
        "allowed_tools": ["plugin_snapshot"],
        "confirmation_approved": confirmation_approved,
        "content_sha256": binding.component_content_sha256,
        "arguments_present": arguments.is_some(),
        "arguments_sha256": arguments_sha256,
        "snapshot_sha256": snapshot_sha256,
        "prompt": prompt,
    })
}

fn route(binding: &PluginToolComponentRuntimeBinding) -> ResolvedMcpRoute {
    ResolvedMcpRoute {
        resource_id: binding.resource_id.clone(),
        server_name: "plugin_review_review".to_string(),
        provider_kind: McpProviderKind::PluginLocal,
        provider_ref: Some(binding.provider_ref.clone()),
        tool_namespace: "plugin_review_review".to_string(),
        allow_writes: binding.allow_writes,
        retry_class: McpRetryClass::NoRetry,
        cancel_supported: false,
        reason: "test".to_string(),
    }
}

fn local_context() -> ProjectExecutionContext {
    ProjectExecutionContext {
        project_id: Some("project-1".to_string()),
        owner_user_id: "user-1".to_string(),
        workspace_provider: WorkspaceProviderKind::LocalConnector,
        workspace: Some(WorkspaceExecutionTarget {
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            relative_root: Some("projects/space-station".to_string()),
        }),
        revision: "project-revision".to_string(),
    }
}

fn snapshot(
    binding: &PluginToolComponentRuntimeBinding,
    local: PluginLocalToolComponentBinding,
    route: ResolvedMcpRoute,
    expires_at_unix: i64,
) -> RuntimeSessionSnapshot {
    RuntimeSessionSnapshot {
        session_id: "session-1".to_string(),
        caller_service: "task-runner".to_string(),
        trace_id: "00000000-0000-4000-8000-000000000001".to_string(),
        tenant_id: "tenant-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_role: None,
        agent_key: RUN_AGENT_KEY.to_string(),
        task_profile: Some("default".to_string()),
        project_id: Some("project-1".to_string()),
        device_id: None,
        run_id: Some("run-1".to_string()),
        execution_group_id: None,
        execution_scope_generation: Some(1),
        turn_id: None,
        task_id: Some("task-1".to_string()),
        task_title: Some("Task one".to_string()),
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        default_remote_connection_id: None,
        remote_connection_route: None,
        tool_result_max_chars: None,
        expected_project_task_ids: Vec::new(),
        workspace_route: None,
        project_context: local_context(),
        policy_revision: "policy-1".to_string(),
        route_revision: "route-1".to_string(),
        routes: vec![route],
        tools: Vec::new(),
        effective_mcp_ids: Vec::new(),
        provider_skills_prompt: None,
        plugin_instruction_items: Vec::new(),
        plugin_mcp_bindings: HashMap::new(),
        plugin_local_bindings: HashMap::new(),
        plugin_tool_component_bindings: HashMap::from([(
            binding.resource_id.clone(),
            binding.clone(),
        )]),
        plugin_local_tool_component_bindings: HashMap::from([(binding.resource_id.clone(), local)]),
        local_connector_mcp_bindings: HashMap::new(),
        expires_at: "2099-01-01T00:00:00Z".to_string(),
        expires_at_unix,
    }
}

async fn start_local_connector(
    secret: &'static str,
    binding: PluginToolComponentRuntimeBinding,
) -> (
    String,
    Arc<Mutex<Vec<(String, Value)>>>,
    tokio::task::JoinHandle<()>,
) {
    #[derive(Clone)]
    struct TestState {
        secret: &'static str,
        binding: PluginToolComponentRuntimeBinding,
        requests: Arc<Mutex<Vec<(String, Value)>>>,
    }

    async fn handler(
        State(state): State<TestState>,
        Path(action): Path<String>,
        Query(query): Query<HashMap<String, String>>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        assert_eq!(
            query.get("workspace_id").map(String::as_str),
            Some("workspace-1")
        );
        if action != "cancel" {
            assert_eq!(
                query.get("cwd").map(String::as_str),
                Some("projects/space-station")
            );
        }
        assert_eq!(
            headers
                .get("x-local-connector-owner-user-id")
                .and_then(|value| value.to_str().ok()),
            Some("user-1")
        );
        let token = headers
            .get("x-local-connector-internal-token")
            .and_then(|value| value.to_str().ok())
            .unwrap();
        let claims = chatos_service_runtime::verify_internal_service_token(
            token,
            state.secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            PLUGIN_RELAY_SCOPE,
        )
        .unwrap();
        assert_eq!(claims.owner_user_id.as_deref(), Some("user-1"));
        state
            .requests
            .lock()
            .unwrap()
            .push((action.clone(), body.clone()));
        match action.as_str() {
            "prepare" => Json(json!({
                "run_id": "session-1",
                "plugin_id": state.binding.plugin_id,
                "release_id": state.binding.release_id,
                "version": state.binding.version,
                "artifact_sha256": state.binding.artifact_sha256,
                "component_key": state.binding.component.component_key,
                "commands": [command_snapshot(
                    &state.binding,
                    state.binding.command_arguments.as_deref(),
                    false,
                )],
                "operations": [COMMAND_INVOKE_OPERATION],
                "adapter_session_id": "adapter-1",
                "session_sha256": "e".repeat(64),
                "expires_at": chrono::Utc::now().timestamp() + 7200
            })),
            "execute" => {
                assert_eq!(body.get("arguments"), Some(&json!("src/lib.rs")));
                Json(json!({
                    "plugin_id": state.binding.plugin_id,
                    "release_id": state.binding.release_id,
                    "version": state.binding.version,
                    "artifact_sha256": state.binding.artifact_sha256,
                    "component_key": state.binding.component.component_key,
                    "adapter_session_id": "adapter-1",
                    "operation": COMMAND_INVOKE_OPERATION,
                    "result": {
                        "command": command_snapshot(&state.binding, Some("src/lib.rs"), true)
                    }
                }))
            }
            "cancel" => Json(json!({"cancelled": true})),
            _ => panic!("unexpected Plugin component action"),
        }
    }

    let requests = Arc::new(Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route(
            "/api/local-connectors/relay/device-1/plugins/{action}",
            post(handler),
        )
        .with_state(TestState {
            secret,
            binding,
            requests: requests.clone(),
        });
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), requests, handle)
}

#[tokio::test]
async fn local_command_is_approved_once_during_prepare_and_reused_without_duplicate_execution() {
    const SECRET: &str = "plugin-component-local-test-secret";
    let mut immutable = command_binding();
    immutable.command_arguments = Some("src/lib.rs".to_string());
    let (base_url, requests, server) = start_local_connector(SECRET, immutable.clone()).await;
    let provider = PluginComponentProvider::new(
        reqwest::Client::new(),
        base_url,
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
        Arc::new(SkillActivationAttestationService::new(SECRET).unwrap()),
    )
    .unwrap();
    let mut routes = vec![route(&immutable)];
    let expires_at_unix = chrono::Utc::now().timestamp() + 600;
    let (local_bindings, tool_snapshots) = provider
        .prepare_routes(
            &HashMap::from([(immutable.resource_id.clone(), immutable.clone())]),
            routes.as_mut_slice(),
            &local_context(),
            "session-1",
            "user-1",
            expires_at_unix,
        )
        .await;
    assert!(!routes[0].cancel_supported);
    assert_eq!(tool_snapshots[&immutable.resource_id][0]["name"], "invoke");
    let local = local_bindings[&immutable.resource_id].clone();
    assert_eq!(local.operation, COMMAND_INVOKE_OPERATION);
    assert!(local.static_result.is_some());
    let prepared_request = requests.lock().unwrap()[0].1.clone();
    assert_eq!(prepared_request.get("catalog_only"), Some(&json!(true)));
    assert_eq!(
        prepared_request.get("arguments"),
        Some(&json!("src/lib.rs"))
    );
    assert_eq!(requests.lock().unwrap()[1].0, "execute");

    let runtime = snapshot(&immutable, local, routes[0].clone(), expires_at_unix);
    let outcome = provider
        .call_tool(
            &runtime,
            &routes[0],
            COMMAND_TOOL_NAME,
            json!({"arguments": " src/lib.rs "}),
            "invocation-command-1",
        )
        .await
        .unwrap();
    let text = outcome
        .result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .unwrap();
    assert!(text.contains("Arguments for this invocation:\nsrc/lib.rs"));
    assert_eq!(requests.lock().unwrap().len(), 2);
    let error = provider
        .call_tool(
            &runtime,
            &routes[0],
            COMMAND_TOOL_NAME,
            json!({"arguments": "README.md"}),
            "invocation-command-2",
        )
        .await
        .expect_err("arguments outside the Runtime Session selection must be rejected");
    assert!(error.message.contains("Runtime Session selection"));
    assert_eq!(requests.lock().unwrap().len(), 2);

    provider.close_session(&runtime).await;
    assert_eq!(requests.lock().unwrap()[2].0, "cancel");
    server.abort();
}

#[tokio::test]
async fn progressive_local_skill_prepare_returns_catalog_without_preloading_instructions() {
    const SECRET: &str = "plugin-component-progressive-skill-test-secret";
    let immutable = progressive_skill_binding();
    let expected = immutable.skill_snapshot.clone().unwrap();
    let requests = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let captured_requests = requests.clone();
    let server_binding = immutable.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/api/local-connectors/relay/device-1/plugins/{action}",
        post(move |Path(action): Path<String>, Json(body): Json<Value>| {
            let requests = captured_requests.clone();
            let binding = server_binding.clone();
            let expected = expected.clone();
            async move {
                requests.lock().unwrap().push((action.clone(), body.clone()));
                match action.as_str() {
                    "prepare" => Json(json!({
                        "run_id": "session-1",
                        "plugin_id": binding.plugin_id,
                        "release_id": binding.release_id,
                        "version": binding.version,
                        "artifact_sha256": binding.artifact_sha256,
                        "component_key": binding.component.component_key,
                        "skills": [expected],
                        "operations": [SKILL_ACTIVATE_OPERATION, SKILL_READ_RESOURCE_OPERATION],
                        "adapter_session_id": "adapter-progressive-skill-1",
                        "session_sha256": "e".repeat(64),
                        "expires_at": chrono::Utc::now().timestamp() + 7200
                    })),
                    "execute" if body["operation"] == SKILL_READ_RESOURCE_OPERATION => Json(json!({
                        "plugin_id": binding.plugin_id,
                        "release_id": binding.release_id,
                        "version": binding.version,
                        "artifact_sha256": binding.artifact_sha256,
                        "component_key": binding.component.component_key,
                        "adapter_session_id": "adapter-progressive-skill-1",
                        "invocation_id": body["invocation_id"],
                        "operation": SKILL_READ_RESOURCE_OPERATION,
                        "result": {
                            "skill_id": "review-skill",
                            "relative_path": "references/guide.md",
                            "sha256": binding.skill_snapshot.as_ref().unwrap().resources[0].sha256,
                            "content": "# Guide\n",
                            "offset": 0,
                            "next_offset": 8,
                            "truncated": true
                        }
                    })),
                    "execute" => Json(json!({
                        "plugin_id": binding.plugin_id,
                        "release_id": binding.release_id,
                        "version": binding.version,
                        "artifact_sha256": binding.artifact_sha256,
                        "component_key": binding.component.component_key,
                        "adapter_session_id": "adapter-progressive-skill-1",
                        "invocation_id": body["invocation_id"],
                        "operation": SKILL_ACTIVATE_OPERATION,
                        "result": {
                            "skill_id": "review-skill",
                            "instructions": "progressive instructions",
                            "instructions_sha256": binding.skill_snapshot.as_ref().unwrap().instructions_sha256,
                            "resource_manifest_sha256": binding.skill_snapshot.as_ref().unwrap().resource_manifest_sha256,
                            "snapshot_sha256": binding.skill_snapshot.as_ref().unwrap().snapshot_sha256,
                            "resources": binding.skill_snapshot.as_ref().unwrap().resources
                        }
                    })),
                    "cancel" => Json(json!({"cancelled": true})),
                    _ => panic!("unexpected Plugin component action"),
                }
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let provider = PluginComponentProvider::new(
        reqwest::Client::new(),
        format!("http://{address}"),
        Duration::from_secs(5),
        Some(SECRET.to_string()),
        1024 * 1024,
        Arc::new(SkillActivationAttestationService::new(SECRET).unwrap()),
    )
    .unwrap();
    let mut routes = vec![route(&immutable)];
    let expires_at_unix = chrono::Utc::now().timestamp() + 600;
    let (local_bindings, tool_snapshots) = provider
        .prepare_routes(
            &HashMap::from([(immutable.resource_id.clone(), immutable.clone())]),
            routes.as_mut_slice(),
            &local_context(),
            "session-1",
            "user-1",
            expires_at_unix,
        )
        .await;
    let prepared_request = requests.lock().unwrap()[0].1.clone();
    assert_eq!(prepared_request["skill_runtime_protocol"], 2);
    assert_eq!(
        prepared_request["skill_snapshot"],
        json!(immutable.skill_snapshot)
    );
    assert_eq!(
        tool_snapshots[&immutable.resource_id][0]["name"],
        SKILL_ACTIVATE_TOOL_NAME
    );
    let local = local_bindings[&immutable.resource_id].clone();
    assert_eq!(local.operation, SKILL_ACTIVATE_OPERATION);
    assert!(local.instruction_items.is_empty());
    assert!(local.static_result.is_none());

    let runtime = snapshot(&immutable, local, routes[0].clone(), expires_at_unix);
    let outcome = provider
        .call_tool(
            &runtime,
            &routes[0],
            SKILL_ACTIVATE_TOOL_NAME,
            json!({"skill_ref": skill_ref(&local_bindings[&immutable.resource_id])}),
            "invocation-skill-activate-1",
        )
        .await
        .unwrap();
    assert!(outcome.result["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("progressive instructions"));
    assert!(outcome.result["structuredContent"]["activation_evidence"]
        .as_str()
        .is_some_and(|value| value.split('.').count() == 3));
    assert_eq!(
        requests.lock().unwrap()[1].1["invocation_id"],
        "invocation-skill-activate-1"
    );
    let activation_ref = outcome.result["structuredContent"]["activation_ref"]
        .as_str()
        .unwrap();
    let activation_evidence = outcome.result["structuredContent"]["activation_evidence"]
        .as_str()
        .unwrap();
    assert!(outcome.result["content"][0]["text"]
        .as_str()
        .is_some_and(|text| text.contains(activation_evidence)));
    let listed = provider
        .call_tool(
            &runtime,
            &routes[0],
            SKILL_LIST_RESOURCES_TOOL_NAME,
            json!({
                "activation_ref": activation_ref,
                "activation_evidence": activation_evidence
            }),
            "invocation-skill-list-1",
        )
        .await
        .unwrap();
    assert_eq!(
        listed.result["structuredContent"]["resources"][0]["relative_path"],
        "references/guide.md"
    );
    let read = provider
        .call_tool(
            &runtime,
            &routes[0],
            SKILL_READ_RESOURCE_TOOL_NAME,
            json!({
                "activation_ref": activation_ref,
                "activation_evidence": activation_evidence,
                "relative_path": "references/guide.md",
                "offset": 0,
                "max_chars": 8
            }),
            "invocation-skill-read-1",
        )
        .await
        .unwrap();
    assert_eq!(read.result["content"][0]["text"], "# Guide\n");
    let resource_request = requests.lock().unwrap()[2].1.clone();
    assert_eq!(resource_request["invocation_id"], "invocation-skill-read-1");
    assert!(resource_request["arguments"]
        .get("activation_evidence")
        .is_none());
    assert_eq!(requests.lock().unwrap()[1].0, "execute");
    server.abort();
}

#[test]
fn command_snapshot_validation_binds_argument_presence_and_hash() {
    let binding = command_binding();
    let command = command_snapshot(&binding, Some("src/lib.rs"), true);
    validate_command_snapshot(&binding, &command, Some("src/lib.rs"), true).unwrap();

    let mut wrong_presence = command.clone();
    wrong_presence["arguments_present"] = json!(false);
    assert!(
        validate_command_snapshot(&binding, &wrong_presence, Some("src/lib.rs"), true).is_err()
    );
    assert!(validate_command_snapshot(&binding, &command, Some("README.md"), true).is_err());
}
