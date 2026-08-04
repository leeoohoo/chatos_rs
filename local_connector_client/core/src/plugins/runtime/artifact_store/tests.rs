// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::WorkspaceState;
use chatos_plugin_management_sdk::{
    PluginArtifactListRequest, PluginArtifactReadRequest,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE, PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
    PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1,
};
use serde_json::json;
use tempfile::TempDir;

fn test_grant(access: &mut PluginArtifactUiAccess) -> PluginUiArtifactGrant {
    let mut ui = PluginUiSnapshot {
        plugin_id: access.plugin_id.clone(),
        release_id: access.release_id.clone(),
        version: "1.0.0".to_string(),
        artifact_sha256: access.artifact_sha256.clone(),
        component_key: access.component_key.clone(),
        title: "Workbench".to_string(),
        surface: "workbench".to_string(),
        relative_source_path: "./ui/index.html".to_string(),
        content_sha256: "c".repeat(64),
        assets: Vec::new(),
        bridge_protocol_version: PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
        bridge_capabilities: vec![
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST.to_string(),
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ.to_string(),
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD.to_string(),
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE.to_string(),
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE.to_string(),
        ],
        artifact_mime_types: vec![
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
            "application/json".to_string(),
        ],
        content_security_policy: PLUGIN_UI_HOST_CSP_V1.to_string(),
        iframe_sandbox: PLUGIN_UI_IFRAME_SANDBOX_V1.to_string(),
        snapshot_sha256: String::new(),
    };
    ui.snapshot_sha256 = plugin_ui_snapshot_sha256(
        ui.plugin_id.as_str(),
        ui.release_id.as_str(),
        ui.component_key.as_str(),
        ui.title.as_str(),
        ui.surface.as_str(),
        ui.relative_source_path.as_str(),
        ui.content_sha256.as_str(),
        ui.assets.as_slice(),
        ui.bridge_protocol_version,
        ui.bridge_capabilities.as_slice(),
        ui.artifact_mime_types.as_slice(),
        ui.content_security_policy.as_str(),
        ui.iframe_sandbox.as_str(),
    )
    .expect("hash UI snapshot");
    access.ui_snapshot_sha256 = ui.snapshot_sha256.clone();
    PluginUiArtifactGrant {
        owner_user_id: "owner-a".to_string(),
        device_id: "device-a".to_string(),
        workspace_id: "workspace-a".to_string(),
        run_id: access.run_id.clone(),
        plugin_id: access.plugin_id.clone(),
        release_id: access.release_id.clone(),
        artifact_sha256: access.artifact_sha256.clone(),
        component_key: access.component_key.clone(),
        adapter_session_id: access.adapter_session_id.clone(),
        ui,
        permission_snapshot: BTreeSet::new(),
        expires_at: Utc::now().timestamp() + 3_600,
    }
}

fn fixture() -> (
    TempDir,
    LocalState,
    RelayRequest,
    PluginArtifactStore,
    PluginArtifactUiAccess,
) {
    let temp = TempDir::new().expect("temp directory");
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(workspace.join("artifacts")).expect("workspace");
    let state = LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-a".to_string(),
            absolute_root: workspace,
            alias: "workspace-a".to_string(),
            fingerprint: "workspace-fingerprint".to_string(),
            project_config_trust: None,
        }],
        ..LocalState::default()
    };
    let request = RelayRequest {
        _message_type: "plugin_execute_request".to_string(),
        request_id: "request-a".to_string(),
        owner_user_id: Some("owner-a".to_string()),
        device_id: Some("device-a".to_string()),
        workspace_id: "workspace-a".to_string(),
        method: Some("POST".to_string()),
        path: Some("/plugins/execute".to_string()),
        headers: BTreeMap::new(),
        body: json!({}),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let mut access = PluginArtifactUiAccess {
        run_id: "run-a".to_string(),
        plugin_id: "plugin-a".to_string(),
        release_id: "release-a".to_string(),
        artifact_sha256: "a".repeat(64),
        component_key: "workbench".to_string(),
        adapter_session_id: "ui-session-a".to_string(),
        ui_snapshot_sha256: String::new(),
    };
    let store = PluginArtifactStore::default();
    store
        .register_ui_grant(test_grant(&mut access))
        .expect("register UI grant");
    (temp, state, request, store, access)
}

