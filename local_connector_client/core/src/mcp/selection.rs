// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::relay::RelayRequest;
use crate::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER;
use chatos_mcp_service::{classify_builtin_tool, BuiltinToolAccess, HostCapabilityPolicy};

#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalMcpToolSelection {
    pub(crate) code_read: bool,
    pub(crate) code_write: bool,
    pub(crate) terminal: bool,
    pub(crate) browser: bool,
}

impl LocalMcpToolSelection {
    pub(crate) fn allows_code_tool(&self, name: &str) -> bool {
        match classify_builtin_tool(name) {
            Some(BuiltinToolAccess::CodeRead) => self.code_read,
            Some(BuiltinToolAccess::CodeWrite) => self.code_write,
            _ => false,
        }
    }
}

pub(crate) fn local_mcp_tool_selection(request: &RelayRequest) -> LocalMcpToolSelection {
    let policy = relay_header(request, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER)
        .map(HostCapabilityPolicy::from_header_value)
        .unwrap_or_default();
    LocalMcpToolSelection {
        code_read: policy.code_read,
        code_write: policy.code_write,
        terminal: policy.terminal,
        browser: policy.browser,
    }
}

pub(crate) fn is_code_maintainer_tool(name: &str) -> bool {
    matches!(
        classify_builtin_tool(name),
        Some(BuiltinToolAccess::CodeRead | BuiltinToolAccess::CodeWrite)
    )
}

pub(crate) fn is_terminal_controller_tool(name: &str) -> bool {
    matches!(
        classify_builtin_tool(name),
        Some(BuiltinToolAccess::Terminal)
    )
}

pub(crate) fn is_browser_tool(name: &str) -> bool {
    matches!(
        classify_builtin_tool(name),
        Some(BuiltinToolAccess::Browser)
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
