// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool_registry::{block_on_result, text_result, ToolRegistry};

#[async_trait]
pub trait NotepadStore: Send + Sync {
    async fn init(&self) -> Result<Value, String>;
    async fn list_folders(&self) -> Result<Value, String>;
    async fn create_folder(&self, folder: &str) -> Result<Value, String>;
    async fn rename_folder(&self, from: &str, to: &str) -> Result<Value, String>;
    async fn delete_folder(&self, folder: &str, recursive: bool) -> Result<Value, String>;
    async fn list_notes(&self, params: Value) -> Result<Value, String>;
    async fn create_note(&self, params: Value) -> Result<Value, String>;
    async fn read_note(&self, id: &str) -> Result<Value, String>;
    async fn update_note(&self, params: Value) -> Result<Value, String>;
    async fn delete_note(&self, id: &str) -> Result<Value, String>;
    async fn list_tags(&self) -> Result<Value, String>;
    async fn search_notes(&self, params: Value) -> Result<Value, String>;
}

#[derive(Clone)]
pub struct NotepadStoreRef(Arc<dyn NotepadStore>);

impl NotepadStoreRef {
    pub fn new(store: Arc<dyn NotepadStore>) -> Self {
        Self(store)
    }

    fn inner(&self) -> Arc<dyn NotepadStore> {
        self.0.clone()
    }
}

impl std::fmt::Debug for NotepadStoreRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotepadStoreRef").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct NotepadOptions {
    pub server_name: String,
    pub store: NotepadStoreRef,
}

#[derive(Clone)]
pub struct NotepadBuiltinService {
    registry: ToolRegistry<ToolHandler>,
}

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

