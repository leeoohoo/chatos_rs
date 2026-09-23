// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde_json::Value;

use super::{
    NodePackageManifestEvidence, NodeSupplyChainPolicy, PackageManifestSessionEvent,
    SupplyChainEvidenceState,
};

pub(super) fn package_manifest_event_from_tool_call(
    name: &str,
    arguments: &Value,
) -> Option<PackageManifestSessionEvent> {
    let session_id = arguments.get("session_id").and_then(Value::as_str)?;
    if name.ends_with("commit_edit_session") {
        return Some(PackageManifestSessionEvent::Commit {
            session_id: session_id.to_string(),
        });
    }
    if name.ends_with("abort_edit_session") {
        return Some(PackageManifestSessionEvent::Abort {
            session_id: session_id.to_string(),
        });
    }
    if !name.ends_with("stage_edit_batch") {
        return None;
    }
    let operations = arguments.get("operations").and_then(Value::as_array)?;
    let mut touched = false;
    let mut update = None;
    for operation in operations {
        let path = operation.get("path").and_then(Value::as_str)?;
        if !path.replace('\\', "/").ends_with("package.json") {
            continue;
        }
        touched = true;
        update = if operation.get("kind").and_then(Value::as_str) == Some("write") {
            operation
                .get("content")
                .and_then(Value::as_str)
                .and_then(parse_package_manifest)
        } else {
            None
        };
    }
    touched.then(|| PackageManifestSessionEvent::Stage {
        session_id: session_id.to_string(),
        update,
    })
}

pub(super) fn result_mutates_package_manifest(payload: &Value) -> bool {
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    name.ends_with("commit_edit_session") && value_mentions_package_manifest(payload)
}

pub(super) fn result_mutates_node_dependency_files(payload: &Value) -> bool {
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    name.ends_with("commit_edit_session") && value_mentions_node_dependency_file(payload)
}

pub(super) fn value_mentions_node_dependency_file(value: &Value) -> bool {
    match value {
        Value::String(value) => {
            let normalized = value.replace('\\', "/").to_ascii_lowercase();
            normalized.ends_with("package.json")
                || [
                    "package-lock.json",
                    "pnpm-lock.yaml",
                    "yarn.lock",
                    "bun.lock",
                    "bun.lockb",
                ]
                .iter()
                .any(|lockfile| normalized.ends_with(lockfile))
        }
        Value::Array(items) => items.iter().any(value_mentions_node_dependency_file),
        Value::Object(map) => map.values().any(value_mentions_node_dependency_file),
        _ => false,
    }
}

pub(super) fn value_mentions_package_manifest(value: &Value) -> bool {
    match value {
        Value::String(value) => value.replace('\\', "/").contains("package.json"),
        Value::Array(items) => items.iter().any(value_mentions_package_manifest),
        Value::Object(map) => map.values().any(value_mentions_package_manifest),
        _ => false,
    }
}

pub(super) fn package_manifest_from_tool_result(
    payload: &Value,
) -> Option<NodePackageManifestEvidence> {
    // MCP tool results carry the structured payload in `result` and expose a
    // JSON/text rendering in `content`.  The latter was the only shape handled
    // here, so real CodeMaintainer reads were visible in the event stream but
    // never populated the final package manifest evidence.
    let mut candidates = Vec::new();
    for key in ["result", "structured_result"] {
        if let Some(value) = payload.get(key) {
            candidates.push(chatos_mcp_runtime::structured_result_payload(value).clone());
        }
    }
    if let Some(content) = payload.get("content").and_then(Value::as_str) {
        if let Ok(value) = serde_json::from_str::<Value>(content) {
            candidates.push(chatos_mcp_runtime::structured_result_payload(&value).clone());
        }
    }

    candidates.into_iter().find_map(|result| {
        let path = result.get("path").and_then(Value::as_str)?;
        if !path.replace('\\', "/").ends_with("package.json") {
            return None;
        }
        parse_package_manifest(result.get("content")?.as_str()?)
    })
}

pub(super) fn parse_package_manifest(content: &str) -> Option<NodePackageManifestEvidence> {
    let manifest = serde_json::from_str::<Value>(content).ok()?;
    let mut requirements = BTreeMap::new();
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        let Some(entries) = manifest.get(section).and_then(Value::as_object) else {
            continue;
        };
        for (name, requirement) in entries {
            let requirement = requirement.as_str()?.trim();
            if name.trim().is_empty() || requirement.is_empty() {
                return None;
            }
            requirements.insert(name.to_string(), requirement.to_string());
        }
    }
    Some(NodePackageManifestEvidence { requirements })
}

pub(super) fn dependency_baseline_violations(
    manifest: &NodePackageManifestEvidence,
    policy: &NodeSupplyChainPolicy,
) -> Vec<String> {
    manifest
        .requirements
        .iter()
        .filter_map(|(name, actual)| {
            let expected = policy.dependency_requirements.get(name)?;
            (actual != expected)
                .then(|| format!("{name} requires `{actual}` but baseline requires `{expected}`"))
        })
        .collect()
}

pub(super) fn observe_project_paths(value: &Value, evidence: &mut SupplyChainEvidenceState) {
    match value {
        Value::String(value) => {
            let normalized = value.replace('\\', "/").to_ascii_lowercase();
            if normalized.ends_with("package.json") {
                evidence.node_project_observed = true;
            }
            if [
                "package-lock.json",
                "pnpm-lock.yaml",
                "yarn.lock",
                "bun.lock",
                "bun.lockb",
            ]
            .iter()
            .any(|lockfile| normalized.ends_with(lockfile))
            {
                evidence.node_project_observed = true;
                evidence.lockfile_observed = true;
            }
        }
        Value::Array(items) => {
            for item in items {
                observe_project_paths(item, evidence);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                observe_project_paths(item, evidence);
            }
        }
        _ => {}
    }
}
