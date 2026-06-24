use super::*;

#[derive(Clone)]
pub(super) enum TaskRunnerBuiltinToolService {
    Shared(SharedBuiltinToolService),
    Notepad(NotepadBuiltinService),
    TaskManager(TaskManagerService),
    TerminalController(TerminalControllerService),
    AskUser(AskUserService),
}

impl TaskRunnerBuiltinToolService {
    fn list_tools(&self) -> Vec<Value> {
        match self {
            Self::Shared(service) => service.list_tools(),
            Self::Notepad(service) => service.list_tools(),
            Self::TaskManager(service) => service.list_tools(),
            Self::TerminalController(service) => service.list_tools(),
            Self::AskUser(service) => service.list_tools(),
        }
    }

    fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: &ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        match self {
            Self::Shared(service) => service.call_tool(name, args, context, on_stream_chunk),
            Self::Notepad(service) => service.call_tool(name, args),
            Self::TaskManager(service) => {
                let callback = on_stream_chunk.map(|callback| -> TaskStreamChunkCallback {
                    Arc::new(move |chunk| callback(chunk))
                });
                service.call_tool(
                    name,
                    args,
                    context.conversation_id.as_deref(),
                    context.conversation_turn_id.as_deref(),
                    callback,
                )
            }
            Self::TerminalController(service) => {
                service.call_tool(name, args, context.conversation_id.as_deref())
            }
            Self::AskUser(service) => service.call_tool(
                name,
                args,
                context.conversation_id.as_deref(),
                context.conversation_turn_id.as_deref(),
                on_stream_chunk.map(|callback| {
                    Arc::new(move |chunk| callback(chunk))
                        as chatos_builtin_tools::AskUserStreamChunkCallback
                }),
            ),
        }
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        match self {
            Self::Shared(service) => service.unavailable_tools(),
            Self::Notepad(_) => Vec::new(),
            Self::TaskManager(_) => Vec::new(),
            Self::TerminalController(_) => Vec::new(),
            Self::AskUser(_) => Vec::new(),
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

#[derive(Clone)]
pub(in crate::services) struct DisabledBuiltinProvider {
    server_name: String,
    unavailable_tools: Vec<(String, String)>,
    error_message: String,
}

impl DisabledBuiltinProvider {
    pub(in crate::services) fn code_maintainer_write_for_chatos_plan() -> Self {
        let reason = "Tool is disabled in Chatos Plan task profile".to_string();
        Self {
            server_name: chatos_mcp_runtime::CODE_MAINTAINER_WRITE_SERVER_NAME.to_string(),
            unavailable_tools: [
                "write_file",
                "edit_file",
                "append_file",
                "delete_path",
                "apply_patch",
                "patch",
            ]
            .into_iter()
            .map(|name| (name.to_string(), reason.clone()))
            .collect(),
            error_message: reason,
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
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        self.service.unavailable_tools()
    }
}

#[async_trait]
impl BuiltinToolProvider for DisabledBuiltinProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        Vec::new()
    }

    async fn call_tool(
        &self,
        _name: &str,
        _args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        Err(self.error_message.clone())
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        self.unavailable_tools.clone()
    }
}
