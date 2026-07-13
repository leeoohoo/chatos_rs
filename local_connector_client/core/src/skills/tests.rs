// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
use super::*;
use crate::WorkspaceState;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use uuid::Uuid;

#[test]
fn embedded_catalog_contains_all_expected_skills() {
    let catalog = internal_skill_catalog().expect("catalog");
    assert_eq!(catalog.skills.len(), 27);
    assert_eq!(
        catalog
            .skills
            .iter()
            .filter(|item| item.implementation_status == "ready")
            .count(),
        12
    );
    assert!(catalog.skills.iter().all(|item| {
        !item.name.trim().is_empty()
            && !item.description.trim().is_empty()
            && !item.category.trim().is_empty()
    }));
}

#[test]
fn inventory_never_reports_planned_adapter_as_available() {
    let inventory = local_skill_inventory().expect("inventory");
    assert_eq!(inventory.len(), 27);
    let available_count = inventory
        .iter()
        .filter(|item| item.status == "available")
        .count();
    assert!((11..=12).contains(&available_count));
    let ready_ids = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .filter(|item| item.implementation_status == "ready")
        .map(|item| item.skill_id)
        .collect::<HashSet<_>>();
    assert!(inventory
        .iter()
        .all(|item| ready_ids.contains(item.skill_id.as_str()) || item.status != "available"));
    assert!(inventory
        .iter()
        .filter(|item| item.status == "available")
        .all(|item| item.dependency_status == "available"));
    assert!(inventory.iter().all(|item| matches!(
        item.dependency_status.as_str(),
        "available" | "missing" | "unsupported" | "error"
    )));
}

#[test]
fn ready_bundle_v2_fingerprint_matches_plugin_management_seed() {
    let catalog = internal_skill_catalog().expect("catalog");
    let rows = catalog
        .skills
        .iter()
        .filter(|item| item.implementation_status == "ready")
        .map(|item| format!("{}:{}", item.skill_id, internal_skill_bundle_hash(item)))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        hex::encode(Sha256::digest(rows.as_bytes())),
        "91dcdc4f36bfa4aa3f7e56d9f9d2c62fe299d2f1175c373fbbe2cdc05168ecee"
    );
}

#[test]
fn all_27_bundled_skill_fingerprints_match_plugin_management_seed() {
    let catalog = internal_skill_catalog().expect("catalog");
    let rows = catalog
        .skills
        .iter()
        .map(|item| format!("{}:{}", item.skill_id, internal_skill_bundle_hash(item)))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        hex::encode(Sha256::digest(rows.as_bytes())),
        "444a397c67701aec2fab0d8ba34bee950f802c84934ce8f2b9718554be7279d2"
    );
}

#[test]
fn ready_skill_prepare_returns_local_instructions() {
    let item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_remotion")
        .expect("remotion");
    let request = json!({
        "type": "skill_prepare_request",
        "request_id": "request-1",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "",
        "body": {
            "skill_id": item.skill_id,
            "bundle_id": item.bundle_id,
            "version": item.version,
            "bundle_hash": internal_skill_bundle_hash(&item),
        }
    });
    let response = handle_skill_prepare(request, &LocalState::default());
    assert_eq!(response.get("status").and_then(Value::as_u64), Some(200));
    assert!(response
        .pointer("/body/instructions")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("Remotion")));
}

#[test]
fn native_skill_execute_requires_prepared_snapshot_and_writes_locally() {
    let root = std::env::temp_dir().join(format!("chatos-skill-e2e-{}", Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("workspace");
    let state = LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-1".to_string(),
            absolute_root: root.clone(),
            alias: "test".to_string(),
            fingerprint: "fp".to_string(),
        }],
        ..LocalState::default()
    };
    let item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_visualize")
        .expect("visualize");
    let bundle_hash = internal_skill_bundle_hash(&item);
    let prepare = handle_skill_prepare(
        json!({
            "type": "skill_prepare_request",
            "request_id": "prepare-1",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
            }
        }),
        &state,
    );
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    let adapter_session_id = prepare
        .pointer("/body/adapter_session_id")
        .and_then(Value::as_str)
        .expect("adapter session");
    let execute = handle_skill_execute(
        json!({
            "type": "skill_execute_request",
            "request_id": "execute-1",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
                "adapter_session_id": adapter_session_id,
                "operation": "write_visualization_html",
                "arguments": {
                    "target_path": "artifacts/e2e.html",
                    "title": "E2E",
                    "body_html": "<main>ready</main>"
                }
            }
        }),
        &state,
    );
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert!(root.join("artifacts/e2e.html").is_file());
    let cancel = handle_skill_cancel(json!({
        "type": "skill_cancel_request",
        "request_id": "cancel-1",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "workspace-1",
        "body": {
            "skill_id": item.skill_id,
            "bundle_id": item.bundle_id,
            "version": item.version,
            "bundle_hash": bundle_hash,
            "adapter_session_id": adapter_session_id,
        }
    }));
    assert_eq!(cancel.get("status").and_then(Value::as_u64), Some(200));
    assert_eq!(
        cancel.pointer("/body/cancelled").and_then(Value::as_bool),
        Some(true)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn document_skill_prepare_publishes_and_executes_native_tools() {
    let root = std::env::temp_dir().join(format!("chatos-document-e2e-{}", Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("workspace");
    let state = LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-1".to_string(),
            absolute_root: root.clone(),
            alias: "test".to_string(),
            fingerprint: "fp".to_string(),
        }],
        ..LocalState::default()
    };
    let item = internal_skill_catalog()
        .expect("catalog")
        .skills
        .into_iter()
        .find(|item| item.skill_id == "internal_skill_documents")
        .expect("documents");
    let bundle_hash = internal_skill_bundle_hash(&item);
    let prepare = handle_skill_prepare(
        json!({
            "type": "skill_prepare_request",
            "request_id": "prepare-documents",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
            }
        }),
        &state,
    );
    assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
    assert!(prepare
        .pointer("/body/tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| tools
            .iter()
            .any(|tool| { tool.get("name").and_then(Value::as_str) == Some("create_docx") })));
    let adapter_session_id = prepare
        .pointer("/body/adapter_session_id")
        .and_then(Value::as_str)
        .expect("adapter session");
    let execute = handle_skill_execute(
        json!({
            "type": "skill_execute_request",
            "request_id": "execute-documents",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
                "adapter_session_id": adapter_session_id,
                "operation": "create_docx",
                "arguments": {
                    "target_path": "artifacts/document.docx",
                    "title": "本机文档",
                    "paragraphs": ["由 Local Connector 创建。"]
                }
            }
        }),
        &state,
    );
    assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
    assert!(root.join("artifacts/document.docx").is_file());
    let cancel = handle_skill_cancel(json!({
        "type": "skill_cancel_request",
        "request_id": "cancel-documents",
        "owner_user_id": "owner-1",
        "device_id": "device-1",
        "workspace_id": "workspace-1",
        "body": {
            "skill_id": item.skill_id,
            "bundle_id": item.bundle_id,
            "version": item.version,
            "bundle_hash": bundle_hash,
            "adapter_session_id": adapter_session_id,
        }
    }));
    assert_eq!(
        cancel.pointer("/body/cancelled").and_then(Value::as_bool),
        Some(true)
    );
    let _ = fs::remove_dir_all(root);
}
