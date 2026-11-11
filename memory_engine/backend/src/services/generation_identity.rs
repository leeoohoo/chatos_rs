// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sha2::{Digest, Sha256};

pub(crate) const CANONICALIZER_REVISION: &str = "canonical-json-v1";
pub(crate) const TOKENIZER_ID: &str = "memory-estimator-chars-v1";

pub(crate) fn sha256_json(value: &serde_json::Value) -> Result<String, String> {
    let bytes =
        serde_json::to_vec(&canonicalize(value.clone())).map_err(|error| error.to_string())?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

pub(crate) fn input_manifest(
    inputs: Vec<serde_json::Value>,
) -> Result<(serde_json::Value, String), String> {
    if inputs.is_empty() {
        return Err("generation input manifest cannot be empty".to_string());
    }
    let mut normalized = Vec::with_capacity(inputs.len());
    for (ordinal, input) in inputs.into_iter().enumerate() {
        let mut input = input
            .as_object()
            .cloned()
            .ok_or_else(|| "generation input identity must be an object".to_string())?;
        input.insert("ordinal".to_string(), serde_json::json!(ordinal));
        let identity = serde_json::Value::Object(input.clone());
        input.insert(
            "digest".to_string(),
            serde_json::json!(sha256_json(&identity)?),
        );
        normalized.push(serde_json::Value::Object(input));
    }
    let manifest = serde_json::json!({
        "schema_version": "memory_generation_input_v1",
        "digest_algorithm": "sha256",
        "canonicalizer_revision": CANONICALIZER_REVISION,
        "inputs": normalized,
    });
    let digest = sha256_json(&manifest)?;
    Ok((manifest, digest))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn descriptor(
    generation_step_key: &str,
    generation_kind: &str,
    generation_profile: &str,
    pipeline_revision: &str,
    project_partition_key: Option<&str>,
    input_manifest: serde_json::Value,
    input_digest: String,
    policy_snapshot: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "generation_step_key": generation_step_key,
        "generation_kind": generation_kind,
        "generation_profile": generation_profile,
        "pipeline_revision": pipeline_revision,
        "project_partition_key": project_partition_key,
        "input_manifest": input_manifest,
        "input_digest": input_digest,
        "policy_snapshot": policy_snapshot,
    })
}

fn canonicalize(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonicalize).collect())
        }
        serde_json::Value::Object(values) => {
            let mut sorted = std::collections::BTreeMap::new();
            for (key, value) in values {
                sorted.insert(key, canonicalize(value));
            }
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_digest_is_stable_and_lowercase_sha256() {
        let left = serde_json::json!({"b": 2, "a": {"d": 4, "c": 3}});
        let right = serde_json::json!({"a": {"c": 3, "d": 4}, "b": 2});
        let digest = sha256_json(&left).unwrap();
        assert_eq!(digest, sha256_json(&right).unwrap());
        assert_eq!(digest.len(), 71);
        assert!(digest
            .strip_prefix("sha256:")
            .unwrap()
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    }
}
