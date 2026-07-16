// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use uuid::Uuid;

use super::*;
use crate::WorkspaceState;

fn test_context() -> (PathBuf, LocalState, RelayRequest) {
    let root = std::env::temp_dir().join(format!("chatos-artifact-test-{}", Uuid::new_v4()));
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
    };
    (root, state, request)
}

#[test]
fn creates_and_inspects_office_artifacts_locally() {
    let (root, state, request) = test_context();
    create_docx(
        &json!({"target_path":"artifacts/demo.docx","title":"Demo","paragraphs":["First paragraph","Second paragraph"]}),
        &state,
        &request,
    )
    .expect("docx");
    let docx = inspect_docx(&json!({"path":"artifacts/demo.docx"}), &state, &request)
        .expect("inspect docx");
    assert!(docx
        .get("text_preview")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("First paragraph")));

    create_xlsx(
        &json!({"target_path":"artifacts/demo.xlsx","sheet_name":"Data","rows":[["Name","Count"],["Apple",3]]}),
        &state,
        &request,
    )
    .expect("xlsx");
    let xlsx = inspect_spreadsheet(&json!({"path":"artifacts/demo.xlsx"}), &state, &request)
        .expect("inspect xlsx");
    assert_eq!(xlsx.get("worksheets").and_then(Value::as_u64), Some(1));

    create_pptx(
        &json!({"target_path":"artifacts/demo.pptx","slides":[{"title":"Demo","body":"Generated locally"}]}),
        &state,
        &request,
    )
    .expect("pptx");
    let pptx = inspect_pptx(&json!({"path":"artifacts/demo.pptx"}), &state, &request)
        .expect("inspect pptx");
    assert_eq!(pptx.get("slides").and_then(Value::as_u64), Some(1));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn artifact_template_verifies_and_instantiates_local_source() {
    let (root, state, request) = test_context();
    create_csv(
        &json!({"target_path":"artifacts/source.csv","rows":[["a","b"],[1,2]]}),
        &state,
        &request,
    )
    .expect("csv");
    create_artifact_template(
        &json!({
            "source_path":"artifacts/source.csv",
            "target_directory":"templates/demo",
            "template_name":"Demo"
        }),
        &state,
        &request,
    )
    .expect("template");
    let inspected = inspect_artifact_template(
        &json!({"template_directory":"templates/demo"}),
        &state,
        &request,
    )
    .expect("inspect template");
    assert_eq!(
        inspected.get("hash_valid").and_then(Value::as_bool),
        Some(true)
    );
    instantiate_artifact_template(
        &json!({"template_directory":"templates/demo","target_path":"artifacts/copy.csv"}),
        &state,
        &request,
    )
    .expect("instantiate");
    assert!(root.join("artifacts/copy.csv").is_file());
    let _ = fs::remove_dir_all(root);
}
