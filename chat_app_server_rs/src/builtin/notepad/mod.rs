use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::services::notepad::{
    CreateNoteParams, ListNotesParams, NotepadService, SearchNotesParams, UpdateNoteParams,
};

#[derive(Debug, Clone)]
pub struct NotepadOptions {
    pub server_name: String,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Clone)]
pub struct NotepadBuiltinService {
    tools: HashMap<String, Tool>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

impl NotepadBuiltinService {
    pub fn new(opts: NotepadOptions) -> Result<Self, String> {
        let user_id = opts
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("builtin");
        let project_id = opts
            .project_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let service = NotepadService::new(user_id, project_id)?;

        let mut out = Self {
            tools: HashMap::new(),
        };

        let server_name = opts.server_name;
        out.register_tool(
            "init",
            &format!("Initialize notepad storage (server: {server_name})."),
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            {
                let service = service.clone();
                Arc::new(move |_| block_on_result(service.init()).map(text_result))
            },
        );

        out.register_tool(
            "list_folders",
            "List all notepad folders.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            {
                let service = service.clone();
                Arc::new(move |_| block_on_result(service.list_folders()).map(text_result))
            },
        );

        out.register_tool(
            "create_folder",
            "Create a notepad folder. Supports nested paths like work/ideas.",
            json!({
                "type": "object",
                "properties": {
                    "folder": {"type": "string"}
                },
                "required": ["folder"],
                "additionalProperties": false
            }),
            {
                let service = service.clone();
                Arc::new(move |args| {
                    let folder = required_string(&args, "folder")?;
                    block_on_result(service.create_folder(folder.as_str())).map(text_result)
                })
            },
        );

        out.register_tool(
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
                let service = service.clone();
                Arc::new(move |args| {
                    let from = required_string(&args, "from")?;
                    let to = required_string(&args, "to")?;
                    block_on_result(service.rename_folder(from.as_str(), to.as_str()))
                        .map(text_result)
                })
            },
        );

        out.register_tool(
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
            {
                let service = service.clone();
                Arc::new(move |args| {
                    let folder = required_string(&args, "folder")?;
                    let recursive = args
                        .get("recursive")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                    block_on_result(service.delete_folder(folder.as_str(), recursive))
                        .map(text_result)
                })
            },
        );

        out.register_tool(
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
                let service = service.clone();
                Arc::new(move |args| {
                    let params = ListNotesParams {
                        folder: optional_string(&args, "folder"),
                        recursive: args
                            .get("recursive")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(true),
                        tags: parse_string_array(args.get("tags")),
                        match_any: optional_string(&args, "match").eq_ignore_ascii_case("any"),
                        query: optional_string(&args, "query"),
                        limit: args
                            .get("limit")
                            .and_then(|value| value.as_u64())
                            .map(|value| value as usize)
                            .unwrap_or(200),
                    };
                    block_on_result(service.list_notes(params)).map(text_result)
                })
            },
        );

        out.register_tool(
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
                let service = service.clone();
                Arc::new(move |args| {
                    let params = CreateNoteParams {
                        folder: optional_string(&args, "folder"),
                        title: optional_string(&args, "title"),
                        content: optional_string(&args, "content"),
                        tags: parse_string_array(args.get("tags")),
                    };
                    block_on_result(service.create_note(params)).map(text_result)
                })
            },
        );

        out.register_tool(
            "read_note",
            "Read a note by id.",
            json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"}
                },
                "required": ["id"],
                "additionalProperties": false
            }),
            {
                let service = service.clone();
                Arc::new(move |args| {
                    let id = required_string(&args, "id")?;
                    block_on_result(service.get_note(id.as_str())).map(text_result)
                })
            },
        );

        out.register_tool(
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
                let service = service.clone();
                Arc::new(move |args| {
                    let id = required_string(&args, "id")?;
                    let params = UpdateNoteParams {
                        id,
                        title: optional_string_field(&args, "title"),
                        content: optional_string_field(&args, "content"),
                        folder: optional_string_field(&args, "folder"),
                        tags: optional_string_array_field(args.get("tags")),
                    };
                    block_on_result(service.update_note(params)).map(text_result)
                })
            },
        );

        out.register_tool(
            "delete_note",
            "Delete a note by id.",
            json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"}
                },
                "required": ["id"],
                "additionalProperties": false
            }),
            {
                let service = service.clone();
                Arc::new(move |args| {
                    let id = required_string(&args, "id")?;
                    block_on_result(service.delete_note(id.as_str())).map(text_result)
                })
            },
        );

        out.register_tool(
            "list_tags",
            "List notepad tags and usage counts.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            {
                let service = service.clone();
                Arc::new(move |_| block_on_result(service.list_tags()).map(text_result))
            },
        );

        out.register_tool(
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
            {
                let service = service.clone();
                Arc::new(move |args| {
                    let query = required_string(&args, "query")?;
                    let params = SearchNotesParams {
                        query,
                        folder: optional_string(&args, "folder"),
                        recursive: args
                            .get("recursive")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(true),
                        tags: parse_string_array(args.get("tags")),
                        match_any: optional_string(&args, "match").eq_ignore_ascii_case("any"),
                        include_content: args
                            .get("includeContent")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(true),
                        limit: args
                            .get("limit")
                            .and_then(|value| value.as_u64())
                            .map(|value| value as usize)
                            .unwrap_or(50),
                    };
                    block_on_result(service.search_notes(params)).map(text_result)
                })
            },
        );

        Ok(out)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema
                })
            })
            .collect()
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let tool = self
            .tools
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
        self.tools.insert(
            name.to_string(),
            Tool {
                name: name.to_string(),
                description: description.to_string(),
                input_schema,
                handler,
            },
        );
    }
}

fn optional_string(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn optional_string_field(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|value| value.as_str())
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
        .and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| entry.as_str())
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn optional_string_array_field(value: Option<&Value>) -> Option<Vec<String>> {
    value
        .and_then(|item| item.as_array())
        .map(|_| parse_string_array(value))
}
