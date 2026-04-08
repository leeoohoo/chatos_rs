mod actions;
mod context;

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;

use self::actions::{
    list_connections_with_context, list_directory_with_context, read_file_with_context,
    run_command_with_context, test_connection_with_context,
};
use self::context::{
    optional_bool, optional_trimmed_string, optional_u64, optional_usize, required_trimmed_string,
};

const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 20;
const MAX_COMMAND_TIMEOUT_SECONDS: u64 = 120;
const DEFAULT_MAX_OUTPUT_CHARS: usize = 20_000;
const DEFAULT_MAX_READ_FILE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct RemoteConnectionControllerOptions {
    pub server_name: String,
    pub user_id: Option<String>,
    pub default_remote_connection_id: Option<String>,
    pub command_timeout_seconds: u64,
    pub max_command_timeout_seconds: u64,
    pub max_output_chars: usize,
    pub max_read_file_bytes: usize,
}

#[derive(Clone)]
pub struct RemoteConnectionControllerService {
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

#[derive(Clone)]
pub(super) struct BoundContext {
    pub(super) server_name: String,
    pub(super) user_id: Option<String>,
    pub(super) default_remote_connection_id: Option<String>,
    pub(super) command_timeout_seconds: u64,
    pub(super) max_command_timeout_seconds: u64,
    pub(super) max_output_chars: usize,
    pub(super) max_read_file_bytes: usize,
}

impl RemoteConnectionControllerService {
    pub fn new(opts: RemoteConnectionControllerOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
        };
        let bound = BoundContext {
            server_name: opts.server_name,
            user_id: opts.user_id,
            default_remote_connection_id: opts.default_remote_connection_id,
            command_timeout_seconds: opts
                .command_timeout_seconds
                .clamp(1, MAX_COMMAND_TIMEOUT_SECONDS)
                .max(DEFAULT_COMMAND_TIMEOUT_SECONDS),
            max_command_timeout_seconds: opts
                .max_command_timeout_seconds
                .max(MAX_COMMAND_TIMEOUT_SECONDS),
            max_output_chars: opts.max_output_chars.max(DEFAULT_MAX_OUTPUT_CHARS),
            max_read_file_bytes: opts.max_read_file_bytes.max(DEFAULT_MAX_READ_FILE_BYTES),
        };

        service.register_list_connections(bound.clone());
        service.register_test_connection(bound.clone());
        service.register_run_command(bound.clone());
        service.register_list_directory(bound.clone());
        service.register_read_file(bound);

        Ok(service)
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

    fn register_list_connections(&mut self, bound: BoundContext) {
        self.register_tool(
            "list_connections",
            "List current user's available remote SSH/SFTP connections (sensitive fields are masked). Use this tool family for remote hosts, not local terminal execution.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Arc::new(move |_args| {
                let ctx = bound.clone();
                let result = block_on_result(async move { list_connections_with_context(ctx).await })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_test_connection(&mut self, bound: BoundContext) {
        self.register_tool(
            "test_connection",
            "Test SSH connectivity for a remote connection. If connection_id is omitted, use default bound connection from chat runtime.",
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    test_connection_with_context(ctx, connection_id).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_run_command(&mut self, bound: BoundContext) {
        self.register_tool(
            "run_command",
            "Run one SSH command on a remote host (preferred for all server-side checks/ops). Returns structured result including exit_code/stdout/stderr/truncated flags. Dangerous commands are blocked by default unless allow_dangerous=true.",
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "command": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 120 },
                    "allow_dangerous": { "type": "boolean" },
                    "max_output_chars": { "type": "integer", "minimum": 128, "maximum": 20000 }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let command = required_trimmed_string(&args, "command")?;
                let timeout_seconds = optional_u64(&args, "timeout_seconds");
                let allow_dangerous = optional_bool(&args, "allow_dangerous");
                let max_output_chars = optional_usize(&args, "max_output_chars");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    run_command_with_context(
                        ctx,
                        connection_id,
                        command,
                        timeout_seconds,
                        allow_dangerous,
                        max_output_chars,
                    )
                    .await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_list_directory(&mut self, bound: BoundContext) {
        self.register_tool(
            "list_directory",
            "List entries under a remote directory path on the bound SSH host.",
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "path": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000 }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let path = optional_trimmed_string(&args, "path");
                let limit = optional_usize(&args, "limit");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    list_directory_with_context(ctx, connection_id, path, limit).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_read_file(&mut self, bound: BoundContext) {
        let server_name = bound.server_name.clone();
        self.register_tool(
            "read_file",
            &format!(
                "Read remote file content (up to size limit) on bound SSH server {}.",
                server_name
            ),
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "path": { "type": "string" },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": 262144 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let connection_id = optional_trimmed_string(&args, "connection_id");
                let path = required_trimmed_string(&args, "path")?;
                let max_bytes = optional_usize(&args, "max_bytes");
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    read_file_with_context(ctx, connection_id, path, max_bytes).await
                })?;
                Ok(text_result(result))
            }),
        );
    }
}
