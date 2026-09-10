// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn obsolete_profile_is_rejected_before_tool_dispatch() {
    let (service, task_service, _) = test_mcp_service().await;
    let user = agent_user("owner-a");
    for profile in ["chatos_plan", " CHATOS_PLAN ", "unknown"] {
        for method in ["tools/list", "tools/call"] {
            let response = service
                .handle_jsonrpc(
                    super::super::JsonRpcRequest {
                        jsonrpc: Some("2.0".to_string()),
                        id: Some(json!("removed-profile")),
                        method: method.to_string(),
                        params: json!({"name": "create_task", "arguments": {}}),
                    },
                    user.clone(),
                    McpRequestContext {
                        task_profile: Some(profile.to_string()),
                        ..Default::default()
                    },
                )
                .await;
            let error = response.error.expect("obsolete profile must not fall back");
            assert_eq!(error.code, -32602);
            assert!(error.message.contains("unknown task_profile"));
        }
    }
    let _ = task_service;
}
