#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::HeaderValue;

    use super::*;

    const MCP_SECRET: &str = "a-long-mcp-management-local-connector-secret";
    const CHATOS_SECRET: &str = "a-long-chatos-local-connector-secret";
    const REMOVED_TASK_RUNNER: &str = "task-runner";
    const REMOVED_TASK_RUNNER_SECRET: &str = "a-long-task-runner-local-connector-secret";

    #[test]
    fn project_context_authorization_is_owner_scope_caller_and_method_bound() {
        let mut config = test_config();
        config
            .internal_api_secrets
            .insert(MCP_MANAGEMENT_CALLER.into(), MCP_SECRET.into());
        config.internal_api_secrets.insert(
            REMOVED_TASK_RUNNER.into(),
            REMOVED_TASK_RUNNER_SECRET.into(),
        );
        let path = "/api/local-connectors/project-context/authorize";
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            MCP_SECRET,
            MCP_MANAGEMENT_CALLER,
            TOKEN_AUDIENCE,
            PROJECT_CONTEXT_AUTHORIZE_SCOPE,
            60,
            "user-1",
        )
        .unwrap();
        let headers = signed_headers(MCP_MANAGEMENT_CALLER, &token);
        let (user, identity) =
            internal_service_auth_from_request(&config, &headers, &Method::POST, path)
                .unwrap()
                .unwrap();
        assert_eq!(user.effective_owner_user_id(), "user-1");
        assert_eq!(identity.scope, PROJECT_CONTEXT_AUTHORIZE_SCOPE);
        assert!(internal_service_auth_from_request(&config, &headers, &Method::GET, path).is_err());
        assert!(internal_service_auth_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/mcp"
        )
        .is_err());
        let wrong_owner = signed_headers_for_owner(MCP_MANAGEMENT_CALLER, &token, "user-2");
        assert!(
            internal_service_auth_from_request(&config, &wrong_owner, &Method::POST, path).is_err()
        );
        let no_owner = chatos_service_runtime::issue_internal_service_token(
            MCP_SECRET,
            MCP_MANAGEMENT_CALLER,
            TOKEN_AUDIENCE,
            PROJECT_CONTEXT_AUTHORIZE_SCOPE,
            60,
        )
        .unwrap();
        assert!(internal_service_auth_from_request(
            &config,
            &signed_headers(MCP_MANAGEMENT_CALLER, &no_owner),
            &Method::POST,
            path
        )
        .is_err());
        let direct_task = chatos_service_runtime::issue_internal_service_token_for_owner(
            REMOVED_TASK_RUNNER_SECRET,
            REMOVED_TASK_RUNNER,
            TOKEN_AUDIENCE,
            PROJECT_CONTEXT_AUTHORIZE_SCOPE,
            60,
            "user-1",
        )
        .unwrap();
        assert!(internal_service_auth_from_request(
            &config,
            &signed_headers(REMOVED_TASK_RUNNER, &direct_task),
            &Method::POST,
            path
        )
        .is_err());
    }

    #[test]
    fn mcp_management_tokens_are_bound_to_caller_scope_owner_and_path() {
        let mut config = test_config();
        config
            .internal_api_secrets
            .insert(MCP_MANAGEMENT_CALLER.to_string(), MCP_SECRET.to_string());
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            MCP_SECRET,
            MCP_MANAGEMENT_CALLER,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
            "user-1",
        )
        .expect("issue MCP Management token");
        let headers = signed_headers(MCP_MANAGEMENT_CALLER, token.as_str());
        let user = internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/mcp",
        )
        .expect("matching MCP relay request")
        .expect("service user");
        assert_eq!(user.user_id, "service:mcp-management-service:user-1");
        let (_user, identity) = internal_service_auth_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/mcp",
        )
        .expect("matching signed request")
        .expect("internal identity");
        assert_eq!(identity.caller_service, MCP_MANAGEMENT_CALLER);
        assert_eq!(identity.scope, MCP_RELAY_SCOPE);
        assert_eq!(identity.owner_user_id, "user-1");
        uuid::Uuid::parse_str(identity.trace_id.as_str()).expect("valid trace id");

        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::GET,
            "/api/local-connectors/devices",
        )
        .is_err());
    }

    #[test]
    fn mcp_management_owns_mcp_skill_plugin_terminal_and_runtime_sandbox_relays() {
        let mut config = test_config();
        config
            .internal_api_secrets
            .insert(MCP_MANAGEMENT_CALLER.to_string(), MCP_SECRET.to_string());

        for (scope, method, path) in [
            (
                MCP_RELAY_SCOPE,
                Method::POST,
                "/api/local-connectors/relay/device-1/mcp",
            ),
            (
                SKILL_RELAY_SCOPE,
                Method::POST,
                "/api/local-connectors/relay/device-1/skills/execute",
            ),
            (
                PLUGIN_RELAY_SCOPE,
                Method::POST,
                "/api/local-connectors/relay/device-1/plugins/prepare",
            ),
            (
                TERMINAL_RELAY_SCOPE,
                Method::POST,
                "/api/local-connectors/relay/device-1/terminal/exec",
            ),
            (
                SANDBOX_ROUTING_READ_SCOPE,
                Method::GET,
                "/api/local-connectors/sandbox-pairings",
            ),
            (
                SANDBOX_SERVICE_SCOPE,
                Method::POST,
                "/api/local-connectors/sandbox-facade/pairing-1/api/local/sandbox/images/mcp",
            ),
            (
                SANDBOX_SERVICE_SCOPE,
                Method::GET,
                "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/sandbox-1",
            ),
            (
                SANDBOX_SERVICE_SCOPE,
                Method::POST,
                "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/sandbox-1/mcp",
            ),
        ] {
            let token = chatos_service_runtime::issue_internal_service_token_for_owner(
                MCP_SECRET,
                MCP_MANAGEMENT_CALLER,
                TOKEN_AUDIENCE,
                scope,
                60,
                "user-1",
            )
            .expect("issue MCP Management token");
            let headers = signed_headers(MCP_MANAGEMENT_CALLER, token.as_str());
            let user = internal_service_user_from_request(&config, &headers, &method, path)
                .expect("authorized MCP Management request")
                .expect("service user");
            assert_eq!(user.owner_user_id.as_deref(), Some("user-1"));
        }

        let sandbox_token = chatos_service_runtime::issue_internal_service_token_for_owner(
            MCP_SECRET,
            MCP_MANAGEMENT_CALLER,
            TOKEN_AUDIENCE,
            SANDBOX_SERVICE_SCOPE,
            60,
            "user-1",
        )
        .expect("issue Sandbox service token");
        let sandbox_headers = signed_headers(MCP_MANAGEMENT_CALLER, sandbox_token.as_str());
        assert!(internal_service_user_from_request(
            &config,
            &sandbox_headers,
            &Method::POST,
            "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/leases",
        )
        .is_err());
    }

    #[test]
    fn task_runner_is_rejected_from_every_internal_execution_surface() {
        let mut config = test_config();
        config.internal_api_secrets.insert(
            REMOVED_TASK_RUNNER.to_string(),
            REMOVED_TASK_RUNNER_SECRET.to_string(),
        );
        for (scope, method, path) in [
            (
                MCP_RELAY_SCOPE,
                Method::POST,
                "/api/local-connectors/relay/device-1/mcp",
            ),
            (
                SKILL_RELAY_SCOPE,
                Method::POST,
                "/api/local-connectors/relay/device-1/skills/prepare",
            ),
            (
                PLUGIN_RELAY_SCOPE,
                Method::POST,
                "/api/local-connectors/relay/device-1/plugins/execute",
            ),
            (
                REMOTE_CONNECTION_RELAY_SCOPE,
                Method::POST,
                "/api/local-connectors/relay/device-1/remote-connections/command",
            ),
            (
                TERMINAL_RELAY_SCOPE,
                Method::POST,
                "/api/local-connectors/relay/device-1/terminal/exec",
            ),
            (
                SANDBOX_ROUTING_READ_SCOPE,
                Method::GET,
                "/api/local-connectors/sandbox-pairings",
            ),
            (
                SYSTEM_STATS_READ_SCOPE,
                Method::GET,
                "/api/local-connectors/system/stats",
            ),
            (
                SANDBOX_SERVICE_SCOPE,
                Method::POST,
                "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/sandbox-1/mcp",
            ),
        ] {
            let token = chatos_service_runtime::issue_internal_service_token_for_owner(
                REMOVED_TASK_RUNNER_SECRET,
                REMOVED_TASK_RUNNER,
                TOKEN_AUDIENCE,
                scope,
                60,
                "user-1",
            )
            .expect("issue removed Task Runner token");
            let headers = signed_headers(REMOVED_TASK_RUNNER, token.as_str());
            let error = internal_service_auth_from_request(&config, &headers, &method, path)
                .expect_err(
                    "Task Runner must not access Local Connector internal execution routes",
                );
            assert_eq!(
                error.message(),
                "caller service is not allowed for this Local Connector operation",
                "unexpected rejection path: {path}"
            );
        }
    }

    #[test]
    fn chatos_plugin_ui_read_token_is_scope_caller_and_path_bound() {
        let mut config = test_config();
        config
            .internal_api_secrets
            .insert(CHATOS_CALLER.to_string(), CHATOS_SECRET.to_string());
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            CHATOS_SECRET,
            CHATOS_CALLER,
            TOKEN_AUDIENCE,
            PLUGIN_UI_READ_SCOPE,
            60,
            "user-1",
        )
        .expect("issue Plugin UI read token");
        let headers = signed_headers(CHATOS_CALLER, token.as_str());
        let user = internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/ui/assets",
        )
        .expect("matching Plugin UI asset request")
        .expect("service user");
        require_chatos_service_caller(&user).expect("ChatOS service caller");
        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/execute",
        )
        .is_err());

        let artifact_token = chatos_service_runtime::issue_internal_service_token_for_owner(
            CHATOS_SECRET,
            CHATOS_CALLER,
            TOKEN_AUDIENCE,
            PLUGIN_ARTIFACT_READ_SCOPE,
            60,
            "user-1",
        )
        .expect("issue Plugin Artifact token");
        let artifact_headers = signed_headers(CHATOS_CALLER, artifact_token.as_str());
        internal_service_user_from_request(
            &config,
            &artifact_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/list",
        )
        .expect("matching Plugin Artifact request")
        .expect("service user");
        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/read",
        )
        .is_err());
        assert!(internal_service_user_from_request(
            &config,
            &artifact_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/ui/assets",
        )
        .is_err());

        let artifact_write_token = chatos_service_runtime::issue_internal_service_token_for_owner(
            CHATOS_SECRET,
            CHATOS_CALLER,
            TOKEN_AUDIENCE,
            PLUGIN_ARTIFACT_WRITE_SCOPE,
            60,
            "user-1",
        )
        .expect("issue Plugin Artifact write token");
        let artifact_write_headers = signed_headers(CHATOS_CALLER, artifact_write_token.as_str());
        internal_service_user_from_request(
            &config,
            &artifact_write_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/create",
        )
        .expect("matching Plugin Artifact write request")
        .expect("service user");
        assert!(internal_service_user_from_request(
            &config,
            &artifact_write_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/read",
        )
        .is_err());
        assert!(internal_service_user_from_request(
            &config,
            &artifact_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/update",
        )
        .is_err());

        let human = CurrentUser {
            principal_type: "human_user".to_string(),
            token_jti: None,
            user_id: "user-1".to_string(),
            username: None,
            display_name: None,
            role: "user".to_string(),
            owner_user_id: None,
            scopes: Vec::new(),
        };
        assert!(require_chatos_service_caller(&human).is_err());
    }

    #[test]
    fn chatos_workspace_token_allows_directory_and_filesystem_relays_only() {
        let mut config = test_config();
        config
            .internal_api_secrets
            .insert(CHATOS_CALLER.to_string(), CHATOS_SECRET.to_string());
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            CHATOS_SECRET,
            CHATOS_CALLER,
            TOKEN_AUDIENCE,
            WORKSPACE_DIRECTORY_WRITE_SCOPE,
            60,
            "user-1",
        )
        .expect("issue workspace directory token");
        let headers = signed_headers(CHATOS_CALLER, token.as_str());
        let path = "/api/local-connectors/relay/device-1/workspaces/workspace-1/directories";

        for method in [Method::GET, Method::POST] {
            internal_service_user_from_request(&config, &headers, &method, path)
                .expect("matching workspace directory request")
                .expect("service user");
        }
        internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/workspaces/workspace-1/filesystem",
        )
        .expect("matching workspace filesystem request")
        .expect("service user");
        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/mcp",
        )
        .is_err());
    }

    #[test]
    fn chatos_owns_remote_connection_relay() {
        let mut config = test_config();
        config
            .internal_api_secrets
            .insert(CHATOS_CALLER.to_string(), CHATOS_SECRET.to_string());
        for path in [
            "/api/local-connectors/relay/device-1/remote-connections/test",
            "/api/local-connectors/relay/device-1/remote-connections/command",
            "/api/local-connectors/relay/device-1/remote-connections/sftp",
        ] {
            let token = chatos_service_runtime::issue_internal_service_token_for_owner(
                CHATOS_SECRET,
                CHATOS_CALLER,
                TOKEN_AUDIENCE,
                REMOTE_CONNECTION_RELAY_SCOPE,
                60,
                "user-1",
            )
            .expect("issue token");
            let headers = signed_headers(CHATOS_CALLER, token.as_str());
            let user = internal_service_user_from_request(&config, &headers, &Method::POST, path)
                .expect("authorized request")
                .expect("service user");
            assert_eq!(user.owner_user_id.as_deref(), Some("user-1"));
        }
    }

    #[test]
    fn mismatched_owner_header_is_rejected_even_with_valid_signed_token() {
        let mut config = test_config();
        config
            .internal_api_secrets
            .insert(MCP_MANAGEMENT_CALLER.to_string(), MCP_SECRET.to_string());
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            MCP_SECRET,
            MCP_MANAGEMENT_CALLER,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
            "user-1",
        )
        .expect("issue owner-bound token");
        let headers = signed_headers_for_owner(MCP_MANAGEMENT_CALLER, token.as_str(), "user-2");
        let error = internal_service_auth_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/mcp",
        )
        .expect_err("mismatched owner header must be rejected");
        assert_eq!(
            error.message(),
            "Local Connector owner user id header does not match the signed token"
        );
    }

    fn signed_headers(caller: &'static str, token: &str) -> HeaderMap {
        signed_headers_for_owner(caller, token, "user-1")
    }

    fn signed_headers_for_owner(
        caller: &'static str,
        token: &str,
        owner_user_id: &str,
    ) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-local-connector-caller", HeaderValue::from_static(caller));
        headers.insert(
            "x-local-connector-internal-token",
            HeaderValue::from_str(token).expect("token header"),
        );
        headers.insert(
            "x-local-connector-owner-user-id",
            HeaderValue::from_str(owner_user_id).expect("owner header"),
        );
        headers
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            internal_mtls_port: 1,
            database_url: "postgresql://postgres:postgres@127.0.0.1/test".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_secs(1),
            relay_request_timeout: Duration::from_secs(1),
            plugin_hook_relay_request_timeout: Duration::from_secs(1),
            public_base_url: None,
            internal_api_secrets: HashMap::new(),
            require_device_connect_signature: true,
            device_connect_signature_max_skew: Duration::from_secs(300),
            active_session_lease_ttl: Duration::from_secs(90),
            valkey_url: "redis://127.0.0.1:6379/0".to_string(),
            valkey_key_prefix: "chatos:local-connector:test".to_string(),
            device_presence_ttl: Duration::from_secs(120),
            valkey_reconnect_delay: Duration::from_secs(2),
            relay_correlation_grace_ttl: Duration::from_secs(30),
            relay_delivery_ack_timeout: Duration::from_secs(3),
            terminal_subscriber_ttl: Duration::from_secs(60),
            terminal_subscriber_refresh_interval: Duration::from_secs(20),
            managed_requirements_toml_path: None,
            managed_requirements_signing_key_path: None,
            managed_requirements_signing_key_id: None,
            managed_requirements_bundle_ttl: Duration::from_secs(3600),
            controlled_network_signing_key_path: None,
            controlled_network_signing_key_id: None,
            controlled_network_policy_ttl: Duration::from_secs(300),
        }
    }
}
