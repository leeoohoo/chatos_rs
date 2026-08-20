// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod create;
mod reaper;
mod release;
mod status;

use serde_json::{json, Value};

use crate::sandbox::types::LocalSandboxLease;

pub(crate) use create::create_local_sandbox_lease;
pub(crate) use reaper::{local_sandbox_lease_expired, spawn_local_sandbox_lease_reaper};
pub(crate) use release::{release_local_sandbox, shutdown_local_sandboxes};
pub(crate) use status::{get_local_sandbox, health_local_sandbox};

fn cloud_safe_local_sandbox_lease(lease: &LocalSandboxLease) -> Value {
    let mut value = json!(lease);
    let Some(object) = value.as_object_mut() else {
        return Value::Null;
    };
    object.insert("lease_id".to_string(), Value::String(lease.id.clone()));
    object.insert(
        "workspace_root".to_string(),
        Value::String("local-sandbox://workspace".to_string()),
    );
    object.insert(
        "run_workspace".to_string(),
        Value::String(format!("local-sandbox://{}/workspace", lease.sandbox_id)),
    );
    object.insert("backend_id".to_string(), Value::Null);
    object.insert("agent_endpoint".to_string(), Value::Null);
    object.insert("agent_token".to_string(), Value::Null);
    redact_absolute_local_paths(&mut value);
    value
}

fn cloud_safe_effective_policy(lease: &LocalSandboxLease) -> Value {
    let mut value = json!(lease.effective_policy);
    redact_absolute_local_paths(&mut value);
    value
}

fn cloud_safe_effective_permissions(lease: &LocalSandboxLease) -> Value {
    let mut value = json!(lease.effective_permissions);
    redact_absolute_local_paths(&mut value);
    value
}

#[cfg(test)]
fn redact_local_output_manifest(mut manifest: Value) -> Value {
    if let Some(object) = manifest.as_object_mut() {
        object.insert("output_workspace".to_string(), Value::Null);
        object.insert("manifest_path".to_string(), Value::Null);
        if let Some(counts) = object.get_mut("counts").and_then(Value::as_object_mut) {
            counts.insert("diff_available".to_string(), Value::from(0));
        }
        if let Some(files) = object.get_mut("files").and_then(Value::as_array_mut) {
            for file in files {
                if let Some(file) = file.as_object_mut() {
                    file.insert("diff_available".to_string(), Value::Bool(false));
                    file.insert("diff_ref".to_string(), Value::Null);
                }
            }
        }
    }
    redact_absolute_local_paths(&mut manifest);
    manifest
}

fn redact_absolute_local_paths(value: &mut Value) {
    match value {
        Value::String(text) if looks_like_absolute_path(text) => {
            *text = "local-sandbox://redacted".to_string();
        }
        Value::Array(items) => {
            for item in items {
                redact_absolute_local_paths(item);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                redact_absolute_local_paths(value);
            }
        }
        _ => {}
    }
}

fn looks_like_absolute_path(value: &str) -> bool {
    let value = value.trim();
    value.starts_with('/')
        || value.starts_with('\\')
        || value.starts_with("file://")
        || (value.as_bytes().get(1) == Some(&b':')
            && value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic))
}

#[cfg(test)]
mod cloud_boundary_tests {
    use super::*;

    #[test]
    fn absolute_paths_are_redacted_before_crossing_the_connector_boundary() {
        let mut value = json!({
            "unix": "/Users/example/workspace",
            "windows": "C:\\Users\\example\\workspace",
            "relative": "apps/backend",
        });
        redact_absolute_local_paths(&mut value);

        assert_eq!(value["unix"], "local-sandbox://redacted");
        assert_eq!(value["windows"], "local-sandbox://redacted");
        assert_eq!(value["relative"], "apps/backend");
    }

    #[test]
    fn local_output_manifest_never_exports_host_paths_or_diff_handles() {
        let manifest = redact_local_output_manifest(json!({
            "output_workspace": "/Users/example/workspace",
            "manifest_path": "/Users/example/manifest.json",
            "counts": {"diff_available": 1},
            "files": [{
                "path": "src/main.rs",
                "diff_available": true,
                "diff_ref": "/Users/example/diffs/main.diff"
            }]
        }));

        assert_eq!(manifest["output_workspace"], Value::Null);
        assert_eq!(manifest["manifest_path"], Value::Null);
        assert_eq!(manifest["counts"]["diff_available"], 0);
        assert_eq!(manifest["files"][0]["diff_available"], false);
        assert_eq!(manifest["files"][0]["diff_ref"], Value::Null);
    }
}
