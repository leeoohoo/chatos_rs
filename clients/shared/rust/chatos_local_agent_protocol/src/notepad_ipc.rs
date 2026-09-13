// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{require_identifier, require_nonempty_bounded_text, ProtocolError};

const MAXIMUM_FOLDER_BYTES: usize = 2_048;
const MAXIMUM_TITLE_BYTES: usize = 1_024;
const MAXIMUM_CONTENT_BYTES: usize = 4 * 1024 * 1024;
const MAXIMUM_TAG_BYTES: usize = 256;
const MAXIMUM_TAGS: usize = 100;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalNotepadKind {
    Folder,
    Note,
}

impl LocalNotepadKind {
    pub fn record_prefix(self) -> &'static str {
        match self {
            Self::Folder => "folder:",
            Self::Note => "note:",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalNotepadDraft {
    pub kind: LocalNotepadKind,
    pub folder: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
}

impl LocalNotepadDraft {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_folder_path(&self.folder, self.kind == LocalNotepadKind::Note)?;
        match self.kind {
            LocalNotepadKind::Folder => {
                if !self.title.is_empty() || !self.content.is_empty() || !self.tags.is_empty() {
                    return Err(ProtocolError::InvalidState {
                        reason: "notepad folder records cannot contain note fields",
                    });
                }
            }
            LocalNotepadKind::Note => {
                require_nonempty_bounded_text("notepad_title", &self.title, MAXIMUM_TITLE_BYTES)?;
                if self.content.len() > MAXIMUM_CONTENT_BYTES {
                    return Err(ProtocolError::InvalidState {
                        reason: "notepad content exceeds the maximum size",
                    });
                }
                if self.tags.len() > MAXIMUM_TAGS {
                    return Err(ProtocolError::InvalidState {
                        reason: "notepad contains too many tags",
                    });
                }
                for tag in &self.tags {
                    require_nonempty_bounded_text("notepad_tag", tag, MAXIMUM_TAG_BYTES)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalNotepadSnapshot {
    pub record_id: String,
    pub owner_user_id: String,
    pub draft: LocalNotepadDraft,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalNotepadSnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("notepad_record_id", &self.record_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.draft.validate()?;
        if !self.record_id.starts_with(self.draft.kind.record_prefix()) {
            return Err(ProtocolError::InvalidState {
                reason: "notepad record identity does not match its kind",
            });
        }
        if self.revision == 0 || self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "notepad revision and timestamps are invalid",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetNotepadCommand {
    pub record_id: String,
}

impl GetNotepadCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("notepad_record_id", &self.record_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ListNotepadCommand {
    pub cursor: Option<String>,
    pub limit: u32,
}

impl ListNotepadCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(cursor) = &self.cursor {
            require_identifier("cursor", cursor)?;
        }
        if self.limit == 0 || self.limit > 500 {
            return Err(ProtocolError::InvalidState {
                reason: "notepad page limit must be between 1 and 500",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PutNotepadCommand {
    pub record_id: String,
    pub expected_revision: Option<u64>,
    pub draft: LocalNotepadDraft,
}

impl PutNotepadCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("notepad_record_id", &self.record_id)?;
        if !self.record_id.starts_with(self.draft.kind.record_prefix()) {
            return Err(ProtocolError::InvalidState {
                reason: "notepad record identity does not match its kind",
            });
        }
        if self.expected_revision == Some(0) {
            return Err(ProtocolError::InvalidState {
                reason: "notepad expected_revision must be positive",
            });
        }
        self.draft.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeleteNotepadCommand {
    pub record_id: String,
    pub expected_revision: u64,
}

impl DeleteNotepadCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("notepad_record_id", &self.record_id)?;
        if self.expected_revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "notepad expected_revision must be positive",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RenameNotepadFolderCommand {
    pub folder: String,
    pub replacement: String,
}

impl RenameNotepadFolderCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_folder_path(&self.folder, false)?;
        validate_folder_path(&self.replacement, false)?;
        if self.folder == self.replacement
            || self.replacement.starts_with(&(self.folder.clone() + "/"))
        {
            return Err(ProtocolError::InvalidState {
                reason: "notepad folder replacement cannot equal or descend from the source",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeleteNotepadFolderCommand {
    pub folder: String,
    pub recursive: bool,
}

impl DeleteNotepadFolderCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_folder_path(&self.folder, false)
    }
}

fn validate_folder_path(value: &str, allow_root: bool) -> Result<(), ProtocolError> {
    if value.is_empty() && allow_root {
        return Ok(());
    }
    if value.is_empty()
        || value.trim() != value
        || value.len() > MAXIMUM_FOLDER_BYTES
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\0')
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(ProtocolError::InvalidState {
            reason: "notepad folder path is invalid",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_paths_are_relative_and_canonical() {
        assert!(validate_folder_path("ideas/design", false).is_ok());
        assert!(validate_folder_path("", true).is_ok());
        for invalid in ["", "/root", "root/", "root//child", "root/../child"] {
            assert!(validate_folder_path(invalid, false).is_err());
        }
    }
}
