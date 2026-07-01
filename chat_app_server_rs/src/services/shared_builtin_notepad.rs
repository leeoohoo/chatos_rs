// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde_json::Value;

use chatos_builtin_tools::NotepadStore;

use crate::services::notepad::{
    CreateNoteParams, ListNotesParams, NotepadService, SearchNotesParams, UpdateNoteParams,
};

#[derive(Clone)]
pub struct ChatosNotepadStore {
    service: NotepadService,
}

impl ChatosNotepadStore {
    pub fn new(user_id: &str) -> Result<Self, String> {
        Ok(Self {
            service: NotepadService::new(user_id)?,
        })
    }
}

#[async_trait]
impl NotepadStore for ChatosNotepadStore {
    async fn init(&self) -> Result<Value, String> {
        self.service.init().await
    }

    async fn list_folders(&self) -> Result<Value, String> {
        self.service.list_folders().await
    }

    async fn create_folder(&self, folder: &str) -> Result<Value, String> {
        self.service.create_folder(folder).await
    }

    async fn rename_folder(&self, from: &str, to: &str) -> Result<Value, String> {
        self.service.rename_folder(from, to).await
    }

    async fn delete_folder(&self, folder: &str, recursive: bool) -> Result<Value, String> {
        self.service.delete_folder(folder, recursive).await
    }

    async fn list_notes(&self, params: Value) -> Result<Value, String> {
        self.service
            .list_notes(ListNotesParams {
                folder: string_field(&params, "folder"),
                recursive: bool_field(&params, "recursive", true),
                tags: string_array_field(&params, "tags"),
                match_any: bool_field(&params, "match_any", false),
                query: string_field(&params, "query"),
                limit: usize_field(&params, "limit", 200),
            })
            .await
    }

    async fn create_note(&self, params: Value) -> Result<Value, String> {
        self.service
            .create_note(CreateNoteParams {
                folder: string_field(&params, "folder"),
                title: string_field(&params, "title"),
                content: string_field(&params, "content"),
                tags: string_array_field(&params, "tags"),
            })
            .await
    }

    async fn read_note(&self, id: &str) -> Result<Value, String> {
        self.service.get_note(id).await
    }

    async fn update_note(&self, params: Value) -> Result<Value, String> {
        self.service
            .update_note(UpdateNoteParams {
                id: string_field(&params, "id"),
                title: optional_string_field(&params, "title"),
                content: optional_string_field(&params, "content"),
                folder: optional_string_field(&params, "folder"),
                tags: optional_string_array_field(&params, "tags"),
            })
            .await
    }

    async fn delete_note(&self, id: &str) -> Result<Value, String> {
        self.service.delete_note(id).await
    }

    async fn list_tags(&self) -> Result<Value, String> {
        self.service.list_tags().await
    }

    async fn search_notes(&self, params: Value) -> Result<Value, String> {
        self.service
            .search_notes(SearchNotesParams {
                query: string_field(&params, "query"),
                folder: string_field(&params, "folder"),
                recursive: bool_field(&params, "recursive", true),
                tags: string_array_field(&params, "tags"),
                match_any: bool_field(&params, "match_any", false),
                include_content: bool_field(&params, "include_content", true),
                limit: usize_field(&params, "limit", 50),
            })
            .await
    }
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn optional_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn string_array_field(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn optional_string_array_field(value: &Value, key: &str) -> Option<Vec<String>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|_| string_array_field(value, key))
}

fn bool_field(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_field(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}
