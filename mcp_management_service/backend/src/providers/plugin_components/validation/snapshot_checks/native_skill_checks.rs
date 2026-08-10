// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::providers::ProviderCallError;
use crate::runtime::PluginToolComponentRuntimeBinding;

use super::super::value_helpers::is_lower_sha256;

pub(in crate::providers::plugin_components) fn validate_native_skill_snapshot(
    immutable: &PluginToolComponentRuntimeBinding,
    native_skill: &Value,
) -> Result<(), ProviderCallError> {
    let expected_metadata = &immutable.component.metadata;
    for (field, expected) in [
        ("plugin_id", immutable.plugin_id.as_str()),
        ("release_id", immutable.release_id.as_str()),
        ("plugin_version", immutable.version.as_str()),
        ("artifact_sha256", immutable.artifact_sha256.as_str()),
        ("component_key", immutable.component.component_key.as_str()),
        ("bundle_hash", immutable.component_content_sha256.as_str()),
    ] {
        if native_skill.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(ProviderCallError::invalid_response(format!(
                "native Plugin Skill response {field} does not match its immutable binding"
            )));
        }
    }
    for field in ["skill_id", "bundle_id"] {
        let expected = expected_metadata
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(format!(
                    "native Plugin Skill metadata is missing {field}"
                ))
            })?;
        if native_skill.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(ProviderCallError::invalid_response(format!(
                "native Plugin Skill response {field} does not match its immutable binding"
            )));
        }
    }
    for field in [
        "skill_snapshot_sha256",
        "snapshot_sha256",
        "tool_snapshot_sha256",
    ] {
        if !native_skill
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(is_lower_sha256)
        {
            return Err(ProviderCallError::invalid_response(format!(
                "native Plugin Skill response is missing valid {field}"
            )));
        }
    }
    let expected_native_snapshot = hex::encode(Sha256::digest(
        format!(
            "chatos.plugin.native-skill.snapshot.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            immutable.plugin_id,
            immutable.release_id,
            immutable.version,
            immutable.artifact_sha256,
            immutable.component.component_key,
            native_skill["skill_snapshot_sha256"]
                .as_str()
                .unwrap_or_default(),
            native_skill["skill_id"].as_str().unwrap_or_default(),
            native_skill["bundle_id"].as_str().unwrap_or_default(),
            native_skill["bundle_version"].as_str().unwrap_or_default(),
            immutable.component_content_sha256,
        )
        .as_bytes(),
    ));
    if native_skill.get("snapshot_sha256").and_then(Value::as_str)
        != Some(expected_native_snapshot.as_str())
    {
        return Err(ProviderCallError::invalid_response(
            "native Plugin Skill binding snapshot hash is invalid",
        ));
    }
    Ok(())
}

pub(in crate::providers::plugin_components) fn validate_native_tool_snapshot_hash(
    native_skill: &Value,
    tools: &[Value],
) -> Result<(), ProviderCallError> {
    let snapshot_sha256 = native_skill
        .get("snapshot_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut payload = format!("chatos.plugin.native-tools.snapshot.v1\n{snapshot_sha256}");
    for tool in tools {
        payload.push('\n');
        payload.push_str(
            serde_json::to_string(tool)
                .map_err(|error| {
                    ProviderCallError::invalid_response(format!(
                        "serialize native Plugin Skill tool snapshot failed: {error}"
                    ))
                })?
                .as_str(),
        );
    }
    let expected = hex::encode(Sha256::digest(payload.as_bytes()));
    if native_skill
        .get("tool_snapshot_sha256")
        .and_then(Value::as_str)
        != Some(expected.as_str())
    {
        return Err(ProviderCallError::invalid_response(
            "native Plugin Skill tool snapshot hash is invalid",
        ));
    }
    Ok(())
}
