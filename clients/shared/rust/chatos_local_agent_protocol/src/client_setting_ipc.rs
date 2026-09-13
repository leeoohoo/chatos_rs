// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{require_bounded_json, require_identifier, ProtocolError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalClientSettingSnapshot {
    pub key: String,
    pub owner_user_id: String,
    pub value: Value,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalClientSettingSnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_client_setting_key(&self.key)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        require_bounded_json("client_setting_value", &self.value)?;
        if self.revision == 0 || self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "client setting revision and timestamps are invalid",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetClientSettingCommand {
    pub key: String,
}

impl GetClientSettingCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_client_setting_key(&self.key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PutClientSettingCommand {
    pub key: String,
    pub expected_revision: Option<u64>,
    pub value: Value,
}

impl PutClientSettingCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_client_setting_key(&self.key)?;
        if self.expected_revision == Some(0) {
            return Err(ProtocolError::InvalidState {
                reason: "client setting expected_revision must be positive",
            });
        }
        require_bounded_json("client_setting_value", &self.value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeleteClientSettingCommand {
    pub key: String,
    pub expected_revision: u64,
}

impl DeleteClientSettingCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_client_setting_key(&self.key)?;
        if self.expected_revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "client setting expected_revision must be positive",
            });
        }
        Ok(())
    }
}

fn validate_client_setting_key(value: &str) -> Result<(), ProtocolError> {
    require_identifier("client_setting_key", value)?;
    if value.trim() != value
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'_' | b'-' | b'.' | b':')
        })
    {
        return Err(ProtocolError::InvalidState {
            reason: "client setting key is not canonical",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_keys_are_canonical_and_values_are_bounded() {
        assert!(validate_client_setting_key("project_run.preferences").is_ok());
        for invalid in ["", " Project", "Project", "project/run", "项目"] {
            assert!(validate_client_setting_key(invalid).is_err());
        }
        let oversized = PutClientSettingCommand {
            key: "oversized".to_string(),
            expected_revision: None,
            value: Value::String("x".repeat(crate::MAX_BOUNDED_JSON_BYTES)),
        };
        assert!(oversized.validate().is_err());
    }
}
