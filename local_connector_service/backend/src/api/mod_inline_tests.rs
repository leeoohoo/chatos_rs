#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        active_remote_connection_workspace, is_allowed_model_config_proxy_request,
        is_local_sandbox_mcp_path, is_plugin_hook_dispatch, mcp_relay_timeout,
        plugin_relay_timeout, STANDARD_MCP_RELAY_TIMEOUT,
    };
    use crate::models::{
        LocalConnectorWorkspace, WORKSPACE_STATUS_ACTIVE, WORKSPACE_STATUS_DISABLED,
    };
    use axum::http::Method;
    use serde_json::json;

    #[test]
    fn only_hook_dispatch_uses_the_extended_interactive_relay_window() {
        assert!(is_plugin_hook_dispatch(
            "execute",
            &json!({"operation": "dispatch_hook_event"})
        ));
        assert!(!is_plugin_hook_dispatch(
            "execute",
            &json!({"operation": "mcp_tools_call"})
        ));
        assert!(!is_plugin_hook_dispatch(
            "prepare",
            &json!({"operation": "dispatch_hook_event"})
        ));
    }

    #[test]
    fn plugin_prepare_and_execute_use_the_two_hour_platform_budget() {
        let control_plane_timeout = Duration::from_secs(30);
        let hook_timeout = Duration::from_secs(5 * 60 + 15);
        for (action, body) in [
            ("prepare", json!({})),
            ("execute", json!({"operation": "mcp_tools_call"})),
            ("execute", json!({"operation": "command_invoke"})),
            ("execute", json!({"operation": "agent_apply"})),
            ("execute", json!({"operation": "dispatch_hook_event"})),
        ] {
            assert_eq!(
                plugin_relay_timeout(control_plane_timeout, hook_timeout, action, &body),
                STANDARD_MCP_RELAY_TIMEOUT
            );
        }
    }

    #[test]
    fn plugin_cancel_keeps_the_short_control_plane_timeout() {
        assert_eq!(
            plugin_relay_timeout(
                Duration::from_secs(30),
                Duration::from_secs(5 * 60 + 15),
                "cancel",
                &json!({})
            ),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn only_concrete_sandbox_tool_calls_require_the_mcp_management_caller() {
        assert!(is_local_sandbox_mcp_path("/api/sandboxes/sandbox-1/mcp"));
        assert!(!is_local_sandbox_mcp_path("/api/sandboxes/leases"));
        assert!(!is_local_sandbox_mcp_path("/api/local/sandbox/images/mcp"));
    }

    #[test]
    fn model_provider_crud_and_refresh_are_available_to_native_clients() {
        assert!(is_allowed_model_config_proxy_request(
            &Method::GET,
            "/api/model-providers"
        ));
        assert!(is_allowed_model_config_proxy_request(
            &Method::POST,
            "/api/model-providers"
        ));
        assert!(is_allowed_model_config_proxy_request(
            &Method::PATCH,
            "/api/model-providers/provider-1"
        ));
        assert!(is_allowed_model_config_proxy_request(
            &Method::POST,
            "/api/model-providers/provider-1/refresh"
        ));
        assert!(is_allowed_model_config_proxy_request(
            &Method::DELETE,
            "/api/model-providers/provider-1"
        ));
        assert!(!is_allowed_model_config_proxy_request(
            &Method::PUT,
            "/api/model-providers/provider-1"
        ));
    }

    #[test]
    fn ordinary_mcp_relay_uses_the_two_hour_platform_budget() {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "command-1",
            "method": "tools/call",
            "params": {
                "name": "execute_command",
                "arguments": {"command": "npm install", "background": false}
            }
        });
        for seconds in [30, 90, 105, 180] {
            assert_eq!(
                mcp_relay_timeout(Duration::from_secs(seconds), &body),
                STANDARD_MCP_RELAY_TIMEOUT
            );
        }
    }

    #[test]
    fn remote_connection_alias_uses_only_an_active_workspace() {
        let workspace = |id: &str, status: &str| LocalConnectorWorkspace {
            id: id.to_string(),
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            display_name: id.to_string(),
            local_path_alias: "/tmp".to_string(),
            local_path_fingerprint: id.to_string(),
            capabilities: Vec::new(),
            status: status.to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let workspaces = vec![
            workspace("disabled", WORKSPACE_STATUS_DISABLED),
            workspace("active", WORKSPACE_STATUS_ACTIVE),
        ];
        assert_eq!(
            active_remote_connection_workspace(workspaces.as_slice()).map(|item| item.id.as_str()),
            Some("active")
        );
    }

    #[test]
    fn mcp_terminal_wait_relay_keeps_the_standard_two_hour_budget() {
        assert_eq!(
            mcp_relay_timeout(
                Duration::from_secs(30),
                &json!({
                    "method": "tools/call",
                    "params": {
                        "name": "terminal_controller_process_wait",
                        "arguments": {"timeout_ms": 600_000}
                    }
                })
            ),
            STANDARD_MCP_RELAY_TIMEOUT
        );
        assert_eq!(
            mcp_relay_timeout(
                Duration::from_secs(30),
                &json!({
                    "method": "tools/call",
                    "params": {
                        "name": "process",
                        "arguments": {"action": "wait", "timeout": 600}
                    }
                })
            ),
            STANDARD_MCP_RELAY_TIMEOUT
        );
    }
}
