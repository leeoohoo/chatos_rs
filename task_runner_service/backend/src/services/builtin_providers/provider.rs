// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Clone)]
pub(super) enum TaskRunnerBuiltinToolService {
    Shared(SharedBuiltinToolService),
    Notepad(NotepadBuiltinService),
    TerminalController(TerminalControllerService),
    AskUser(AskUserService),
    ProjectManagement(ProjectManagementBuiltinService),
}

impl TaskRunnerBuiltinToolService {
    fn list_tools(&self) -> Vec<Value> {
        match self {
            Self::Shared(service) => service.list_tools(),
            Self::Notepad(service) => service.list_tools(),
            Self::TerminalController(service) => service.list_tools(),
            Self::AskUser(service) => service.list_tools(),
            Self::ProjectManagement(service) => service.list_tools(),
        }
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: &ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        match self {
            Self::Shared(service) => service.call_tool(name, args, context, on_stream_chunk),
            Self::Notepad(service) => service.call_tool(name, args),
            Self::TerminalController(service) => {
                service.call_tool(name, args, context.conversation_id.as_deref())
            }
            Self::AskUser(service) => service.call_tool(
                name,
                args,
                context.conversation_id.as_deref(),
                context.conversation_turn_id.as_deref(),
                on_stream_chunk.map(|callback| {
                    Arc::new(move |chunk| callback(chunk)) as chatos_mcp::AskUserStreamChunkCallback
                }),
            ),
            Self::ProjectManagement(service) => service.call_tool(name, args).await,
        }
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        match self {
            Self::Shared(service) => service.unavailable_tools(),
            Self::Notepad(_) => Vec::new(),
            Self::TerminalController(_) => Vec::new(),
            Self::AskUser(_) => Vec::new(),
            Self::ProjectManagement(service) => service.unavailable_tools(),
        }
    }
}

#[derive(Clone)]
pub(in crate::services) struct TaskRunnerBuiltinProvider {
    server_name: String,
    service: TaskRunnerBuiltinToolService,
}

impl TaskRunnerBuiltinProvider {
    pub(super) fn new(
        server_name: impl Into<String>,
        service: TaskRunnerBuiltinToolService,
    ) -> Self {
        Self {
            server_name: server_name.into(),
            service,
        }
    }
}

#[async_trait]
impl BuiltinToolProvider for TaskRunnerBuiltinProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        self.service.list_tools()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        self.service
            .call_tool(name, args, &context, on_stream_chunk)
            .await
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        self.service.unavailable_tools()
    }
}
