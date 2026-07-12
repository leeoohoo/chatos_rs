// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

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
        created_by: "user-1".to_string(),
        updated_by: "user-1".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

fn local_connector_record() -> McpRecord {
    McpRecord {
        id: "local-mcp-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_kind: OWNER_KIND_USER.to_string(),
        visibility: VISIBILITY_PRIVATE.to_string(),
        source_kind: SOURCE_KIND_LOCAL_CONNECTOR_DISCOVERED.to_string(),
        name: "user_mcp_manifest1".to_string(),
        display_name: "Local MCP".to_string(),
        description: None,
        enabled: true,
        runtime: McpRuntime {
            kind: RUNTIME_KIND_LOCAL_CONNECTOR_STDIO.to_string(),
            server_name: Some("user_mcp_manifest1".to_string()),
            local_connector: Some(LocalConnectorRef {
                device_id: Some("device-1".to_string()),
                workspace_id: None,
                manifest_id: Some("manifest-1".to_string()),
                relative_path: None,
                requires_online: true,
            }),
            ..McpRuntime::default()
        },
        security: ResourceSecurity::default(),
        metadata: ResourceMetadata::default(),
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
    for kind in [RUNTIME_KIND_HTTP, RUNTIME_KIND_STDIO_CLOUD] {
        let payload = McpPayload {
            runtime: Some(McpRuntime {
                kind: kind.to_string(),
                ..McpRuntime::default()
            }),
            ..McpPayload::default()
        };
        let err = validate_client_managed_mcp_payload(&payload, &ordinary)
            .expect_err("ordinary cloud MCP should be rejected");
        assert_eq!(err.status, StatusCode::FORBIDDEN);
        let runtime = payload.runtime.as_ref().expect("test runtime");
        let err = validate_client_managed_mcp_runtime(runtime, &ordinary)
            .expect_err("persisted legacy cloud MCP should also be rejected");
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }

    for kind in [
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO,
        RUNTIME_KIND_LOCAL_CONNECTOR_HTTP,
    ] {
        let payload = McpPayload {
            runtime: Some(McpRuntime {
                kind: kind.to_string(),
                ..McpRuntime::default()
            }),
            ..McpPayload::default()
        };
        assert!(validate_client_managed_mcp_payload(&payload, &ordinary).is_ok());
    }
}

#[test]
fn super_admin_can_create_explicit_cloud_mcps() {
    let admin = user(USER_ROLE_SUPER_ADMIN);
    for kind in [RUNTIME_KIND_HTTP, RUNTIME_KIND_STDIO_CLOUD] {
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
fn local_connector_mcp_requires_connector_reference() {
    let runtime = McpRuntime {
        kind: RUNTIME_KIND_LOCAL_CONNECTOR_STDIO.to_string(),
        command: Some("tool".to_string()),
        ..McpRuntime::default()
    };
    assert_eq!(
        validate_mcp_runtime(&runtime).unwrap_err().status,
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn local_connector_user_mcp_does_not_require_workspace() {
    let runtime = McpRuntime {
        kind: RUNTIME_KIND_LOCAL_CONNECTOR_STDIO.to_string(),
        server_name: Some("user_mcp_manifest1".to_string()),
        local_connector: Some(LocalConnectorRef {
            device_id: Some("device-1".to_string()),
            workspace_id: None,
            manifest_id: Some("manifest-1".to_string()),
            relative_path: None,
            requires_online: true,
        }),
        ..McpRuntime::default()
    };

    assert!(validate_mcp_runtime(&runtime).is_ok());
}

#[test]
fn local_connector_user_mcp_scope_is_private_and_owner_isolated() {
    let record = local_connector_record();
    assert!(
        ensure_local_connector_record_scope(&record, "user-1", "device-1", "manifest-1",).is_ok()
    );

    let mut public = record.clone();
    public.visibility = VISIBILITY_PUBLIC.to_string();
    assert_eq!(
        ensure_local_connector_record_scope(&public, "user-1", "device-1", "manifest-1",)
            .unwrap_err()
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        ensure_local_connector_record_scope(&record, "user-2", "device-1", "manifest-1",)
            .unwrap_err()
            .status,
        StatusCode::NOT_FOUND
    );
}

#[test]
fn local_connector_status_rejects_manifest_hash_mismatch() {
    let check = ResourceCheckRecord {
        id: "mcp:local-mcp-1".to_string(),
        resource_kind: RESOURCE_KIND_MCP.to_string(),
        resource_id: "local-mcp-1".to_string(),
        owner_user_id: "user-1".to_string(),
        status: "available".to_string(),
        last_checked_at: now_rfc3339(),
        last_error: None,
        tool_snapshot: vec![json!({"name": "demo"})],
        manifest_hash: Some("hash-1".to_string()),
    };

    assert!(ensure_local_connector_manifest_hash_matches(Some(&check), Some("hash-1")).is_ok());
    assert_eq!(
        ensure_local_connector_manifest_hash_matches(Some(&check), Some("hash-2"))
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
}

#[test]
fn local_connector_availability_check_expires_after_ttl() {
    let now = chrono::Utc::now().to_rfc3339();
    let stale = (chrono::Utc::now() - chrono::Duration::seconds(61)).to_rfc3339();

    assert!(local_connector_check_is_fresh(
        now.as_str(),
        Duration::from_secs(60)
    ));
    assert!(!local_connector_check_is_fresh(
        stale.as_str(),
        Duration::from_secs(60)
    ));
    assert!(!local_connector_check_is_fresh(
        "invalid",
        Duration::from_secs(60)
    ));
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
fn system_routed_mcps_cannot_be_created_through_the_api() {
    let payload = McpPayload {
        runtime: Some(McpRuntime {
            kind: RUNTIME_KIND_SYSTEM_ROUTED.to_string(),
            server_name: Some("sandbox_images".to_string()),
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
    let binding = automatic_user_binding("task_runner_run_phase", "user-1", "mcp", "mcp-1");
    assert!(!binding.required);
    assert_eq!(binding.owner_user_id.as_deref(), Some("user-1"));
    assert_eq!(binding.resource_kind, RESOURCE_KIND_MCP);
}