#[test]
fn registers_lists_and_revalidates_exact_plugin_artifact_files() {
    let (_temp, state, request, store, access) = fixture();
    let workspace = &state.workspaces[0].absolute_root;
    fs::write(workspace.join("artifacts/report.docx"), b"docx fixture").expect("write artifact");
    let descriptors = store
        .register_native_outputs(
            &state,
            &request,
            PluginArtifactProducer {
                owner_user_id: "owner-a",
                device_id: "device-a",
                workspace_id: "workspace-a",
                run_id: "run-a",
                plugin_id: "plugin-a",
                release_id: "release-a",
                artifact_sha256: &"a".repeat(64),
                component_key: "documents",
                adapter_session_id: "native-session-a",
                skill_id: "internal_skill_documents",
                tool_name: "create_docx",
            },
            &json!({"target_path": "artifacts/report.docx"}),
            &json!({"created": true, "path": "artifacts/report.docx"}),
        )
        .expect("register Artifact");
    assert_eq!(descriptors.len(), 1);
    let grant = store
        .ui_grant(
            &request,
            &PluginArtifactListRequest {
                access: access.clone(),
            }
            .access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
        )
        .expect("UI grant");
    let listed = store.list(&grant, access.clone()).expect("list Artifacts");
    assert_eq!(listed.artifacts, descriptors);

    let read_request = PluginArtifactReadRequest {
        access: access.clone(),
        artifact_id: descriptors[0].artifact_id.clone(),
        mode: PluginArtifactReadMode::Download,
    };
    let read = store
        .read(
            &state,
            &request,
            &grant,
            read_request.access,
            read_request.artifact_id.as_str(),
            read_request.mode,
        )
        .expect("read Artifact");
    assert_eq!(
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, read.body_base64,)
            .expect("decode Artifact"),
        b"docx fixture"
    );

    fs::write(workspace.join("artifacts/report.docx"), b"tampered").expect("tamper Artifact");
    assert!(store
        .read(
            &state,
            &request,
            &grant,
            access,
            descriptors[0].artifact_id.as_str(),
            PluginArtifactReadMode::Download,
        )
        .is_err());
}

