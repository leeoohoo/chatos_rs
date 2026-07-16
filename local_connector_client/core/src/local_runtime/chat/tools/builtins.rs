// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_mcp_runtime::{
    BuiltinMcpKind, BuiltinToolProvider, ToolCallContext, ToolStreamChunkCallback,
};
use chatos_mcp_service::{classify_builtin_tool, BuiltinToolAccess};
use serde_json::Value;

use crate::history::CommandHistoryRecorder;
use crate::mcp::provider::{
    call_builtin_compatible_local_tool, local_mcp_builtin_compatible_tools,
};
use crate::relay::RelayRequest;
use crate::{LocalState, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER};

#[derive(Clone)]
pub(super) struct LocalChatBuiltinProvider {
    kind: BuiltinMcpKind,
    request: RelayRequest,
    state: LocalState,
    history_recorder: CommandHistoryRecorder,
}

impl LocalChatBuiltinProvider {
    pub(super) fn new(
        kind: BuiltinMcpKind,
        mut request: RelayRequest,
        state: LocalState,
        history_recorder: CommandHistoryRecorder,
    ) -> Self {
        request.headers.insert(
            LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
            kind.kind_name().to_string(),
        );
        Self {
            kind,
            request,
            state,
            history_recorder,
        }
    }

    fn allows(&self, tool_name: &str) -> bool {
        matches!(
            (self.kind, classify_builtin_tool(tool_name)),
            (
                BuiltinMcpKind::CodeMaintainerRead,
                Some(BuiltinToolAccess::CodeRead)
            ) | (
                BuiltinMcpKind::CodeMaintainerWrite,
                Some(BuiltinToolAccess::CodeWrite)
            ) | (
                BuiltinMcpKind::TerminalController,
                Some(BuiltinToolAccess::Terminal)
            ) | (
                BuiltinMcpKind::BrowserTools,
                Some(BuiltinToolAccess::Browser)
            )
        )
    }
}

#[async_trait]
impl BuiltinToolProvider for LocalChatBuiltinProvider {
    fn server_name(&self) -> &str {
        self.kind.server_name()
    }

    fn list_tools(&self) -> Vec<Value> {
        local_mcp_builtin_compatible_tools(&self.request, &self.state)
            .unwrap_or_default()
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| self.allows(name))
            })
            .collect()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        if !self.allows(name) {
            return Err(format!(
                "local builtin tool is not allowed by {}: {name}",
                self.kind.kind_name()
            ));
        }
        call_builtin_compatible_local_tool(
            &self.request,
            &self.state,
            name,
            args,
            &self.history_recorder,
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("unsupported local builtin tool: {name}"))
    }
}
