// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;

fn user(role: &str) -> CurrentUser {
    CurrentUser {
        principal_type: "human_user".to_string(),
        user_id: "user-1".to_string(),
        username: "user".to_string(),
        display_name: "User".to_string(),
        role: role.to_string(),
        owner_user_id: None,
        owner_username: None,
        owner_display_name: None,
    }
}

fn binding(scope: &str) -> AgentBindingRecord {
    AgentBindingRecord {
        id: "binding-1".to_string(),
        agent_key: "agent".to_string(),
        binding_scope: scope.to_string(),
        owner_user_id: None,
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: "resource-1".to_string(),
        enabled: true,
        required: false,
        priority: 100,
        conditions: BindingConditions::default(),
        component_allowlist: Vec::new(),
        tool_allowlist: Vec::new(),
        tool_blocklist: Vec::new(),
        created_by: "user-1".to_string(),
        updated_by: "user-1".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

fn http_record() -> McpRecord {
    McpRecord {
        id: "local-mcp-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_kind: OWNER_KIND_USER.to_string(),
        visibility: VISIBILITY_PRIVATE.to_string(),
        source_kind: SOURCE_KIND_USER_CREATED.to_string(),
        name: "external_http_mcp".to_string(),
        display_name: "External HTTP MCP".to_string(),
        description: None,
        enabled: true,
        runtime: McpRuntime {
            kind: RUNTIME_KIND_HTTP.to_string(),
            url: Some("https://mcp.example.com/rpc".to_string()),
            ..McpRuntime::default()
        },
        security: ResourceSecurity::default(),
        metadata: ResourceMetadata::default(),
        plugin_component: PluginComponentOwnership::default(),
        created_by: "local-connector-service".to_string(),
        updated_by: "local-connector-service".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

#[test]
fn ordinary_users_can_only_choose_private_visibility() {
    let ordinary = user(USER_ROLE_USER);
    assert_eq!(
        normalize_visibility(Some(VISIBILITY_PRIVATE), &ordinary).unwrap(),
        VISIBILITY_PRIVATE
    );
    assert_eq!(
        normalize_visibility(Some(VISIBILITY_PUBLIC), &ordinary)
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        normalize_visibility(Some(VISIBILITY_SYSTEM_PRIVATE), &ordinary)
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
}

#[test]
fn super_admin_can_choose_public_and_system_private_visibility() {
    let admin = user(USER_ROLE_SUPER_ADMIN);
    assert_eq!(
        normalize_visibility(Some(VISIBILITY_PUBLIC), &admin).unwrap(),
        VISIBILITY_PUBLIC
    );
    assert_eq!(
        normalize_visibility(Some(VISIBILITY_SYSTEM_PRIVATE), &admin).unwrap(),
        VISIBILITY_SYSTEM_PRIVATE
    );
}

#[test]
fn ordinary_users_can_only_create_local_connector_mcps() {
    let ordinary = user(USER_ROLE_USER);
    for kind in [RUNTIME_KIND_HTTP] {
        let payload = McpPayload {
            runtime: Some(McpRuntime {
                kind: kind.to_string(),
                ..McpRuntime::default()
            }),
            ..McpPayload::default()
        };
        let err = validate_client_managed_mcp_payload(&payload, &ordinary)
            .expect_err("ordinary external HTTP MCP should be rejected");
        assert_eq!(err.status, StatusCode::FORBIDDEN);
        let runtime = payload.runtime.as_ref().expect("test runtime");
        let err = validate_client_managed_mcp_runtime(runtime, &ordinary)
            .expect_err("persisted external HTTP MCP should also be rejected");
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }
}

#[test]
fn super_admin_can_create_external_http_mcps() {
    let admin = user(USER_ROLE_SUPER_ADMIN);
    for kind in [RUNTIME_KIND_HTTP] {
        let payload = McpPayload {
            runtime: Some(McpRuntime {
                kind: kind.to_string(),
                ..McpRuntime::default()
            }),
            ..McpPayload::default()
        };
        assert!(validate_client_managed_mcp_payload(&payload, &admin).is_ok());
    }
}

#[test]
fn ordinary_users_cannot_write_for_another_owner() {
    let ordinary = user(USER_ROLE_USER);
    assert_eq!(
        requested_owner_user_id(Some("user-2"), &ordinary)
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        requested_owner_user_id(Some("user-1"), &ordinary).unwrap(),
        "user-1"
    );
}

#[test]
fn system_private_resources_require_system_or_global_binding() {
    assert!(resource_visible_in_runtime(
        "admin-id",
        VISIBILITY_SYSTEM_PRIVATE,
        "user-id",
        &binding(BINDING_SCOPE_SYSTEM_REQUIRED)
    ));
    assert!(resource_visible_in_runtime(
        "admin-id",
        VISIBILITY_SYSTEM_PRIVATE,
        "user-id",
        &binding(BINDING_SCOPE_GLOBAL_DEFAULT)
    ));
    assert!(!resource_visible_in_runtime(
        "admin-id",
        VISIBILITY_SYSTEM_PRIVATE,
        "user-id",
        &binding(BINDING_SCOPE_USER_OVERRIDE)
    ));
}

#[test]
fn external_http_mcp_requires_plain_https_and_safe_headers() {
    let valid = McpRuntime {
        kind: RUNTIME_KIND_HTTP.to_string(),
        url: Some("https://mcp.example.com/rpc?tenant=one".to_string()),
        headers: std::collections::BTreeMap::from([(
            "authorization".to_string(),
            "Bearer secret".to_string(),
        )]),
        ..McpRuntime::default()
    };
    assert!(validate_mcp_runtime(&valid).is_ok());

    for url in [
        "http://mcp.example.com/rpc",
        "https://user@mcp.example.com/rpc",
        "https://mcp.example.com/rpc#fragment",
    ] {
        assert!(validate_mcp_runtime(&McpRuntime {
            url: Some(url.to_string()),
            ..valid.clone()
        })
        .is_err());
    }
    assert!(validate_mcp_runtime(&McpRuntime {
        headers: std::collections::BTreeMap::from([(
            "host".to_string(),
            "internal.example".to_string(),
        )]),
        ..valid
    })
    .is_err());
}

#[test]
fn read_only_external_http_mcp_requires_an_explicit_tool_allowlist() {
    let runtime = McpRuntime {
        kind: RUNTIME_KIND_HTTP.to_string(),
        url: Some("https://mcp.example.com/rpc".to_string()),
        ..McpRuntime::default()
    };
    assert!(validate_mcp_security(&runtime, &ResourceSecurity::default()).is_err());
    assert!(validate_mcp_security(
        &runtime,
        &ResourceSecurity {
            allowed_tool_names: vec!["search".to_string()],
            ..ResourceSecurity::default()
        }
    )
    .is_ok());
    assert!(validate_mcp_security(
        &runtime,
        &ResourceSecurity {
            allow_writes: Some(true),
            ..ResourceSecurity::default()
        }
    )
    .is_ok());
}

#[test]
fn public_mcp_responses_redact_runtime_credentials() {
    let mut record = http_record();
    record.owner_user_id = "user-2".to_string();
    record.runtime.headers = std::collections::BTreeMap::from([(
        "authorization".to_string(),
        "Bearer secret".to_string(),
    )]);
    record.runtime.env =
        std::collections::BTreeMap::from([("API_KEY".to_string(), "secret".to_string())]);
    record.runtime.args = vec!["--token".to_string(), "secret".to_string()];
    record.runtime.url = Some("https://mcp.example.com/rpc?token=secret".to_string());
    redact_mcp_runtime_secrets_for_user(&mut record, &user(USER_ROLE_USER));
    assert!(record.runtime.headers.is_empty());
    assert!(record.runtime.env.is_empty());
    assert!(record.runtime.args.is_empty());
    assert_eq!(
        record.runtime.url.as_deref(),
        Some("https://mcp.example.com/rpc")
    );

    let mut owner_record = http_record();
    owner_record.runtime.headers = std::collections::BTreeMap::from([(
        "authorization".to_string(),
        "Bearer owner-secret".to_string(),
    )]);
    redact_mcp_runtime_secrets_for_user(&mut owner_record, &user(USER_ROLE_USER));
    assert_eq!(owner_record.runtime.headers.len(), 1);
}

#[test]
fn builtin_mcps_cannot_be_created_through_the_api() {
    let payload = McpPayload {
        runtime: Some(McpRuntime {
            kind: RUNTIME_KIND_BUILTIN.to_string(),
            builtin_kind: Some("Notepad".to_string()),
            ..McpRuntime::default()
        }),
        ..McpPayload::default()
    };
    assert_eq!(
        validate_client_managed_mcp_payload(&payload, &user(USER_ROLE_USER))
            .unwrap_err()
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn system_mcps_cannot_be_created_through_the_api() {
    let payload = McpPayload {
        runtime: Some(McpRuntime {
            kind: RUNTIME_KIND_SYSTEM.to_string(),
            system_key: Some("code_maintainer_read".to_string()),
            ..McpRuntime::default()
        }),
        ..McpPayload::default()
    };
    assert_eq!(
        validate_client_managed_mcp_payload(&payload, &user(USER_ROLE_USER))
            .unwrap_err()
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn client_managed_mcps_cannot_claim_the_system_seed_source() {
    let payload = McpPayload {
        source_kind: Some(SOURCE_KIND_SYSTEM_SEED.to_string()),
        ..McpPayload::default()
    };
    assert_eq!(
        validate_client_managed_mcp_payload(&payload, &user(USER_ROLE_USER))
            .unwrap_err()
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn system_seed_mcps_only_allow_enabled_updates() {
    assert!(validate_system_seed_mcp_update(&McpPayload {
        enabled: Some(false),
        ..McpPayload::default()
    })
    .is_ok());

    assert_eq!(
        validate_system_seed_mcp_update(&McpPayload {
            name: Some("renamed".to_string()),
            ..McpPayload::default()
        })
        .unwrap_err()
        .status,
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn mcp_binding_modes_are_limited_to_three_states() {
    assert!(validate_mcp_binding_mode(MCP_BINDING_MODE_DISABLED).is_ok());
    assert!(validate_mcp_binding_mode(MCP_BINDING_MODE_OPTIONAL).is_ok());
    assert!(validate_mcp_binding_mode(MCP_BINDING_MODE_REQUIRED).is_ok());
    assert_eq!(
        validate_mcp_binding_mode("conditional").unwrap_err().status,
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn disabled_mcp_bindings_are_persisted_but_excluded_from_runtime() {
    assert_eq!(
        mcp_binding_state(MCP_BINDING_MODE_DISABLED).unwrap(),
        (false, false, BINDING_SCOPE_GLOBAL_DEFAULT)
    );
    assert_eq!(
        mcp_binding_state(MCP_BINDING_MODE_OPTIONAL).unwrap(),
        (true, false, BINDING_SCOPE_GLOBAL_DEFAULT)
    );
    assert_eq!(
        mcp_binding_state(MCP_BINDING_MODE_REQUIRED).unwrap(),
        (true, true, BINDING_SCOPE_SYSTEM_REQUIRED)
    );
}

#[test]
fn automatic_user_resources_are_optional_and_owner_scoped() {
    let binding = automatic_user_binding(
        chatos_agent::SystemAgentKey::TaskRunnerRunPhase.as_str(),
        "user-1",
        "mcp",
        "mcp-1",
    );
    assert!(!binding.required);
    assert_eq!(binding.owner_user_id.as_deref(), Some("user-1"));
    assert_eq!(binding.resource_kind, RESOURCE_KIND_MCP);
}

#[test]
fn release_managed_components_only_allow_rollout_overrides() {
    let ownership = PluginComponentOwnership {
        plugin_id: Some("plugin-1".to_string()),
        release_id: Some("release-1".to_string()),
        component_key: Some("main".to_string()),
        managed_by_plugin: true,
        immutable_from_release: true,
    };
    assert!(validate_release_managed_mcp_update(
        &ownership,
        &McpPayload {
            display_name: Some("Friendly name".to_string()),
            enabled: Some(false),
            ..McpPayload::default()
        }
    )
    .is_ok());
    assert_eq!(
        validate_release_managed_mcp_update(
            &ownership,
            &McpPayload {
                runtime: Some(McpRuntime::default()),
                ..McpPayload::default()
            }
        )
        .unwrap_err()
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        validate_release_managed_agent_update(
            &ownership,
            &SystemAgentPayload {
                service_name: Some("other-service".to_string()),
                ..SystemAgentPayload::default()
            }
        )
        .unwrap_err()
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        ensure_release_managed_resource_not_deleted(&ownership)
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
}