#[test]
fn creates_and_optimistically_updates_ui_owned_mutable_artifacts() {
    let (temp, state, request, _ephemeral_store, access) = fixture();
    let state_path = temp.path().join("state.json");
    let storage = SecureStorage::in_memory("Plugin Artifact write test");
    let store =
        PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage.clone());
    let mut write_access = access.clone();
    store
        .register_ui_grant(test_grant(&mut write_access))
        .expect("persist write grant");
    let grant = store
        .ui_grant(
            &request,
            &write_access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
        )
        .expect("create grant");
    fs::write(
        state.workspaces[0]
            .absolute_root
            .join("artifacts/native.docx"),
        b"immutable native artifact",
    )
    .expect("write immutable native Artifact");
    let immutable = store
        .register_native_outputs(
            &state,
            &request,
            PluginArtifactProducer {
                owner_user_id: "owner-a",
                device_id: "device-a",
                workspace_id: "workspace-a",
                run_id: "run-a",
                plugin_id: "plugin-a",
                release_id: "release-a",
                artifact_sha256: &"a".repeat(64),
                component_key: "documents",
                adapter_session_id: "native-session-a",
                skill_id: "internal_skill_documents",
                tool_name: "create_docx",
            },
            &json!({"target_path": "artifacts/native.docx"}),
            &json!({"created": true, "path": "artifacts/native.docx"}),
        )
        .expect("register immutable native Artifact");
    let immutable_update = store
        .update(
            &state,
            &request,
            &grant,
            write_access.clone(),
            immutable[0].artifact_id.as_str(),
            immutable[0].sha256.as_str(),
            b"must not overwrite native Artifact",
        )
        .expect_err("immutable native Artifact update must fail");
    assert_eq!(immutable_update.0, 409);
    let created = store
        .create(
            &state,
            &request,
            &grant,
            write_access.clone(),
            "report.json",
            "application/json",
            br#"{"version":1}"#,
        )
        .expect("create mutable Artifact");
    assert_eq!(created.operation, PluginArtifactWriteOperation::Create);
    assert!(created.artifact.mutable);
    assert_eq!(
        created.artifact.owner.adapter_session_id,
        write_access.adapter_session_id
    );
    assert!(created
        .artifact
        .workspace_relative_path
        .starts_with("chatos-plugin-artifacts/"));
    assert!(!created.artifact.workspace_relative_path.contains("owner-a"));

    let stale = store
        .update(
            &state,
            &request,
            &grant,
            write_access.clone(),
            created.artifact.artifact_id.as_str(),
            &"0".repeat(64),
            br#"{"version":2}"#,
        )
        .expect_err("stale update must fail");
    assert_eq!(stale.0, 409);

    let updated = store
        .update(
            &state,
            &request,
            &grant,
            write_access.clone(),
            created.artifact.artifact_id.as_str(),
            created.artifact.sha256.as_str(),
            br#"{"version":2}"#,
        )
        .expect("update mutable Artifact");
    assert_eq!(updated.operation, PluginArtifactWriteOperation::Update);
    assert_ne!(updated.artifact.sha256, created.artifact.sha256);
    assert_eq!(
        updated.artifact.producer_tool_name,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE
    );
    drop(store);

    let restored = PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage);
    let restored_grant = restored
        .ui_grant(
            &request,
            &write_access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
        )
        .expect("restore write grant");
    let read = restored
        .read(
            &state,
            &request,
            &restored_grant,
            write_access,
            updated.artifact.artifact_id.as_str(),
            PluginArtifactReadMode::Inline,
        )
        .expect("read restored mutable Artifact");
    assert_eq!(read.artifact, updated.artifact);
    assert_eq!(
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, read.body_base64,)
            .expect("decode updated Artifact"),
        br#"{"version":2}"#
    );
}

#[cfg(unix)]
#[test]
fn rejects_symbolic_links_during_registration() {
    use std::os::unix::fs::symlink;

    let (_temp, state, request, store, _access) = fixture();
    let workspace = &state.workspaces[0].absolute_root;
    fs::write(workspace.join("outside.docx"), b"outside").expect("write source");
    symlink(
        workspace.join("outside.docx"),
        workspace.join("artifacts/link.docx"),
    )
    .expect("create symlink");
    let error = store
        .register_native_outputs(
            &state,
            &request,
            PluginArtifactProducer {
                owner_user_id: "owner-a",
                device_id: "device-a",
                workspace_id: "workspace-a",
                run_id: "run-a",
                plugin_id: "plugin-a",
                release_id: "release-a",
                artifact_sha256: &"a".repeat(64),
                component_key: "documents",
                adapter_session_id: "native-session-a",
                skill_id: "internal_skill_documents",
                tool_name: "create_docx",
            },
            &json!({"target_path": "artifacts/link.docx"}),
            &json!({"created": true, "path": "artifacts/link.docx"}),
        )
        .expect_err("symlink must fail");
    assert!(error.to_string().contains("symbolic link"));
}

