// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::{
    clipboard_ipc::validate_payload_reference, require_digest, require_identifier,
    require_nonempty_bounded_text, ProtocolError,
};

const MAXIMUM_MEDIA_PROMPT_BYTES: usize = 32 * 1024;
const MAXIMUM_MEDIA_MODEL_BYTES: usize = 1024;
const MAXIMUM_MEDIA_MIME_BYTES: usize = 256;
const MAXIMUM_MEDIA_REVISED_PROMPT_BYTES: usize = 32 * 1024;
const MAXIMUM_MEDIA_ASSETS: usize = 8;
const MAXIMUM_DISCARDED_REFERENCES: usize = 16;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalMediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalMediaStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalMediaAsset {
    pub asset_id: String,
    pub mime_type: String,
    pub payload_reference: String,
    pub content_hash: String,
    pub byte_count: u64,
    pub revised_prompt: Option<String>,
}

impl LocalMediaAsset {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("asset_id", &self.asset_id)?;
        require_nonempty_bounded_text("mime_type", &self.mime_type, MAXIMUM_MEDIA_MIME_BYTES)?;
        require_digest("content_hash", &self.content_hash)?;
        validate_payload_reference(&self.payload_reference)?;
        if self.byte_count == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "media byte_count must be positive",
            });
        }
        if let Some(prompt) = &self.revised_prompt {
            require_nonempty_bounded_text(
                "revised_prompt",
                prompt,
                MAXIMUM_MEDIA_REVISED_PROMPT_BYTES,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalMediaDraft {
    pub project_id: Option<String>,
    pub kind: LocalMediaKind,
    pub status: LocalMediaStatus,
    pub prompt: String,
    pub model_name: String,
    pub generated_at: DateTime<Utc>,
    pub assets: Vec<LocalMediaAsset>,
}

impl LocalMediaDraft {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(project_id) = &self.project_id {
            require_identifier("project_id", project_id)?;
        }
        require_nonempty_bounded_text("prompt", &self.prompt, MAXIMUM_MEDIA_PROMPT_BYTES)?;
        require_nonempty_bounded_text("model_name", &self.model_name, MAXIMUM_MEDIA_MODEL_BYTES)?;
        if self.assets.len() > MAXIMUM_MEDIA_ASSETS {
            return Err(ProtocolError::InvalidState {
                reason: "media asset count exceeds the supported limit",
            });
        }
        match self.status {
            LocalMediaStatus::Completed if self.assets.is_empty() => {
                return Err(ProtocolError::InvalidState {
                    reason: "completed media requires at least one asset",
                })
            }
            LocalMediaStatus::Pending | LocalMediaStatus::Failed if !self.assets.is_empty() => {
                return Err(ProtocolError::InvalidState {
                    reason: "incomplete media cannot reference completed payloads",
                })
            }
            _ => {}
        }
        match self.kind {
            LocalMediaKind::Video if self.assets.len() > 1 => {
                return Err(ProtocolError::InvalidState {
                    reason: "video media supports exactly one completed asset",
                })
            }
            _ => {}
        }
        let mut asset_ids = HashSet::new();
        let mut payload_references = HashSet::new();
        for asset in &self.assets {
            asset.validate()?;
            if !asset_ids.insert(asset.asset_id.as_str())
                || !payload_references.insert(asset.payload_reference.as_str())
            {
                return Err(ProtocolError::InvalidState {
                    reason: "media assets must have unique identities and payload references",
                });
            }
            let valid_mime = match self.kind {
                LocalMediaKind::Image => asset.mime_type.starts_with("image/"),
                LocalMediaKind::Video => asset.mime_type.starts_with("video/"),
            };
            if !valid_mime {
                return Err(ProtocolError::InvalidState {
                    reason: "media asset MIME type does not match its kind",
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalMediaSnapshot {
    pub record_id: String,
    pub owner_user_id: String,
    pub draft: LocalMediaDraft,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalMediaSnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("record_id", &self.record_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.draft.validate()?;
        if self.revision == 0 || self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "media revision and timestamps are invalid",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetMediaCommand {
    pub record_id: String,
}

impl GetMediaCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("record_id", &self.record_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ListMediaCommand {
    pub cursor: Option<String>,
    pub limit: u32,
}

impl ListMediaCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(cursor) = &self.cursor {
            require_identifier("cursor", cursor)?;
        }
        if self.limit == 0 || self.limit > 500 {
            return Err(ProtocolError::InvalidState {
                reason: "media page limit must be between 1 and 500",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PutMediaCommand {
    pub record_id: String,
    pub expected_revision: Option<u64>,
    pub draft: LocalMediaDraft,
}

impl PutMediaCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("record_id", &self.record_id)?;
        if self.expected_revision == Some(0) {
            return Err(ProtocolError::InvalidState {
                reason: "media expected_revision must be positive",
            });
        }
        self.draft.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeleteMediaCommand {
    pub record_id: String,
    pub expected_revision: u64,
}

impl DeleteMediaCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("record_id", &self.record_id)?;
        if self.expected_revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "media expected_revision must be positive",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MediaMutationResult {
    pub record: Option<LocalMediaSnapshot>,
    pub discarded_payload_references: Vec<String>,
}

impl MediaMutationResult {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(record) = &self.record {
            record.validate()?;
        }
        if self.discarded_payload_references.len() > MAXIMUM_DISCARDED_REFERENCES {
            return Err(ProtocolError::InvalidState {
                reason: "media mutation discarded too many payloads",
            });
        }
        for reference in &self.discarded_payload_references {
            validate_payload_reference(reference)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(mime_type: &str) -> LocalMediaAsset {
        LocalMediaAsset {
            asset_id: "asset-1".to_string(),
            mime_type: mime_type.to_string(),
            payload_reference: "Payloads/owner/record-1/asset.png".to_string(),
            content_hash: format!("sha256:{}", "a".repeat(64)),
            byte_count: 4,
            revised_prompt: None,
        }
    }

    #[test]
    fn completed_media_requires_kind_safe_assets() {
        let mut draft = LocalMediaDraft {
            project_id: None,
            kind: LocalMediaKind::Image,
            status: LocalMediaStatus::Completed,
            prompt: "A quiet landscape".to_string(),
            model_name: "image-model".to_string(),
            generated_at: Utc::now(),
            assets: vec![asset("image/png")],
        };
        draft.validate().unwrap();
        draft.assets[0].mime_type = "video/mp4".to_string();
        assert!(draft.validate().is_err());
    }

    #[test]
    fn pending_media_cannot_claim_local_payloads() {
        let draft = LocalMediaDraft {
            project_id: None,
            kind: LocalMediaKind::Image,
            status: LocalMediaStatus::Pending,
            prompt: "A quiet landscape".to_string(),
            model_name: "image-model".to_string(),
            generated_at: Utc::now(),
            assets: vec![asset("image/png")],
        };
        assert!(draft.validate().is_err());
    }
}
