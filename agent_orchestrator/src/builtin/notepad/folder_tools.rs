use std::sync::Arc;

use serde_json::json;

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::services::notepad::NotepadService;

use super::support::required_string;
use super::NotepadBuiltinService;

pub(super) fn register_folder_tools(
    builtin: &mut NotepadBuiltinService,
    service: NotepadService,
    server_name: &str,
) {
    builtin.register_tool(
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

    builtin.register_tool(
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

    builtin.register_tool(
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

    builtin.register_tool(
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
                block_on_result(service.rename_folder(from.as_str(), to.as_str())).map(text_result)
            })
        },
    );

    builtin.register_tool(
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
                block_on_result(service.delete_folder(folder.as_str(), recursive)).map(text_result)
            })
        },
    );
}
