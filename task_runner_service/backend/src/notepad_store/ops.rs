// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chatos_mcp::NotepadStore;
use serde_json::{json, Value};
use tokio::fs;
use uuid::Uuid;

use super::support::{
    derive_title, filter_notes, normalize_folder, normalize_optional_folder, normalize_required,
    normalize_tags, now_iso, optional_non_empty, read_text_limited, value_string,
    value_string_array, write_atomic_limited, MAX_NOTE_CONTENT_BYTES,
};
use super::{NoteMeta, TaskRunnerNotepadStore};

mod folders;
mod notes;
mod search;

#[async_trait]
impl NotepadStore for TaskRunnerNotepadStore {
    async fn init(&self) -> Result<Value, String> {
        self.init_store().await
    }

    async fn list_folders(&self) -> Result<Value, String> {
        self.list_folders_value().await
    }

    async fn create_folder(&self, folder: &str) -> Result<Value, String> {
        self.create_folder_value(folder).await
    }

    async fn rename_folder(&self, from: &str, to: &str) -> Result<Value, String> {
        self.rename_folder_value(from, to).await
    }

    async fn delete_folder(&self, folder: &str, recursive: bool) -> Result<Value, String> {
        self.delete_folder_value(folder, recursive).await
    }

    async fn list_notes(&self, params: Value) -> Result<Value, String> {
        self.list_notes_value(params).await
    }

    async fn create_note(&self, params: Value) -> Result<Value, String> {
        self.create_note_value(params).await
    }

    async fn read_note(&self, id: &str) -> Result<Value, String> {
        self.read_note_value(id).await
    }

    async fn update_note(&self, params: Value) -> Result<Value, String> {
        self.update_note_value(params).await
    }

    async fn delete_note(&self, id: &str) -> Result<Value, String> {
        self.delete_note_value(id).await
    }

    async fn list_tags(&self) -> Result<Value, String> {
        self.list_tags_value().await
    }

    async fn search_notes(&self, params: Value) -> Result<Value, String> {
        self.search_notes_value(params).await
    }
}
