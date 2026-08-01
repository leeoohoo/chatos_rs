// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use chatos_ai_runtime::ToolExecutor;
use chatos_mcp_runtime::ToolCallContext;
use serde_json::{json, Value};

use super::tool_executor::{approval_public_tool_name, rename_tool_call, APPROVAL_AGGREGATED_TOOL};
use crate::approval::decision_tool::APPROVAL_DECISION_TOOL;

#[test]
fn execution_mode_defaults_to_shadow_and_gateway_is_explicit() {
    assert_eq!(
        McpManagementExecutionMode::from_value(None),
        McpManagementExecutionMode::Shadow
    );
    assert_eq!(
        McpManagementExecutionMode::from_value(Some("gateway")),
        McpManagementExecutionMode::Gateway
    );
    assert_eq!(
        McpManagementExecutionMode::from_value(Some("off")),
        McpManagementExecutionMode::Off
    );
}

#[test]
fn legacy_approval_tools_keep_public_names_and_extra_mcps_stay_namespaced() {
    assert_eq!(
        approval_public_tool_name("code_maintainer_read_read_file_raw").as_deref(),
        Some("read_file_raw")
    );
    assert!(approval_public_tool_name("code_maintainer_read_read_file").is_none());
    assert_eq!(
        approval_public_tool_name(APPROVAL_AGGREGATED_TOOL).as_deref(),
        Some(APPROVAL_DECISION_TOOL)
    );
    assert_eq!(
        approval_public_tool_name("custom_security_scan").as_deref(),
        Some("custom_security_scan")
    );
}

#[test]
fn tool_call_translation_preserves_shape_and_arguments() {
    let translated = rename_tool_call(
        &json!({
            "id": "call-1",
            "function": {
                "name": "approval_decision",
                "arguments": "{\"decision\":\"deny\",\"reason\":\"unsafe\"}"
            }
        }),
        APPROVAL_AGGREGATED_TOOL,
    );
    assert_eq!(
        translated.pointer("/function/name").and_then(Value::as_str),
        Some(APPROVAL_AGGREGATED_TOOL)
    );
    assert_eq!(
        translated
            .pointer("/function/arguments")
            .and_then(Value::as_str),
        Some("{\"decision\":\"deny\",\"reason\":\"unsafe\"}")
    );
}

#[test]
fn relative_runtime_facade_url_is_resolved_against_the_cloud_service() {
    assert_eq!(
        resolve_mcp_server_url(
            "/api/local-connectors/mcp-management/runtime/mcp",
            "https://cloud.example.com/api"
        )
        .expect("same-origin facade URL"),
        "https://cloud.example.com/api/local-connectors/mcp-management/runtime/mcp"
    );
    assert!(
        resolve_mcp_server_url("//other.example.com/mcp", "https://cloud.example.com").is_err()
    );
}

#[tokio::test]
async fn gateway_executor_routes_decision_through_mcp_and_records_local_sink() {
    async fn handler(headers: HeaderMap, Json(request): Json<Value>) -> Json<Value> {
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer runtime-token")
        );
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let result = match request.get("method").and_then(Value::as_str) {
            Some("tools/list") => json!({
                "tools": [
                    {
                        "name": "code_maintainer_read_read_file_raw",
                        "description": "read",
                        "inputSchema": {"type": "object"}
                    },
                    {
                        "name": APPROVAL_AGGREGATED_TOOL,
                        "description": "decide",
                        "inputSchema": {"type": "object"}
                    },
                    {
                        "name": "custom_security_scan",
                        "description": "scan",
                        "inputSchema": {"type": "object"}
                    }
                ]
            }),
            Some("tools/call") => {
                assert_eq!(
                    request.pointer("/params/name").and_then(Value::as_str),
                    Some(APPROVAL_AGGREGATED_TOOL)
                );
                json!({
                    "content": [{"type": "text", "text": "denied"}],
                    "_structured_result": {
                        "decision": "deny",
                        "reason": "unsafe command",
                        "remember_allow": false
                    }
                })
            }
            _ => json!({}),
        };
        Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake MCP");
    let address = listener.local_addr().expect("fake MCP address");
    let server = tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/mcp", post(handler)))
            .await
            .expect("serve fake MCP");
    });
    let mcp = McpExecutor::builder()
        .with_http_server(
            McpHttpServer::new(GATEWAY_SERVER_NAME, format!("http://{address}/mcp"))
                .with_headers(HashMap::from([(
                    "authorization".to_string(),
                    "Bearer runtime-token".to_string(),
                )]))
                .with_preserved_tool_names()
                .with_fail_on_unavailable(),
        )
        .build_initialized()
        .await
        .expect("initialize fake MCP");
    let decision = Arc::new(Mutex::new(None));
    let executor = ApprovalMcpGatewayToolExecutor::new(mcp, decision.clone())
        .expect("approval gateway executor");
    let names = executor
        .available_tools()
        .into_iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "read_file_raw".to_string(),
            APPROVAL_DECISION_TOOL.to_string(),
            "custom_security_scan".to_string()
        ]
    );
    let results = executor
        .execute_tools_stream(
            &[json!({
                "id": "call-1",
                "function": {
                    "name": APPROVAL_DECISION_TOOL,
                    "arguments": "{\"decision\":\"deny\",\"reason\":\"unsafe command\"}"
                }
            })],
            ToolCallContext::default(),
            None,
        )
        .await;
    assert_eq!(results.len(), 1);
    assert!(results[0].success);
    assert_eq!(results[0].name, APPROVAL_DECISION_TOOL);
    assert_eq!(
        decision.lock().expect("decision lock").as_ref(),
        Some(&ApprovalToolDecision {
            decision: "deny".to_string(),
            reason: "unsafe command".to_string(),
        })
    );
    server.abort();
}
