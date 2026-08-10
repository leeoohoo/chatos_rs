// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sha2::{Digest, Sha256};

use serde_json::Value;

use crate::providers::ProviderCallError;
use crate::runtime::PluginToolComponentRuntimeBinding;

pub(in crate::providers::plugin_components) fn component_metadata_text<'a>(
    binding: &'a PluginToolComponentRuntimeBinding,
    key: &str,
) -> Option<&'a str> {
    binding
        .component
        .metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(in crate::providers::plugin_components) fn component_metadata_string_array(
    binding: &PluginToolComponentRuntimeBinding,
    key: &str,
) -> Result<Vec<String>, ProviderCallError> {
    value_string_array(
        binding.component.metadata.get(key),
        format!("Plugin component metadata {key}").as_str(),
    )
}

pub(in crate::providers::plugin_components) fn value_string_array(
    value: Option<&Value>,
    label: &str,
) -> Result<Vec<String>, ProviderCallError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        ProviderCallError::provider_unavailable(format!("{label} must be an array"))
    })?;
    let mut result = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    ProviderCallError::provider_unavailable(format!(
                        "{label} contains an invalid item"
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    result.sort();
    result.dedup();
    Ok(result)
}

pub(in crate::providers::plugin_components) fn normalized_value_text(
    value: Option<&Value>,
) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(in crate::providers::plugin_components) fn required_value_text<'a>(
    value: &'a Value,
    field: &str,
) -> Result<&'a str, ProviderCallError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ProviderCallError::invalid_response(format!(
                "Plugin component response is missing {field}"
            ))
        })
}

pub(in crate::providers::plugin_components) fn sha256_text(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub(in crate::providers::plugin_components) fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
