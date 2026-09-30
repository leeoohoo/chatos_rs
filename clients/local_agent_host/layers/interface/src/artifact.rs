// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text};
use serde::{Deserialize, Serialize};

pub const LOCAL_AGENT_ARTIFACT_MAX_BYTES: usize = 2 * 1_024 * 1_024;
pub const LOCAL_AGENT_ARTIFACT_MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateArtifactCommand {
    pub owner_user_id: String,
    pub name: String,
    pub mime_type: String,
    pub data_base64: String,
    pub sha256: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListArtifactsCommand {
    pub owner_user_id: String,
    pub limit: u32,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetArtifactDataCommand {
    pub owner_user_id: String,
    pub artifact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteArtifactCommand {
    pub owner_user_id: String,
    pub artifact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalAgentArtifact {
    pub artifact_id: String,
    pub owner_user_id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub sha256: String,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalAgentArtifactPage {
    pub artifacts: Vec<LocalAgentArtifact>,
    pub next_cursor: Option<String>,
}

impl CreateArtifactCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_text("name", &self.name, 1_000)?;
        validate_text("mime_type", &self.mime_type, 255)?;
        validate_sha256(&self.sha256)?;
        validate_identifier("idempotency_key", &self.idempotency_key)?;
        let maximum_base64 = LOCAL_AGENT_ARTIFACT_MAX_BYTES.div_ceil(3) * 4;
        if self.data_base64.is_empty() || self.data_base64.len() > maximum_base64 {
            return Err(format!(
                "artifact data must contain at most {LOCAL_AGENT_ARTIFACT_MAX_BYTES} decoded bytes"
            ));
        }
        Ok(())
    }
}

impl ListArtifactsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        if !(1..=LOCAL_AGENT_ARTIFACT_MAX_PAGE_SIZE).contains(&self.limit) {
            return Err(format!(
                "limit must be between 1 and {LOCAL_AGENT_ARTIFACT_MAX_PAGE_SIZE}"
            ));
        }
        if let Some(cursor) = self.cursor.as_deref() {
            validate_text("cursor", cursor, 300)?;
        }
        Ok(())
    }
}

impl GetArtifactDataCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner_and_artifact(&self.owner_user_id, &self.artifact_id)
    }
}

impl DeleteArtifactCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner_and_artifact(&self.owner_user_id, &self.artifact_id)
    }
}

fn validate_owner_and_artifact(owner_user_id: &str, artifact_id: &str) -> Result<(), String> {
    validate_identifier("owner_user_id", owner_user_id)?;
    validate_identifier("artifact_id", artifact_id)
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("sha256 must contain exactly 64 hexadecimal characters".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_artifact_payloads_and_invalid_hashes() {
        let mut command = CreateArtifactCommand {
            owner_user_id: "owner-1".to_string(),
            name: "report.md".to_string(),
            mime_type: "text/markdown".to_string(),
            data_base64: "YQ==".to_string(),
            sha256: "a".repeat(64),
            idempotency_key: "attachment-1".to_string(),
        };
        assert!(command.validate().is_ok());
        command.sha256 = "bad".to_string();
        assert!(command.validate().is_err());
        command.sha256 = "a".repeat(64);
        command.data_base64 = "a".repeat(LOCAL_AGENT_ARTIFACT_MAX_BYTES.div_ceil(3) * 4 + 1);
        assert!(command.validate().is_err());
    }
}