#[test]
fn persists_and_restores_exact_artifact_registry_state() {
    let (temp, state, request, _ephemeral_store, access) = fixture();
    let state_path = temp.path().join("state.json");
    let storage = SecureStorage::in_memory("Plugin Artifact registry test");
    let store =
        PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage.clone());
    let mut persisted_access = access.clone();
    store
        .register_ui_grant(test_grant(&mut persisted_access))
        .expect("persist UI grant");
    let workspace = &state.workspaces[0].absolute_root;
    fs::write(
        workspace.join("artifacts/restored.docx"),
        b"restart fixture",
    )
    .expect("write persisted Artifact");
    let descriptors = store
        .register_native_outputs(
            &state,
            &request,
            PluginArtifactProducer {
                owner_user_id: "owner-a",
                device_id: "device-a",
                workspace_id: "workspace-a",
                run_id: "run-a",
                plugin_id: "plugin-a",
                release_id: "release-a",
                artifact_sha256: &"a".repeat(64),
                component_key: "documents",
                adapter_session_id: "native-session-a",
                skill_id: "internal_skill_documents",
                tool_name: "create_docx",
            },
            &json!({"target_path": "artifacts/restored.docx"}),
            &json!({"created": true, "path": "artifacts/restored.docx"}),
        )
        .expect("persist Artifact");
    assert_eq!(descriptors.len(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let registry_path = temp
            .path()
            .join("plugins")
            .join(PLUGIN_ARTIFACT_REGISTRY_FILE_NAME);
        let mode = fs::metadata(registry_path)
            .expect("registry metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    drop(store);

    let restored =
        PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage.clone());
    let grant = restored
        .ui_grant(
            &request,
            &persisted_access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
        )
        .expect("restore UI grant");
    assert_eq!(
        restored
            .list(&grant, persisted_access.clone())
            .expect("list restored Artifacts")
            .artifacts,
        descriptors
    );
    let read = restored
        .read(
            &state,
            &request,
            &grant,
            persisted_access,
            descriptors[0].artifact_id.as_str(),
            PluginArtifactReadMode::Download,
        )
        .expect("read restored Artifact");
    assert_eq!(
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, read.body_base64,)
            .expect("decode restored Artifact"),
        b"restart fixture"
    );
}

#[test]
fn rejects_tampered_persisted_artifact_registry() {
    let (temp, _state, request, _ephemeral_store, access) = fixture();
    let state_path = temp.path().join("state.json");
    let storage = SecureStorage::in_memory("Plugin Artifact registry tamper test");
    let store =
        PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage.clone());
    let mut persisted_access = access.clone();
    store
        .register_ui_grant(test_grant(&mut persisted_access))
        .expect("persist UI grant");
    drop(store);

    let registry_path = temp
        .path()
        .join("plugins")
        .join(PLUGIN_ARTIFACT_REGISTRY_FILE_NAME);
    let original = fs::read_to_string(registry_path.as_path()).expect("read registry");
    let tampered = original.replacen("owner-a", "owner-z", 1);
    assert_ne!(tampered, original);
    fs::write(registry_path.as_path(), tampered).expect("tamper registry");

    let restored = PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage);
    let error = restored
        .ui_grant(
            &request,
            &persisted_access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
        )
        .expect_err("tampered registry must fail closed");
    assert_eq!(error.0, 500);
    assert!(error.1.contains("integrity verification failed"));
}

#[test]
fn expired_ui_grants_are_not_restored() {
    let (temp, _state, request, _ephemeral_store, access) = fixture();
    let state_path = temp.path().join("state.json");
    let storage = SecureStorage::in_memory("Plugin Artifact registry expiry test");
    let persistence =
        PluginArtifactPersistence::open(state_path.as_path(), &storage).expect("persistence");
    let mut expired_access = access.clone();
    let mut expired_grant = test_grant(&mut expired_access);
    expired_grant.expires_at = Utc::now().timestamp() - 1;
    let mut state = PluginArtifactStoreState::default();
    state
        .ui_grants
        .insert(expired_grant.adapter_session_id.clone(), expired_grant);
    persistence.save(&state).expect("persist pruned registry");

    let restored = PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage);
    let error = restored
        .ui_grant(
            &request,
            &expired_access,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
        )
        .expect_err("expired grant must not restore");
    assert_eq!(error.0, 404);
}
