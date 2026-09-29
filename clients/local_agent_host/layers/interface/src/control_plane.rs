// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_CAPABILITY_INSTRUCTIONS_BYTES: usize = 256 * 1024;
pub const MAX_CAPABILITY_ITEMS: usize = 256;
pub const MAX_CONTROL_PLANE_SNAPSHOT_BYTES: usize = 1024 * 1024;

/// Immutable, non-secret capability revision cached by the client. Model
/// credentials and executable runtime objects deliberately do not belong in
/// this record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalCapabilityPolicySnapshot {
    pub owner_user_id: String,
    pub profile_key: String,
    pub capability_policy_revision: String,
    pub instructions: Option<String>,
    pub prefixed_input_items: Vec<Value>,
    pub tools: Vec<Value>,
}

impl LocalCapabilityPolicySnapshot {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("profile_key", &self.profile_key)?;
        validate_identifier(
            "capability_policy_revision",
            &self.capability_policy_revision,
        )?;
        validate_instructions(self.instructions.as_deref())?;
        if self.prefixed_input_items.len() > MAX_CAPABILITY_ITEMS {
            return Err(format!(
                "prefixed_input_items must contain at most {MAX_CAPABILITY_ITEMS} entries"
            ));
        }
        if self.tools.len() > MAX_CAPABILITY_ITEMS {
            return Err(format!(
                "tools must contain at most {MAX_CAPABILITY_ITEMS} entries"
            ));
        }
        validate_encoded_size("capability snapshot", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalJsonSchemaOutputFormat {
    pub name: String,
    pub description: Option<String>,
    pub schema: Value,
    pub strict: bool,
}

/// Durable model configuration metadata. This protocol type structurally cannot carry
/// an API key: only a reference resolvable by Keychain/Credential Manager is
/// persisted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalModelConfigSnapshot {
    pub owner_user_id: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub credential_ref: String,
    pub base_url: String,
    pub model: String,
    pub provider: String,
    pub supports_responses: bool,
    pub supports_images: Option<bool>,
    pub instructions: Option<String>,
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_level: Option<String>,
    pub include_prompt_cache_retention: bool,
    pub request_body_limit_bytes: Option<u64>,
    pub max_transient_retries: Option<u32>,
    pub output_format: Option<LocalJsonSchemaOutputFormat>,
}

impl LocalModelConfigSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("model_config_ref", &self.model_config_ref)?;
        validate_identifier("model_config_revision", &self.model_config_revision)?;
        validate_text("credential_ref", &self.credential_ref, 512)?;
        validate_text("base_url", &self.base_url, 4096)?;
        if !(self.base_url.starts_with("http://") || self.base_url.starts_with("https://")) {
            return Err("base_url must use http or https".to_string());
        }
        validate_identifier("model", &self.model)?;
        validate_identifier("provider", &self.provider)?;
        validate_instructions(self.instructions.as_deref())?;
        if self.temperature.is_some_and(|value| !value.is_finite()) {
            return Err("temperature must be finite".to_string());
        }
        if self.max_output_tokens.is_some_and(|value| value <= 0) {
            return Err("max_output_tokens must be greater than zero".to_string());
        }
        if let Some(level) = self.thinking_level.as_deref() {
            validate_text("thinking_level", level, 64)?;
        }
        if self
            .request_body_limit_bytes
            .is_some_and(|value| value == 0 || value > 64 * 1024 * 1024)
        {
            return Err("request_body_limit_bytes must be between 1 and 67108864".to_string());
        }
        if self.max_transient_retries.is_some_and(|value| value > 20) {
            return Err("max_transient_retries must be at most 20".to_string());
        }
        if let Some(format) = &self.output_format {
            validate_text("output_format.name", &format.name, 128)?;
            if let Some(description) = format.description.as_deref() {
                validate_text("output_format.description", description, 4096)?;
            }
        }
        validate_encoded_size("model config snapshot", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PutModelConfigSnapshotCommand {
    pub snapshot: LocalModelConfigSnapshot,
}

impl PutModelConfigSnapshotCommand {
    pub fn validate(&self) -> Result<(), String> {
        self.snapshot.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetModelConfigSnapshotCommand {
    pub owner_user_id: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
}

impl GetModelConfigSnapshotCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("model_config_ref", &self.model_config_ref)?;
        validate_identifier("model_config_revision", &self.model_config_revision)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PutCapabilityPolicySnapshotCommand {
    pub snapshot: LocalCapabilityPolicySnapshot,
}

impl PutCapabilityPolicySnapshotCommand {
    pub fn validate(&self) -> Result<(), String> {
        self.snapshot.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetCapabilityPolicySnapshotCommand {
    pub owner_user_id: String,
    pub profile_key: String,
    pub capability_policy_revision: String,
}

impl GetCapabilityPolicySnapshotCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("profile_key", &self.profile_key)?;
        validate_identifier(
            "capability_policy_revision",
            &self.capability_policy_revision,
        )
    }
}

fn validate_identifier(name: &str, value: &str) -> Result<(), String> {
    validate_text(name, value, 256)
}

fn validate_text(name: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(format!(
            "{name} must be 1..={max_bytes} bytes without control characters"
        ));
    }
    Ok(())
}

fn validate_instructions(value: Option<&str>) -> Result<(), String> {
    if value.is_some_and(|value| value.len() > MAX_CAPABILITY_INSTRUCTIONS_BYTES) {
        return Err(format!(
            "instructions must be at most {MAX_CAPABILITY_INSTRUCTIONS_BYTES} bytes"
        ));
    }
    Ok(())
}

fn validate_encoded_size<T: Serialize>(name: &str, value: &T) -> Result<(), String> {
    let encoded = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if encoded.len() > MAX_CONTROL_PLANE_SNAPSHOT_BYTES {
        return Err(format!(
            "{name} must be at most {MAX_CONTROL_PLANE_SNAPSHOT_BYTES} bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_snapshot_contains_reference_but_has_no_api_key_field() {
        let snapshot = LocalModelConfigSnapshot {
            owner_user_id: "user-1".to_string(),
            model_config_ref: "default".to_string(),
            model_config_revision: "revision-1".to_string(),
            credential_ref: "keychain:model/default".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            model: "example-model".to_string(),
            provider: "openai".to_string(),
            supports_responses: true,
            supports_images: Some(true),
            instructions: None,
            temperature: None,
            max_output_tokens: Some(4096),
            thinking_level: Some("high".to_string()),
            include_prompt_cache_retention: true,
            request_body_limit_bytes: Some(1024 * 1024),
            max_transient_retries: Some(5),
            output_format: None,
        };
        snapshot.validate().expect("valid snapshot");
        let encoded = serde_json::to_value(snapshot).expect("serialize");
        assert_eq!(encoded["owner_user_id"], "user-1");
        assert_eq!(encoded["credential_ref"], "keychain:model/default");
        assert!(encoded.get("api_key").is_none());
    }
}
