// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{require_digest, require_identifier, ProtocolError};

const MAXIMUM_CLIPBOARD_PREVIEW_BYTES: usize = 4 * 1024;
const MAXIMUM_CLIPBOARD_SOURCE_BYTES: usize = 1024;
const MAXIMUM_CLIPBOARD_MIME_BYTES: usize = 256;
const MAXIMUM_PAYLOAD_REFERENCE_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalClipboardKind {
    Text,
    Url,
    Files,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalClipboardDraft {
    pub kind: LocalClipboardKind,
    pub mime_type: String,
    pub content_hash: String,
    pub text_preview: Option<String>,
    pub source_bundle_id: Option<String>,
    pub payload_reference: String,
    pub byte_count: u64,
    pub pasteboard_type: Option<String>,
}

impl LocalClipboardDraft {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_digest("content_hash", &self.content_hash)?;
        require_bounded_text(
            "mime_type",
            &self.mime_type,
            MAXIMUM_CLIPBOARD_MIME_BYTES,
            true,
        )?;
        require_optional_text(
            "text_preview",
            self.text_preview.as_deref(),
            MAXIMUM_CLIPBOARD_PREVIEW_BYTES,
        )?;
        require_optional_text(
            "source_bundle_id",
            self.source_bundle_id.as_deref(),
            MAXIMUM_CLIPBOARD_SOURCE_BYTES,
        )?;
        require_optional_text(
            "pasteboard_type",
            self.pasteboard_type.as_deref(),
            MAXIMUM_CLIPBOARD_MIME_BYTES,
        )?;
        if self.byte_count == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "clipboard byte_count must be positive",
            });
        }
        validate_payload_reference(&self.payload_reference)?;
        match self.kind {
            LocalClipboardKind::Image if self.pasteboard_type.is_none() => {
                Err(ProtocolError::InvalidState {
                    reason: "image clipboard records require a pasteboard type",
                })
            }
            LocalClipboardKind::Text | LocalClipboardKind::Url | LocalClipboardKind::Files
                if self.pasteboard_type.is_some() =>
            {
                Err(ProtocolError::InvalidState {
                    reason: "non-image clipboard records cannot carry a pasteboard type",
                })
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalClipboardSnapshot {
    pub entry_id: String,
    pub owner_user_id: String,
    pub draft: LocalClipboardDraft,
    pub revision: u64,
    pub is_pinned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalClipboardSnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("entry_id", &self.entry_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.draft.validate()?;
        if self.revision == 0 || self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "clipboard revision and timestamps are invalid",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ListClipboardCommand {
    pub cursor: Option<String>,
    pub limit: u32,
}

impl ListClipboardCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(cursor) = &self.cursor {
            require_identifier("cursor", cursor)?;
        }
        if self.limit == 0 || self.limit > 500 {
            return Err(ProtocolError::InvalidState {
                reason: "clipboard page limit must be between 1 and 500",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetClipboardCommand {
    pub entry_id: String,
}

impl GetClipboardCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("entry_id", &self.entry_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoreClipboardCommand {
    pub entry_id: String,
    pub draft: LocalClipboardDraft,
}

impl StoreClipboardCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("entry_id", &self.entry_id)?;
        self.draft.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SetClipboardPinnedCommand {
    pub entry_id: String,
    pub expected_revision: u64,
    pub is_pinned: bool,
}

impl SetClipboardPinnedCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("entry_id", &self.entry_id)?;
        require_positive_revision(self.expected_revision)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeleteClipboardCommand {
    pub entry_id: String,
    pub expected_revision: u64,
}

impl DeleteClipboardCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("entry_id", &self.entry_id)?;
        require_positive_revision(self.expected_revision)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClipboardMutationResult {
    pub entry: Option<LocalClipboardSnapshot>,
    pub discarded_payload_references: Vec<String>,
}

impl ClipboardMutationResult {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(entry) = &self.entry {
            entry.validate()?;
        }
        if self.discarded_payload_references.len() > 500 {
            return Err(ProtocolError::InvalidState {
                reason: "clipboard mutation discarded too many payloads",
            });
        }
        for reference in &self.discarded_payload_references {
            validate_payload_reference(reference)?;
        }
        Ok(())
    }
}

fn require_positive_revision(value: u64) -> Result<(), ProtocolError> {
    if value == 0 {
        Err(ProtocolError::InvalidState {
            reason: "clipboard expected_revision must be positive",
        })
    } else {
        Ok(())
    }
}

fn require_optional_text(
    field: &'static str,
    value: Option<&str>,
    maximum: usize,
) -> Result<(), ProtocolError> {
    match value {
        Some(value) => require_bounded_text(field, value, maximum, false),
        None => Ok(()),
    }
}

fn require_bounded_text(
    _field: &'static str,
    value: &str,
    maximum: usize,
    require_nonempty: bool,
) -> Result<(), ProtocolError> {
    if (require_nonempty && value.is_empty())
        || value.trim() != value
        || value.len() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(ProtocolError::InvalidState {
            reason: "clipboard text metadata is invalid",
        });
    }
    Ok(())
}

fn validate_payload_reference(value: &str) -> Result<(), ProtocolError> {
    let valid = !value.is_empty()
        && value.len() <= MAXIMUM_PAYLOAD_REFERENCE_BYTES
        && value.trim() == value
        && value.starts_with("Payloads/")
        && !value.contains(['\\', ':'])
        && !value.chars().any(char::is_control)
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."));
    if valid {
        Ok(())
    } else {
        Err(ProtocolError::InvalidState {
            reason: "clipboard payload_reference must be a safe relative payload path",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_references_cannot_escape_the_private_root() {
        for accepted in ["Payloads/entry.txt", "Payloads/entry-1.png"] {
            assert!(validate_payload_reference(accepted).is_ok());
        }
        for rejected in [
            "/Payloads/entry.txt",
            "Payloads/../entry.txt",
            "Payloads/a/../../entry.txt",
            "Payloads\\entry.txt",
            "Other/entry.txt",
        ] {
            assert!(validate_payload_reference(rejected).is_err(), "{rejected}");
        }
    }
}
