// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::relay::RelayRequest;
use crate::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER;

#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalMcpToolSelection {
    pub(crate) code_read: bool,
    pub(crate) code_write: bool,
    pub(crate) terminal: bool,
    pub(crate) browser: bool,
}

impl LocalMcpToolSelection {
    pub(crate) fn allows_code_tool(&self, name: &str) -> bool {
        if is_code_maintainer_write_tool(name) {
            return self.code_write;
        }
        is_code_maintainer_read_tool(name) && self.code_read
    }
}

pub(crate) fn local_mcp_tool_selection(request: &RelayRequest) -> LocalMcpToolSelection {
    let mut selection = LocalMcpToolSelection {
        code_read: false,
        code_write: false,
        terminal: false,
        browser: false,
    };
    let Some(raw) = relay_header(request, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER) else {
        return selection;
    };
    for token in raw.split([',', ';', '|', ' ']).map(str::trim) {
        match normalize_local_mcp_builtin_kind_token(token).as_str() {
            "codemaintainerread" => selection.code_read = true,
            "codemaintainerwrite" => {
                selection.code_read = true;
                selection.code_write = true;
            }
            "terminalcontroller" => selection.terminal = true,
            "browsertools" => selection.browser = true,
            _ => {}
        }
    }
    selection
}

pub(crate) fn is_code_maintainer_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file_raw"
            | "read_file_range"
            | "read_file"
            | "list_dir"
            | "search_text"
            | "search_files"
            | "write_file"
            | "edit_file"
            | "append_file"
            | "delete_path"
            | "apply_patch"
            | "patch"
    )
}

pub(crate) fn is_terminal_controller_tool(name: &str) -> bool {
    matches!(
        name,
        "execute_command"
            | "get_recent_logs"
            | "process_list"
            | "process_poll"
            | "process_log"
            | "process_wait"
            | "process_write"
            | "process_kill"
            | "process"
    )
}

pub(crate) fn is_browser_tool(name: &str) -> bool {
    matches!(
        name,
        "browser_navigate"
            | "browser_snapshot"
            | "browser_click"
            | "browser_type"
            | "browser_scroll"
            | "browser_back"
            | "browser_press"
            | "browser_console"
            | "browser_get_images"
            | "browser_inspect"
            | "browser_research"
            | "browser_vision"
    )
}

fn relay_header<'a>(request: &'a RelayRequest, key: &str) -> Option<&'a str> {
    request
        .headers
        .get(key)
        .or_else(|| {
            request
                .headers
                .iter()
                .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
                .map(|(_, value)| value)
        })
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalize_local_mcp_builtin_kind_token(token: &str) -> String {
    token
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_code_maintainer_read_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file_raw"
            | "read_file_range"
            | "read_file"
            | "list_dir"
            | "search_text"
            | "search_files"
    )
}

fn is_code_maintainer_write_tool(name: &str) -> bool {
    matches!(
        name,
        "write_file" | "edit_file" | "append_file" | "delete_path" | "apply_patch" | "patch"
    )
}
