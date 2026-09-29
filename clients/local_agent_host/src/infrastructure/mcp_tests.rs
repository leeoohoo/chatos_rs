// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::{LocalToolExecutor, LocalToolRegistry};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentToolApprovalStatus, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LocalAgentToolStatus, LocalPluginInstallationRecord, LocalPluginInstallationSpec,
};
use serde_json::{json, Value};
use std::sync::Arc;

fn invocation(tool_name: &str) -> LocalAgentToolInvocationRecord {
    LocalAgentToolInvocationRecord {
        invocation_id: "invocation-1".to_string(),
        run_id: "run-1".to_string(),
        batch_id: "batch-1".to_string(),
        call_id: "call-1".to_string(),
        tool_name: tool_name.to_string(),
        arguments: json!({"path": "README.md"}),
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
        claim_until_unix_ms: Some(10_000),
        created_at_unix_ms: 1,
        updated_at_unix_ms: 2,
    }
}

struct FakeClient {
    response: Value,
}

struct FakeSecrets;

#[async_trait]
impl LocalPluginSecretResolver for FakeSecrets {
    async fn resolve_secret(
        &self,
        owner_user_id: &str,
        secret_ref: &str,
    ) -> Result<String, String> {
        assert_eq!(owner_user_id, "user-1");
        assert_eq!(secret_ref, "keychain:plugin/token");
        Ok("transient-secret".to_string())
    }
}

#[async_trait]
impl LocalMcpClient for FakeClient {
    async fn call_tool(&self, _name: &str, _arguments: Value) -> Result<Value, McpCallError> {
        Ok(self.response.clone())
    }
}

#[tokio::test]
async fn known_mcp_tool_error_is_a_durable_failed_result() {
    let executor = LocalMcpToolExecutor {
        session: Arc::new(FakeClient {
            response: json!({
                "isError": true,
                "content": [{"type": "text", "text": "file not found"}]
            }),
        }),
        public_name: "files__read".to_string(),
        original_name: "read".to_string(),
    };
    let outcome = executor
        .execute_tool(&invocation("files__read"))
        .await
        .expect("known result");
    assert!(matches!(
        outcome,
        LocalAgentToolOutcome::Failed { error, .. } if error == "file not found"
    ));
}

#[test]
fn definitions_are_prefixed_filtered_and_model_ready() {
    let mut config = LocalMcpServerConfig::new("filesystem", "/plugin/server");
    config.allowed_tools = Some(vec!["read_file".to_string()]);
    let definitions = decode_tool_definitions(
        &config,
        vec![
            json!({
                "name": "read_file",
                "description": "Read a file",
                "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}}
            }),
            json!({"name": "write_file"}),
        ],
    )
    .expect("definitions");
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].public_name, "filesystem__read_file");
    assert_eq!(definitions[0].model_tool()["name"], "filesystem__read_file");
}

#[tokio::test]
async fn installation_resolves_secret_references_only_for_process_launch() {
    let record = LocalPluginInstallationRecord {
        spec: LocalPluginInstallationSpec {
            installation_id: "install-1".to_string(),
            owner_user_id: "user-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            release_digest: "sha256:abc".to_string(),
            component_id: "component-1".to_string(),
            component_revision: "revision-1".to_string(),
            server_id: "files".to_string(),
            executable_path: "/plugin/server".to_string(),
            args: vec!["--stdio".to_string()],
            working_directory: Some("/plugin".to_string()),
            environment_secret_refs: std::collections::BTreeMap::from([(
                "API_TOKEN".to_string(),
                "keychain:plugin/token".to_string(),
            )]),
            tool_prefix: None,
            allowed_tools: None,
            enabled: true,
        },
        version: 1,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    let config = LocalMcpServerConfig::from_installation(&record, &FakeSecrets)
        .await
        .expect("config");
    assert_eq!(config.environment["API_TOKEN"], "transient-secret");
    let persisted = serde_json::to_string(&record).expect("serialize");
    assert!(!persisted.contains("transient-secret"));
}

#[cfg(unix)]
#[tokio::test]
async fn stdio_session_initializes_lists_and_calls_locally() {
    let script = r#"
read initialize
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}}}'
read initialized
read list_tools
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"read","description":"Read","inputSchema":{"type":"object"}}]}}'
read call_tool
printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"local result"}]}}'
"#;
    let mut config = LocalMcpServerConfig::new("fixture", "/bin/sh");
    config.args = vec!["-c".to_string(), script.to_string()];
    config.inherit_environment = true;
    let tools = LocalMcpStdioSession::connect(config)
        .await
        .expect("local session");
    assert_eq!(tools.definitions()[0].public_name, "fixture__read");
    let mut registry = LocalToolRegistry::new();
    tools.register_into(&mut registry).expect("register");
    let executor = registry.executor_for("fixture__read").expect("executor");
    let outcome = executor
        .execute_tool(&invocation("fixture__read"))
        .await
        .expect("call");
    assert!(matches!(outcome, LocalAgentToolOutcome::Succeeded { .. }));
}
