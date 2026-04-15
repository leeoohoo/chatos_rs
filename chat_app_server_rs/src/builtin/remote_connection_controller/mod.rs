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
    unavailable_tools: HashMap<String, String>,
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
            unavailable_tools: HashMap::new(),
        };
        let bound = BoundContext {
            server_name: opts.server_name,
            user_id: opts
                .user_id
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string),
            default_remote_connection_id: opts
                .default_remote_connection_id
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string),
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

        if bound.user_id.is_none() {
            let reason = "remote_connection_controller 缺少 user_id 上下文".to_string();
            for tool_name in [
                "list_connections",
                "test_connection",
                "run_command",
                "list_directory",
                "read_file",
            ] {
                service
                    .unavailable_tools
                    .insert(tool_name.to_string(), reason.clone());
            }
            return Ok(service);
        }

        let require_connection_id = bound.default_remote_connection_id.is_none();
        service.register_list_connections(bound.clone());
        service.register_test_connection(bound.clone(), require_connection_id);
        service.register_run_command(bound.clone(), require_connection_id);
        service.register_list_directory(bound.clone(), require_connection_id);
        service.register_read_file(bound, require_connection_id);

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

    pub fn unavailable_tools(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .unavailable_tools
            .iter()
            .map(|(name, reason)| (name.clone(), reason.clone()))
            .collect();
        pairs.sort_by(|left, right| left.0.cmp(&right.0));
        pairs
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

    fn register_test_connection(&mut self, bound: BoundContext, require_connection_id: bool) {
        let required = if require_connection_id {
            json!(["connection_id"])
        } else {
            json!([])
        };
        let description = if require_connection_id {
            "Test SSH connectivity for a remote connection. connection_id is required because no default connection is bound."
        } else {
            "Test SSH connectivity for a remote connection. If connection_id is omitted, use default bound connection from chat runtime."
        };
        self.register_tool(
            "test_connection",
            description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" }
                },
                "required": required,
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

    fn register_run_command(&mut self, bound: BoundContext, require_connection_id: bool) {
        let required = if require_connection_id {
            json!(["connection_id", "command"])
        } else {
            json!(["command"])
        };
        let description = if require_connection_id {
            "Run one SSH command on a remote host. connection_id is required because no default connection is bound. Returns exit_code/stdout/stderr/truncated flags. Dangerous commands are blocked by default unless allow_dangerous=true."
        } else {
            "Run one SSH command on a remote host (preferred for all server-side checks/ops). Returns structured result including exit_code/stdout/stderr/truncated flags. Dangerous commands are blocked by default unless allow_dangerous=true."
        };
        self.register_tool(
            "run_command",
            description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "command": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 120 },
                    "allow_dangerous": { "type": "boolean" },
                    "max_output_chars": { "type": "integer", "minimum": 128, "maximum": 20000 }
                },
                "required": required,
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

    fn register_list_directory(&mut self, bound: BoundContext, require_connection_id: bool) {
        let required = if require_connection_id {
            json!(["connection_id"])
        } else {
            json!([])
        };
        let description = if require_connection_id {
            "List entries under a remote directory path. connection_id is required because no default connection is bound."
        } else {
            "List entries under a remote directory path on the bound SSH host."
        };
        self.register_tool(
            "list_directory",
            description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "path": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000 }
                },
                "required": required,
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

    fn register_read_file(&mut self, bound: BoundContext, require_connection_id: bool) {
        let server_name = bound.server_name.clone();
        let required = if require_connection_id {
            json!(["connection_id", "path"])
        } else {
            json!(["path"])
        };
        let description = if require_connection_id {
            format!(
                "Read remote file content (up to size limit) on SSH server {}. connection_id is required because no default connection is bound.",
                server_name
            )
        } else {
            format!(
                "Read remote file content (up to size limit) on bound SSH server {}.",
                server_name
            )
        };
        self.register_tool(
            "read_file",
            &description,
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string" },
                    "path": { "type": "string" },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": 262144 }
                },
                "required": required,
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

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{RemoteConnectionControllerOptions, RemoteConnectionControllerService};

    fn option_base() -> RemoteConnectionControllerOptions {
        RemoteConnectionControllerOptions {
            server_name: "remote_connection_controller".to_string(),
            user_id: Some("u1".to_string()),
            default_remote_connection_id: None,
            command_timeout_seconds: 20,
            max_command_timeout_seconds: 120,
            max_output_chars: 20_000,
            max_read_file_bytes: 256 * 1024,
        }
    }

    fn find_required_for_tool(tools: &[Value], name: &str) -> Vec<String> {
        tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
            .and_then(|tool| tool.get("inputSchema"))
            .and_then(|schema| schema.get("required"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    #[test]
    fn hides_tools_when_user_context_is_missing() {
        let mut options = option_base();
        options.user_id = None;

        let service = RemoteConnectionControllerService::new(options).expect("init");
        assert!(service.list_tools().is_empty());

        let unavailable = service.unavailable_tools();
        assert_eq!(unavailable.len(), 5);
        for name in [
            "list_connections",
            "test_connection",
            "run_command",
            "list_directory",
            "read_file",
        ] {
            assert!(
                unavailable.iter().any(|(tool_name, _)| tool_name == name),
                "missing unavailable tool: {name}"
            );
        }
    }

    #[test]
    fn requires_connection_id_when_default_connection_is_missing() {
        let options = option_base();
        let service = RemoteConnectionControllerService::new(options).expect("init");
        let tools = service.list_tools();

        let test_required = find_required_for_tool(&tools, "test_connection");
        assert!(test_required.iter().any(|value| value == "connection_id"));

        let run_required = find_required_for_tool(&tools, "run_command");
        assert!(run_required.iter().any(|value| value == "connection_id"));
        assert!(run_required.iter().any(|value| value == "command"));

        let list_required = find_required_for_tool(&tools, "list_directory");
        assert!(list_required.iter().any(|value| value == "connection_id"));

        let read_required = find_required_for_tool(&tools, "read_file");
        assert!(read_required.iter().any(|value| value == "connection_id"));
        assert!(read_required.iter().any(|value| value == "path"));
    }

    #[test]
    fn keeps_connection_id_optional_when_default_connection_exists() {
        let mut options = option_base();
        options.default_remote_connection_id = Some("conn_default".to_string());
        let service = RemoteConnectionControllerService::new(options).expect("init");
        let tools = service.list_tools();

        let test_required = find_required_for_tool(&tools, "test_connection");
        assert!(!test_required.iter().any(|value| value == "connection_id"));

        let run_required = find_required_for_tool(&tools, "run_command");
        assert!(run_required.iter().any(|value| value == "command"));
        assert!(!run_required.iter().any(|value| value == "connection_id"));

        let list_required = find_required_for_tool(&tools, "list_directory");
        assert!(!list_required.iter().any(|value| value == "connection_id"));

        let read_required = find_required_for_tool(&tools, "read_file");
        assert!(read_required.iter().any(|value| value == "path"));
        assert!(!read_required.iter().any(|value| value == "connection_id"));
    }
}
