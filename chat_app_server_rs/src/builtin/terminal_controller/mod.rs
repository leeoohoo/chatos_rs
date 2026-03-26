mod actions;
mod capture;
mod context;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;

use self::actions::{execute_command_with_context, get_recent_logs_with_context};
use self::context::{canonicalize_path, required_trimmed_string};

pub struct TerminalControllerOptions {
    pub root: PathBuf,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub idle_timeout_ms: u64,
    pub max_wait_ms: u64,
    pub max_output_chars: usize,
}

#[derive(Clone)]
pub struct TerminalControllerService {
    tools: HashMap<String, Tool>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value, Option<&str>) -> Result<Value, String> + Send + Sync>;

pub(super) const RECENT_LOGS_MAX_PER_TERMINAL_LIMIT: i64 = 50;
pub(super) const RECENT_LOGS_MAX_TERMINAL_LIMIT: u64 = 20;
pub(super) const RECENT_LOGS_PER_ENTRY_MAX_CHARS: usize = 1_500;
pub(super) const RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL: usize = 16_000;

#[derive(Clone)]
pub(super) struct BoundContext {
    pub(super) root: PathBuf,
    pub(super) user_id: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) idle_timeout_ms: u64,
    pub(super) max_wait_ms: u64,
    pub(super) max_output_chars: usize,
}

impl TerminalControllerService {
    pub fn new(opts: TerminalControllerOptions) -> Result<Self, String> {
        std::fs::create_dir_all(&opts.root)
            .map_err(|err| format!("create terminal controller root failed: {}", err))?;
        let root = canonicalize_path(&opts.root)?;

        let mut service = Self {
            tools: HashMap::new(),
        };

        let bound = BoundContext {
            root: root.clone(),
            user_id: opts.user_id.clone(),
            project_id: opts
                .project_id
                .as_deref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            idle_timeout_ms: opts.idle_timeout_ms.max(1_000),
            max_wait_ms: opts.max_wait_ms.max(5_000),
            max_output_chars: opts.max_output_chars.max(1_000),
        };

        let root_for_desc = root.to_string_lossy().to_string();
        service.register_execute_command(bound.clone(), root_for_desc.as_str());
        service.register_get_recent_logs(bound);

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

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        session_id: Option<&str>,
    ) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args, session_id)
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

    fn register_execute_command(&mut self, bound: BoundContext, root_for_desc: &str) {
        self.register_tool(
            "execute_command",
            &format!(
                "Execute command in project terminal with path switching. Relative path is resolved from project root ({root_for_desc})."
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "common": { "type": "string" }
                },
                "additionalProperties": false,
                "required": ["path", "common"]
            }),
            Arc::new(move |args, _session_id| {
                let path = required_trimmed_string(&args, "path")?;
                let command = required_trimmed_string(&args, "common")?;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    execute_command_with_context(ctx, path.as_str(), command.as_str()).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_get_recent_logs(&mut self, bound: BoundContext) {
        self.register_tool(
            "get_recent_logs",
            "Get recent logs grouped by terminal for current agent project.",
            json!({
                "type": "object",
                "properties": {
                    "per_terminal_limit": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "terminal_limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, _session_id| {
                let per_terminal_limit = args
                    .get("per_terminal_limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(10)
                    .clamp(1, RECENT_LOGS_MAX_PER_TERMINAL_LIMIT);
                let terminal_limit =
                    args.get("terminal_limit")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(20)
                        .clamp(1, RECENT_LOGS_MAX_TERMINAL_LIMIT) as usize;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    get_recent_logs_with_context(ctx, per_terminal_limit, terminal_limit).await
                })?;
                Ok(text_result(result))
            }),
        );
    }
}
