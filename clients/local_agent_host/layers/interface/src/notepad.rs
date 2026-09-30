// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text};
use serde::{Deserialize, Serialize};

pub const LOCAL_NOTEPAD_MAX_CONTENT_BYTES: usize = 512 * 1024;
pub const LOCAL_NOTEPAD_MAX_IMAGE_BYTES: usize = 512 * 1024;
pub const LOCAL_NOTEPAD_MAX_LIST_LIMIT: u32 = 500;
pub const LOCAL_NOTEPAD_MAX_TAGS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitializeNotepadCommand {
    pub owner_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListNotepadFoldersCommand {
    pub owner_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateNotepadFolderCommand {
    pub owner_user_id: String,
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenameNotepadFolderCommand {
    pub owner_user_id: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteNotepadFolderCommand {
    pub owner_user_id: String,
    pub folder: String,
    pub recursive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListNotepadNotesCommand {
    pub owner_user_id: String,
    pub query: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateNotepadNoteCommand {
    pub owner_user_id: String,
    pub folder: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetNotepadNoteCommand {
    pub owner_user_id: String,
    pub note_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateNotepadNoteCommand {
    pub owner_user_id: String,
    pub note_id: String,
    pub expected_version: u64,
    pub title: Option<String>,
    pub content: Option<String>,
    pub folder: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteNotepadNoteCommand {
    pub owner_user_id: String,
    pub note_id: String,
    pub expected_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutNotepadImageCommand {
    pub owner_user_id: String,
    pub note_id: String,
    pub name: String,
    pub mime_type: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalNotepadNote {
    pub note_id: String,
    pub owner_user_id: String,
    pub title: String,
    pub folder: String,
    pub tags: Vec<String>,
    pub file: String,
    pub version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalNotepadNoteDetail {
    pub note: LocalNotepadNote,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalNotepadImage {
    pub image_id: String,
    pub note_id: String,
    pub owner_user_id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub sha256: String,
    pub data_url: String,
    pub created_at_unix_ms: i64,
}

macro_rules! validate_owner {
    ($value:expr) => {
        validate_identifier("owner_user_id", &$value.owner_user_id)
    };
}

impl InitializeNotepadCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)
    }
}

impl ListNotepadFoldersCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)
    }
}

impl CreateNotepadFolderCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_folder("folder", &self.folder, false)
    }
}

impl RenameNotepadFolderCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_folder("from", &self.from, false)?;
        validate_folder("to", &self.to, false)
    }
}

impl DeleteNotepadFolderCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_folder("folder", &self.folder, false)
    }
}

impl ListNotepadNotesCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        if self.limit == 0 || self.limit > LOCAL_NOTEPAD_MAX_LIST_LIMIT {
            return Err(format!(
                "limit must be between 1 and {LOCAL_NOTEPAD_MAX_LIST_LIMIT}"
            ));
        }
        if let Some(query) = self.query.as_deref() {
            validate_optional_text("query", query, 1_000)?;
        }
        Ok(())
    }
}

impl CreateNotepadNoteCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_folder("folder", &self.folder, true)?;
        validate_optional_text("title", &self.title, 1_000)?;
        validate_content(&self.content)?;
        validate_tags(&self.tags)
    }
}

impl GetNotepadNoteCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_identifier("note_id", &self.note_id)
    }
}

impl UpdateNotepadNoteCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_identifier("note_id", &self.note_id)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        if self.title.is_none()
            && self.content.is_none()
            && self.folder.is_none()
            && self.tags.is_none()
        {
            return Err("at least one note field must be updated".to_string());
        }
        if let Some(title) = self.title.as_deref() {
            validate_optional_text("title", title, 1_000)?;
        }
        if let Some(content) = self.content.as_deref() {
            validate_content(content)?;
        }
        if let Some(folder) = self.folder.as_deref() {
            validate_folder("folder", folder, true)?;
        }
        if let Some(tags) = self.tags.as_deref() {
            validate_tags(tags)?;
        }
        Ok(())
    }
}

impl DeleteNotepadNoteCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_identifier("note_id", &self.note_id)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        Ok(())
    }
}

impl PutNotepadImageCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_owner!(self)?;
        validate_identifier("note_id", &self.note_id)?;
        validate_text("name", &self.name, 512)?;
        match self.mime_type.as_str() {
            "image/png" | "image/jpeg" | "image/webp" | "image/gif" => {}
            _ => return Err("mime_type must be png, jpeg, webp, or gif".to_string()),
        }
        let maximum_base64 = LOCAL_NOTEPAD_MAX_IMAGE_BYTES.div_ceil(3) * 4;
        if self.data_base64.is_empty() || self.data_base64.len() > maximum_base64 {
            return Err(format!(
                "image exceeds {LOCAL_NOTEPAD_MAX_IMAGE_BYTES} bytes"
            ));
        }
        Ok(())
    }
}

fn validate_folder(name: &str, value: &str, allow_empty: bool) -> Result<(), String> {
    if allow_empty && value.trim().is_empty() {
        return Ok(());
    }
    validate_text(name, value, 1_000)?;
    if value.contains('\0') {
        return Err(format!("{name} must not contain NUL"));
    }
    Ok(())
}

fn validate_optional_text(name: &str, value: &str, maximum_length: usize) -> Result<(), String> {
    if value.is_empty() {
        return Ok(());
    }
    validate_text(name, value, maximum_length)
}

fn validate_content(content: &str) -> Result<(), String> {
    if content.len() > LOCAL_NOTEPAD_MAX_CONTENT_BYTES || content.contains('\0') {
        return Err(format!(
            "content must not contain NUL or exceed {LOCAL_NOTEPAD_MAX_CONTENT_BYTES} bytes"
        ));
    }
    Ok(())
}

fn validate_tags(tags: &[String]) -> Result<(), String> {
    if tags.len() > LOCAL_NOTEPAD_MAX_TAGS {
        return Err(format!(
            "tags must contain at most {LOCAL_NOTEPAD_MAX_TAGS} values"
        ));
    }
    for tag in tags {
        validate_text("tag", tag, 128)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_requires_version_and_a_change() {
        let command = UpdateNotepadNoteCommand {
            owner_user_id: "user-1".to_string(),
            note_id: "note-1".to_string(),
            expected_version: 0,
            title: None,
            content: None,
            folder: None,
            tags: None,
        };
        assert!(command.validate().is_err());
    }

    #[test]
    fn image_rejects_unsupported_mime_type() {
        let command = PutNotepadImageCommand {
            owner_user_id: "user-1".to_string(),
            note_id: "note-1".to_string(),
            name: "payload.svg".to_string(),
            mime_type: "image/svg+xml".to_string(),
            data_base64: "PHN2Zy8+".to_string(),
        };
        assert!(command.validate().is_err());
    }
}
