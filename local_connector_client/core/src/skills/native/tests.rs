// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::WorkspaceState;
use std::collections::BTreeMap;

#[test]
fn validates_skill_manifest() {
    let result = validate_skill_bundle_manifest(&json!({
        "manifest": {
            "bundle_id": "chatos.internal.demo-skill",
            "skill_id": "internal_skill_demo",
            "name": "demo-skill",
            "version": "1.0.0",
            "entrypoint": {"kind": "native_adapter"}
        }
    }))
    .expect("manifest");
    assert_eq!(result.get("valid").and_then(Value::as_bool), Some(true));
}

#[test]
fn visualization_is_written_inside_workspace() {
    let root = std::env::temp_dir().join(format!("chatos-skill-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("workspace");
    let state = LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-1".to_string(),
            absolute_root: root.clone(),
            alias: "test".to_string(),
            fingerprint: "fp".to_string(),
            project_config_trust: None,
        }],
        ..LocalState::default()
    };
    let request = RelayRequest {
        _message_type: "skill_execute_request".to_string(),
        request_id: "request-1".to_string(),
        owner_user_id: Some("owner-1".to_string()),
        device_id: Some("device-1".to_string()),
        workspace_id: "workspace-1".to_string(),
        method: Some("POST".to_string()),
        path: Some("/skills/execute".to_string()),
        headers: BTreeMap::new(),
        body: Value::Null,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let result = write_visualization_html(
        &json!({
            "target_path": "artifacts/demo.html",
            "title": "Demo",
            "body_html": "<main>ok</main>"
        }),
        &state,
        &request,
    )
    .expect("visualization");
    assert_eq!(result.get("created").and_then(Value::as_bool), Some(true));
    let output = fs::read_to_string(root.join("artifacts/demo.html")).expect("output");
    assert!(output.contains("connect-src 'none'"));
    let _ = fs::remove_dir_all(root);
}
