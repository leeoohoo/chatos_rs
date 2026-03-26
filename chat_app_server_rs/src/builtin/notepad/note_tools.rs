use std::sync::Arc;

use serde_json::json;

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::services::notepad::{
    CreateNoteParams, ListNotesParams, NotepadService, SearchNotesParams, UpdateNoteParams,
};

use super::support::{
    optional_string, optional_string_array_field, optional_string_field, parse_string_array,
    required_string,
};
use super::NotepadBuiltinService;

pub(super) fn register_note_tools(builtin: &mut NotepadBuiltinService, service: NotepadService) {
    builtin.register_tool(
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

    builtin.register_tool(
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

    builtin.register_tool(
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

    builtin.register_tool(
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

    builtin.register_tool(
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

    builtin.register_tool(
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

    builtin.register_tool(
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
}
