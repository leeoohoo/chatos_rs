// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::asset_response::plugin_ui_response_content_security_policy;
use super::{
    get_plugin_ui_workbench_session, issue_plugin_ui_workbench_session, lock_workbench_sessions,
    normalize_requested_asset_path, plugin_artifact_download_response, plugin_ui_asset_response,
    prepare_plugin_artifact_relay_request, require_workbench_capability,
    validate_artifact_read_response, validate_artifact_write_response, validate_asset_response,
    validate_ready_payload,
};
use crate::core::auth::AuthUser;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_SECURITY_POLICY, REFERRER_POLICY,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chatos_plugin_management_sdk::{
    plugin_ui_snapshot_sha256, PluginArtifactDescriptor, PluginArtifactOwner,
    PluginArtifactReadResponse, PluginArtifactUiAccess, PluginArtifactWriteOperation,
    PluginArtifactWriteResponse, PluginUiAssetKind, PluginUiAssetReadResponse,
    PluginUiAssetSnapshot, PluginUiReadyEventPayload, PluginUiSnapshot,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
    PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1, PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1,
    PLUGIN_UI_READY_EVENT_VERSION_V1,
};
use sha2::{Digest, Sha256};

fn ready_payload() -> PluginUiReadyEventPayload {
    let html = b"<!doctype html><script src=\"app.js\"></script>";
    let script = b"window.parent.postMessage({type:'ready'}, '*');";
    let content_sha256 = hex::encode(Sha256::digest(html));
    let assets = vec![PluginUiAssetSnapshot {
        relative_path: "./ui/app.js".to_string(),
        media_type: "text/javascript".to_string(),
        size_bytes: script.len() as u64,
        sha256: hex::encode(Sha256::digest(script)),
    }];
    let snapshot_sha256 = plugin_ui_snapshot_sha256(
        "plugin-1",
        "release-1",
        "workbench",
        "Workbench",
        "workbench",
        "./ui/index.html",
        content_sha256.as_str(),
        assets.as_slice(),
        PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
        &[
            "host.context.read".to_string(),
            "artifact.list".to_string(),
            "artifact.read".to_string(),
            "artifact.download".to_string(),
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE.to_string(),
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE.to_string(),
        ],
        &["application/pdf".to_string()],
        PLUGIN_UI_HOST_CSP_V1,
        PLUGIN_UI_IFRAME_SANDBOX_V1,
    )
    .expect("snapshot hash");
    PluginUiReadyEventPayload {
        event_schema_version: PLUGIN_UI_READY_EVENT_VERSION_V1,
        run_id: "run-1".to_string(),
        device_id: "device-1".to_string(),
        workspace_id: Some("workspace-1".to_string()),
        plugin_id: "plugin-1".to_string(),
        release_id: "release-1".to_string(),
        artifact_sha256: "b".repeat(64),
        component_key: "workbench".to_string(),
        adapter_session_id: "adapter-1".to_string(),
        ui: PluginUiSnapshot {
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            version: "1.0.0".to_string(),
            artifact_sha256: "b".repeat(64),
            component_key: "workbench".to_string(),
            title: "Workbench".to_string(),
            surface: "workbench".to_string(),
            relative_source_path: "./ui/index.html".to_string(),
            content_sha256,
            assets,
            bridge_protocol_version: PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
            bridge_capabilities: vec![
                "host.context.read".to_string(),
                "artifact.list".to_string(),
                "artifact.read".to_string(),
                "artifact.download".to_string(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE.to_string(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE.to_string(),
            ],
            artifact_mime_types: vec!["application/pdf".to_string()],
            content_security_policy: PLUGIN_UI_HOST_CSP_V1.to_string(),
            iframe_sandbox: PLUGIN_UI_IFRAME_SANDBOX_V1.to_string(),
            snapshot_sha256,
        },
    }
}

#[test]
fn ready_event_and_asset_response_are_revalidated_end_to_end() {
    let ready = ready_payload();
    validate_ready_payload(&ready).expect("valid ready payload");
    let body = b"window.parent.postMessage({type:'ready'}, '*');";
    let response = PluginUiAssetReadResponse {
        run_id: ready.run_id.clone(),
        owner_user_id: "user-1".to_string(),
        plugin_id: ready.plugin_id.clone(),
        release_id: ready.release_id.clone(),
        artifact_sha256: ready.artifact_sha256.clone(),
        component_key: ready.component_key.clone(),
        adapter_session_id: ready.adapter_session_id.clone(),
        ui_snapshot_sha256: ready.ui.snapshot_sha256.clone(),
        kind: PluginUiAssetKind::StaticAsset,
        relative_path: "./ui/app.js".to_string(),
        media_type: "text/javascript".to_string(),
        size_bytes: body.len() as u64,
        sha256: hex::encode(Sha256::digest(body)),
        body_base64: BASE64_STANDARD.encode(body),
    };
    validate_asset_response(
        &AuthUser {
            user_id: "user-1".to_string(),
            role: "user".to_string(),
        },
        &ready,
        "./ui/app.js",
        &response,
    )
    .expect("valid asset response");

    let mut tampered = response;
    tampered.body_base64 = BASE64_STANDARD.encode(b"tampered");
    assert!(validate_asset_response(
        &AuthUser {
            user_id: "user-1".to_string(),
            role: "user".to_string(),
        },
        &ready,
        "./ui/app.js",
        &tampered,
    )
    .is_err());
}

#[test]
fn asset_url_path_is_canonical_and_traversal_safe() {
    assert_eq!(
        normalize_requested_asset_path("ui/app.js").expect("valid path"),
        "./ui/app.js"
    );
    for path in ["", "../secret", "ui/../secret", "ui\\app.js", "ui/app.exe"] {
        assert!(normalize_requested_asset_path(path).is_err(), "{path}");
    }
}

#[test]
fn workbench_session_is_short_lived_snapshot_bound_and_revocable() {
    let auth = AuthUser {
        user_id: format!("user-{}", uuid::Uuid::new_v4()),
        role: "user".to_string(),
    };
    let ready = ready_payload();
    validate_ready_payload(&ready).expect("valid ready payload");
    let response =
        issue_plugin_ui_workbench_session(&auth, "message-1", "event-1", ready.clone(), None)
            .expect("issue workbench session");
    assert_eq!(response.expires_in, 300);
    assert!(response
        .iframe_path
        .starts_with("/api/plugin-ui/workbench/pui_"));
    assert!(response.iframe_path.contains("/ui/index.html#"));
    assert!(!response.iframe_path.contains("<!doctype"));
    assert_eq!(response.host_context.run_id, ready.run_id);

    let stored = get_plugin_ui_workbench_session(response.session_id.as_str())
        .expect("read workbench session");
    assert_eq!(stored.owner_user_id, auth.user_id);
    assert_eq!(stored.ready.ui.snapshot_sha256, ready.ui.snapshot_sha256);

    lock_workbench_sessions()
        .expect("lock workbench sessions")
        .remove(response.session_id.as_str());
    assert!(get_plugin_ui_workbench_session(response.session_id.as_str()).is_err());
}

#[test]
fn entrypoint_response_enforces_opaque_workbench_security_headers() {
    let ready = ready_payload();
    let body = b"<!doctype html><script src=\"app.js\"></script>";
    let response = plugin_ui_asset_response(
        &ready.ui,
        PluginUiAssetReadResponse {
            run_id: ready.run_id,
            owner_user_id: "user-1".to_string(),
            plugin_id: ready.plugin_id,
            release_id: ready.release_id,
            artifact_sha256: ready.artifact_sha256,
            component_key: ready.component_key,
            adapter_session_id: ready.adapter_session_id,
            ui_snapshot_sha256: ready.ui.snapshot_sha256.clone(),
            kind: PluginUiAssetKind::Entrypoint,
            relative_path: ready.ui.relative_source_path.clone(),
            media_type: "text/html; charset=utf-8".to_string(),
            size_bytes: body.len() as u64,
            sha256: ready.ui.content_sha256.clone(),
            body_base64: BASE64_STANDARD.encode(body),
        },
        None,
    )
    .expect("entrypoint response");
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[REFERRER_POLICY], "no-referrer");
    assert_eq!(
        response.headers()["cross-origin-resource-policy"],
        "same-origin"
    );
    assert_eq!(
        response.headers()[CONTENT_SECURITY_POLICY],
        ready.ui.content_security_policy.as_str()
    );
    assert!(!ready.ui.iframe_sandbox.contains("allow-same-origin"));
}

#[test]
fn workbench_resource_origin_is_absolute_and_csp_is_parent_exact() {
    let auth = AuthUser {
        user_id: format!("user-{}", uuid::Uuid::new_v4()),
        role: "user".to_string(),
    };
    let response = issue_plugin_ui_workbench_session(
        &auth,
        "message-origin",
        "event-origin",
        ready_payload(),
        Some("https://plugin-ui.example.com"),
    )
    .expect("issue cross-origin workbench session");
    assert!(response.iframe_path.starts_with(&format!(
        "https://plugin-ui.example.com/api/plugin-ui/workbench/{}/",
        response.session_id
    )));
    let csp = plugin_ui_response_content_security_policy(
        PLUGIN_UI_HOST_CSP_V1,
        Some("https://app.example.com"),
    )
    .expect("derive exact parent CSP");
    assert!(csp.contains("frame-ancestors https://app.example.com"));
    assert!(!csp.contains("frame-ancestors 'self'"));
    assert!(plugin_ui_response_content_security_policy(
        PLUGIN_UI_HOST_CSP_V1,
        Some("https://app.example.com; frame-src *"),
    )
    .is_err());

    lock_workbench_sessions()
        .expect("lock workbench sessions")
        .remove(response.session_id.as_str());
}

#[test]
fn artifact_read_is_owner_bound_and_download_headers_are_safe() {
    let ready = ready_payload();
    let auth = AuthUser {
        user_id: "user-1".to_string(),
        role: "user".to_string(),
    };
    let access = PluginArtifactUiAccess {
        run_id: ready.run_id.clone(),
        plugin_id: ready.plugin_id.clone(),
        release_id: ready.release_id.clone(),
        artifact_sha256: ready.artifact_sha256.clone(),
        component_key: ready.component_key.clone(),
        adapter_session_id: ready.adapter_session_id.clone(),
        ui_snapshot_sha256: ready.ui.snapshot_sha256.clone(),
    };
    let body = b"%PDF-fixture";
    let artifact = PluginArtifactDescriptor {
        artifact_id: format!("pa_{}", "c".repeat(32)),
        owner: PluginArtifactOwner {
            owner_user_id: auth.user_id.clone(),
            run_id: ready.run_id.clone(),
            device_id: ready.device_id.clone(),
            workspace_id: ready.workspace_id.clone().expect("workspace"),
            plugin_id: ready.plugin_id.clone(),
            release_id: ready.release_id.clone(),
            artifact_sha256: ready.artifact_sha256.clone(),
            component_key: "documents".to_string(),
            adapter_session_id: "producer-session".to_string(),
        },
        workspace_relative_path: "artifacts/report.pdf".to_string(),
        display_name: "报告 \"final\".pdf".to_string(),
        media_type: "application/pdf".to_string(),
        size_bytes: body.len() as u64,
        sha256: hex::encode(Sha256::digest(body)),
        created_at: "2026-07-26T00:00:00Z".to_string(),
        producer_tool_name: "create_text_pdf".to_string(),
        downloadable: true,
        mutable: false,
    };
    let response = PluginArtifactReadResponse {
        access: access.clone(),
        artifact: artifact.clone(),
        body_base64: BASE64_STANDARD.encode(body),
    };
    assert!(validate_artifact_read_response(
        &auth,
        &ready,
        &access,
        artifact.artifact_id.as_str(),
        &response,
    )
    .is_err());

    let mut safe = response;
    safe.artifact.display_name = "report.pdf".to_string();
    validate_artifact_read_response(&auth, &ready, &access, artifact.artifact_id.as_str(), &safe)
        .expect("valid Artifact response");
    let download = plugin_artifact_download_response(safe).expect("download response");
    assert_eq!(download.headers()[CACHE_CONTROL], "no-store");
    assert!(download.headers()[CONTENT_DISPOSITION]
        .to_str()
        .expect("Content-Disposition")
        .starts_with("attachment;"));
}

#[test]
fn artifact_write_is_capability_exact_ui_owner_and_body_bound() {
    let ready = ready_payload();
    let auth = AuthUser {
        user_id: "user-1".to_string(),
        role: "user".to_string(),
    };
    let issued = issue_plugin_ui_workbench_session(
        &auth,
        "message-write",
        "event-write",
        ready.clone(),
        None,
    )
    .expect("issue writable workbench session");
    let session = get_plugin_ui_workbench_session(issued.session_id.as_str())
        .expect("read writable workbench session");
    require_workbench_capability(&session, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE)
        .expect("create capability");

    let access = PluginArtifactUiAccess {
        run_id: ready.run_id.clone(),
        plugin_id: ready.plugin_id.clone(),
        release_id: ready.release_id.clone(),
        artifact_sha256: ready.artifact_sha256.clone(),
        component_key: ready.component_key.clone(),
        adapter_session_id: ready.adapter_session_id.clone(),
        ui_snapshot_sha256: ready.ui.snapshot_sha256.clone(),
    };
    let body = b"%PDF-plugin-draft";
    let artifact_id = format!("pa_{}", "e".repeat(32));
    let artifact = PluginArtifactDescriptor {
        artifact_id: artifact_id.clone(),
        owner: PluginArtifactOwner {
            owner_user_id: auth.user_id.clone(),
            run_id: ready.run_id.clone(),
            device_id: ready.device_id.clone(),
            workspace_id: ready.workspace_id.clone().expect("workspace"),
            plugin_id: ready.plugin_id.clone(),
            release_id: ready.release_id.clone(),
            artifact_sha256: ready.artifact_sha256.clone(),
            component_key: ready.component_key.clone(),
            adapter_session_id: ready.adapter_session_id.clone(),
        },
        workspace_relative_path: format!("chatos-plugin-artifacts/opaque/{artifact_id}/draft.pdf"),
        display_name: "draft.pdf".to_string(),
        media_type: "application/pdf".to_string(),
        size_bytes: body.len() as u64,
        sha256: hex::encode(Sha256::digest(body)),
        created_at: "2026-07-26T00:00:00Z".to_string(),
        producer_tool_name: PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE.to_string(),
        downloadable: true,
        mutable: true,
    };
    let response = PluginArtifactWriteResponse {
        access: access.clone(),
        operation: PluginArtifactWriteOperation::Create,
        artifact: artifact.clone(),
    };
    validate_artifact_write_response(
        &auth,
        &ready,
        &access,
        PluginArtifactWriteOperation::Create,
        None,
        Some(("draft.pdf", "application/pdf")),
        body,
        &response,
    )
    .expect("valid mutable Artifact create response");

    let mut wrong_owner = response.clone();
    wrong_owner.artifact.owner.adapter_session_id = "other-session".to_string();
    assert!(validate_artifact_write_response(
        &auth,
        &ready,
        &access,
        PluginArtifactWriteOperation::Create,
        None,
        Some(("draft.pdf", "application/pdf")),
        body,
        &wrong_owner,
    )
    .is_err());

    let updated_body = b"%PDF-plugin-draft-v2";
    let mut updated = artifact;
    updated.size_bytes = updated_body.len() as u64;
    updated.sha256 = hex::encode(Sha256::digest(updated_body));
    updated.producer_tool_name = PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE.to_string();
    validate_artifact_write_response(
        &auth,
        &ready,
        &access,
        PluginArtifactWriteOperation::Update,
        Some(artifact_id.as_str()),
        None,
        updated_body,
        &PluginArtifactWriteResponse {
            access: access.clone(),
            operation: PluginArtifactWriteOperation::Update,
            artifact: updated,
        },
    )
    .expect("valid mutable Artifact update response");

    lock_workbench_sessions()
        .expect("lock workbench sessions")
        .remove(issued.session_id.as_str());
}

#[test]
fn artifact_relay_request_is_signed_routed_and_timeout_bound() {
    let auth = AuthUser {
        user_id: "user-1".to_string(),
        role: "user".to_string(),
    };
    let mut ready = ready_payload();
    ready.device_id = "device /一".to_string();
    let secret = "a-long-chatos-local-connector-secret";

    let read = prepare_plugin_artifact_relay_request(
        auth.user_id.as_str(),
        &ready,
        "read",
        " https://connector.example.test/ ",
        secret,
        100,
    )
    .expect("prepare read relay request");
    assert_eq!(
            read.url,
            "https://connector.example.test/api/local-connectors/relay/device%20%2F%E4%B8%80/plugins/artifacts/read"
        );
    assert_eq!(read.workspace_id, "workspace-1");
    assert_eq!(read.owner_user_id, "user-1");
    assert_eq!(read.timeout, std::time::Duration::from_millis(300));
    chatos_service_runtime::verify_internal_service_token(
        read.token.as_str(),
        secret,
        "chatos-backend",
        "local-connector-service",
        "plugin.artifact.read",
    )
    .expect("verify read-scoped service token");

    let write = prepare_plugin_artifact_relay_request(
        auth.user_id.as_str(),
        &ready,
        "update",
        "https://connector.example.test",
        secret,
        1_000,
    )
    .expect("prepare write relay request");
    assert_eq!(write.timeout, std::time::Duration::from_millis(315_000));
    chatos_service_runtime::verify_internal_service_token(
        write.token.as_str(),
        secret,
        "chatos-backend",
        "local-connector-service",
        "plugin.artifact.write",
    )
    .expect("verify write-scoped service token");
    assert!(chatos_service_runtime::verify_internal_service_token(
        write.token.as_str(),
        secret,
        "chatos-backend",
        "local-connector-service",
        "plugin.artifact.read",
    )
    .is_err());
}