impl NotepadBuiltinService {
    pub fn new(opts: NotepadOptions) -> Result<Self, String> {
        let mut service = Self {
            registry: ToolRegistry::new(),
        };
        service.register_folder_tools(opts.server_name.as_str(), opts.store.clone());
        service.register_note_tools(opts.store);
        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args)
    }

    fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: ToolHandler,
    ) {
        self.registry
            .register_tool(name, description, input_schema, handler);
    }

    fn register_folder_tools(&mut self, server_name: &str, store: NotepadStoreRef) {
        self.register_tool(
            "init",
            &format!("Initialize notepad storage (server: {server_name})."),
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |_| block_on_result(store.inner().init()).map(text_result))
            },
        );
        self.register_tool(
            "list_folders",
            "List all notepad folders.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |_| block_on_result(store.inner().list_folders()).map(text_result))
            },
        );
        self.register_tool(
            "create_folder",
            "Create a notepad folder. Supports nested paths like work/ideas.",
            json!({
                "type": "object",
                "properties": { "folder": {"type": "string"} },
                "required": ["folder"],
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |args| {
                    let folder = required_string(&args, "folder")?;
                    block_on_result(store.inner().create_folder(folder.as_str())).map(text_result)
                })
            },
        );
        self.register_tool(
            "rename_folder",
            "Rename or move a notepad folder.",
            json!({
                "type": "object",
                "properties": {
                    "from": {"type": "string"},
                    "to": {"type": "string"}
                },
                "required": ["from", "to"],
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |args| {
                    let from = required_string(&args, "from")?;
                    let to = required_string(&args, "to")?;
                    block_on_result(store.inner().rename_folder(from.as_str(), to.as_str()))
                        .map(text_result)
                })
            },
        );
        self.register_tool(
            "delete_folder",
            "Delete a notepad folder. Use recursive=true to remove all notes under it.",
            json!({
                "type": "object",
                "properties": {
                    "folder": {"type": "string"},
                    "recursive": {"type": "boolean"}
                },
                "required": ["folder"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let folder = required_string(&args, "folder")?;
                let recursive = args
                    .get("recursive")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                block_on_result(store.inner().delete_folder(folder.as_str(), recursive))
                    .map(text_result)
            }),
        );
    }

    fn register_note_tools(&mut self, store: NotepadStoreRef) {
        self.register_tool(
            "list_notes",
            "List notes with optional folder/tag/query filters.",
            json!({
                "type": "object",
                "properties": {
                    "folder": {"type": "string"},
                    "recursive": {"type": "boolean"},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "match": {"type": "string", "enum": ["all", "any"]},
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 500}
                },
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |args| {
                    let params = json!({
                        "folder": optional_string(&args, "folder"),
                        "recursive": args.get("recursive").and_then(Value::as_bool).unwrap_or(true),
                        "tags": parse_string_array(args.get("tags")),
                        "match_any": optional_string(&args, "match").eq_ignore_ascii_case("any"),
                        "query": optional_string(&args, "query"),
                        "limit": args.get("limit").and_then(Value::as_u64).map(|value| value as usize).unwrap_or(200),
                    });
                    block_on_result(store.inner().list_notes(params)).map(text_result)
                })
            },
        );
        self.register_tool(
            "create_note",
            "Create a markdown note.",
            json!({
                "type": "object",
                "properties": {
                    "folder": {"type": "string"},
                    "title": {"type": "string"},
                    "content": {"type": "string"},
                    "tags": {"type": "array", "items": {"type": "string"}}
                },
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |args| {
                    let params = json!({
                        "folder": optional_string(&args, "folder"),
                        "title": optional_string(&args, "title"),
                        "content": optional_string(&args, "content"),
                        "tags": parse_string_array(args.get("tags")),
                    });
                    block_on_result(store.inner().create_note(params)).map(text_result)
                })
            },
        );
        self.register_tool(
            "read_note",
            "Read a note by id.",
            json!({
                "type": "object",
                "properties": { "id": {"type": "string"} },
                "required": ["id"],
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |args| {
                    let id = required_string(&args, "id")?;
                    block_on_result(store.inner().read_note(id.as_str())).map(text_result)
                })
            },
        );
        self.register_tool(
            "update_note",
            "Update a note by id. You can change title/content/folder/tags.",
            json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "title": {"type": "string"},
                    "content": {"type": "string"},
                    "folder": {"type": "string"},
                    "tags": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["id"],
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |args| {
                    let params = json!({
                        "id": required_string(&args, "id")?,
                        "title": optional_string_field(&args, "title"),
                        "content": optional_string_field(&args, "content"),
                        "folder": optional_string_field(&args, "folder"),
                        "tags": optional_string_array_field(args.get("tags")),
                    });
                    block_on_result(store.inner().update_note(params)).map(text_result)
                })
            },
        );
        self.register_tool(
            "delete_note",
            "Delete a note by id.",
            json!({
                "type": "object",
                "properties": { "id": {"type": "string"} },
                "required": ["id"],
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |args| {
                    let id = required_string(&args, "id")?;
                    block_on_result(store.inner().delete_note(id.as_str())).map(text_result)
                })
            },
        );
        self.register_tool(
            "list_tags",
            "List notepad tags and usage counts.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            {
                let store = store.clone();
                Arc::new(move |_| block_on_result(store.inner().list_tags()).map(text_result))
            },
        );
        self.register_tool(
            "search_notes",
            "Search notes by keyword with optional folder/tag filters.",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "folder": {"type": "string"},
                    "recursive": {"type": "boolean"},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "match": {"type": "string", "enum": ["all", "any"]},
                    "includeContent": {"type": "boolean"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200}
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let params = json!({
                    "query": required_string(&args, "query")?,
                    "folder": optional_string(&args, "folder"),
                    "recursive": args.get("recursive").and_then(Value::as_bool).unwrap_or(true),
                    "tags": parse_string_array(args.get("tags")),
                    "match_any": optional_string(&args, "match").eq_ignore_ascii_case("any"),
                    "include_content": args.get("includeContent").and_then(Value::as_bool).unwrap_or(true),
                    "limit": args.get("limit").and_then(Value::as_u64).map(|value| value as usize).unwrap_or(50),
                });
                block_on_result(store.inner().search_notes(params)).map(text_result)
            }),
        );
    }
}

fn optional_string(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn optional_string_field(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn required_string(args: &Value, key: &str) -> Result<String, String> {
    let value = optional_string(args, key);
    if value.is_empty() {
        Err(format!("{key} is required"))
    } else {
        Ok(value)
    }
}

fn parse_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn optional_string_array_field(value: Option<&Value>) -> Option<Vec<String>> {
    value
        .and_then(Value::as_array)
        .map(|_| parse_string_array(value))
}
