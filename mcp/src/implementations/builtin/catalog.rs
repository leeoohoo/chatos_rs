// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_runtime::BuiltinMcpKind;
use serde_json::Value;

pub fn builtin_tool_catalog(kind: BuiltinMcpKind) -> Result<Vec<Value>, String> {
    serde_json::from_str::<HashMap<String, Vec<Value>>>(include_str!(
        "../../builtin_tool_catalog.json"
    ))
    .map_err(|error| format!("decode builtin tool catalog failed: {error}"))?
    .remove(kind.kind_name())
    .ok_or_else(|| format!("builtin tool catalog is unavailable: {}", kind.kind_name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use BuiltinMcpKind::*;

    #[test]
    fn every_builtin_kind_has_a_non_empty_catalog() {
        for kind in [
            CodeMaintainerRead,
            CodeMaintainerWrite,
            TerminalController,
            Notepad,
            AgentBuilder,
            AskUser,
            RemoteConnectionController,
            MemorySkillReader,
            MemoryCommandReader,
            MemoryPluginReader,
        ] {
            let tools = builtin_tool_catalog(kind)
                .unwrap_or_else(|error| panic!("{}: {error}", kind.kind_name()));
            assert!(!tools.is_empty(), "{}", kind.kind_name());
            assert!(tools
                .iter()
                .all(|tool| tool.get("name").and_then(Value::as_str).is_some()));
        }
    }

    #[test]
    fn project_tool_catalogs_are_location_neutral() {
        for kind in [CodeMaintainerWrite, TerminalController] {
            let descriptions = builtin_tool_catalog(kind)
                .expect("project tool catalog")
                .into_iter()
                .filter_map(|tool| {
                    tool.get("description")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
                .join("\n");

            assert!(!descriptions.contains("Current project workspace root:"));
            assert!(!descriptions.contains("/workspace"));
            assert!(descriptions.contains("current project workspace root"));
        }
    }

    #[test]
    fn notepad_read_note_supports_image_pagination() {
        let tools = builtin_tool_catalog(Notepad).expect("notepad catalog");
        let read_note = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("read_note"))
            .expect("read_note tool");

        assert_eq!(
            read_note
                .pointer("/inputSchema/properties/imageOffset/type")
                .and_then(Value::as_str),
            Some("integer")
        );
        assert_eq!(
            read_note
                .pointer("/inputSchema/properties/imageOffset/minimum")
                .and_then(Value::as_u64),
            Some(0)
        );
    }
}
