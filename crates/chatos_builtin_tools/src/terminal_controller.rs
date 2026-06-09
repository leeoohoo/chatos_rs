use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool_registry::{async_text_tool_handler, block_on_result, text_result, ToolRegistry};

pub const RECENT_LOGS_MAX_PER_TERMINAL_LIMIT: i64 = 50;
pub const RECENT_LOGS_MAX_TERMINAL_LIMIT: u64 = 20;
pub const PROCESS_LIST_MAX_LIMIT: u64 = 100;
pub const PROCESS_POLL_MAX_LIMIT: i64 = 200;
pub const PROCESS_WAIT_MAX_TIMEOUT_MS: u64 = 600_000;

#[derive(Debug, Clone)]
pub struct TerminalControllerOptions {
    pub root: PathBuf,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub idle_timeout_ms: u64,
    pub max_wait_ms: u64,
    pub max_output_chars: usize,
    pub store: TerminalControllerStoreRef,
}

#[derive(Debug, Clone)]
pub struct TerminalControllerContext {
    pub root: PathBuf,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub idle_timeout_ms: u64,
    pub max_wait_ms: u64,
    pub max_output_chars: usize,
}

#[async_trait]
pub trait TerminalControllerStore: Send + Sync {
    async fn execute_command(
        &self,
        context: TerminalControllerContext,
        path: String,
        command: String,
        background: bool,
    ) -> Result<Value, String>;

    async fn get_recent_logs(
        &self,
        context: TerminalControllerContext,
        per_terminal_limit: i64,
        terminal_limit: usize,
    ) -> Result<Value, String>;

    async fn process_list(
        &self,
        context: TerminalControllerContext,
        include_exited: bool,
        limit: usize,
    ) -> Result<Value, String>;

    async fn process_poll(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String>;

    async fn process_log(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String>;

    async fn process_wait(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        timeout_ms: u64,
    ) -> Result<Value, String>;

    async fn process_write(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        data: String,
        submit: bool,
    ) -> Result<Value, String>;

    async fn process_kill(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
    ) -> Result<Value, String>;
}

#[derive(Clone)]
pub struct TerminalControllerStoreRef(Arc<dyn TerminalControllerStore>);

impl TerminalControllerStoreRef {
    pub fn new(store: Arc<dyn TerminalControllerStore>) -> Self {
        Self(store)
    }

    fn inner(&self) -> Arc<dyn TerminalControllerStore> {
        self.0.clone()
    }
}

impl std::fmt::Debug for TerminalControllerStoreRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalControllerStoreRef")
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct TerminalControllerService {
    registry: ToolRegistry<ToolHandler>,
}

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

impl TerminalControllerService {
    pub fn new(opts: TerminalControllerOptions) -> Result<Self, String> {
        std::fs::create_dir_all(&opts.root)
            .map_err(|err| format!("create terminal controller root failed: {err}"))?;
        let root = canonicalize_path(&opts.root)?;

        let mut service = Self {
            registry: ToolRegistry::new(),
        };
        let bound = TerminalControllerContext {
            root: root.clone(),
            user_id: opts.user_id.clone(),
            project_id: opts
                .project_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            idle_timeout_ms: opts.idle_timeout_ms.max(1_000),
            max_wait_ms: opts.max_wait_ms.max(5_000),
            max_output_chars: opts.max_output_chars.max(1_000),
        };

        let root_for_desc = root.to_string_lossy().to_string();
        service.register_execute_command(bound.clone(), opts.store.clone(), root_for_desc.as_str());
        service.register_get_recent_logs(bound.clone(), opts.store.clone());
        service.register_process_list(bound.clone(), opts.store.clone());
        service.register_process_poll(bound.clone(), opts.store.clone());
        service.register_process_log(bound.clone(), opts.store.clone());
        service.register_process_wait(bound.clone(), opts.store.clone());
        service.register_process_write(bound.clone(), opts.store.clone());
        service.register_process_kill(bound.clone(), opts.store.clone());
        service.register_process_compat(bound, opts.store);
        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        _conversation_id: Option<&str>,
    ) -> Result<Value, String> {
        let tool = self
            .registry
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
        self.registry
            .register_tool(name, description, input_schema, handler);
    }

    fn register_execute_command(
        &mut self,
        bound: TerminalControllerContext,
        store: TerminalControllerStoreRef,
        root_for_desc: &str,
    ) {
        self.register_tool(
            "execute_command",
            &format!(
                "LOCAL ONLY: execute shell command in the local project terminal with path switching. Relative path is resolved from project root ({root_for_desc}). This tool does NOT execute on remote SSH hosts. For remote servers, use builtin_remote_connection_controller.run_command instead."
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Local directory path under project root. Defaults to project root when omitted."
                    },
                    "common": {
                        "type": "string",
                        "description": "Local shell command to run."
                    },
                    "command": {
                        "type": "string",
                        "description": "Alias of common. Local shell command to run."
                    },
                    "background": {
                        "type": "boolean",
                        "default": false,
                        "description": "When true, return immediately and use process_poll/process_wait to track progress."
                    }
                },
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(".")
                    .to_string();
                let command = args
                    .get("common")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        args.get("command")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(ToOwned::to_owned)
                    })
                    .ok_or_else(|| "common is required".to_string())?;
                let background = args
                    .get("background")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.execute_command(ctx, path, command, background).await })
            }),
        );
    }

    fn register_get_recent_logs(
        &mut self,
        bound: TerminalControllerContext,
        store: TerminalControllerStoreRef,
    ) {
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
            async_text_tool_handler(move |args| {
                let per_terminal_limit = args
                    .get("per_terminal_limit")
                    .and_then(Value::as_i64)
                    .unwrap_or(10)
                    .clamp(1, RECENT_LOGS_MAX_PER_TERMINAL_LIMIT);
                let terminal_limit =
                    args.get("terminal_limit")
                        .and_then(Value::as_u64)
                        .unwrap_or(20)
                        .clamp(1, RECENT_LOGS_MAX_TERMINAL_LIMIT) as usize;
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move {
                    store
                        .get_recent_logs(ctx, per_terminal_limit, terminal_limit)
                        .await
                })
            }),
        );
    }

    fn register_process_list(
        &mut self,
        bound: TerminalControllerContext,
        store: TerminalControllerStoreRef,
    ) {
        self.register_tool(
            "process_list",
            "List local terminal processes in current agent project context.",
            json!({
                "type": "object",
                "properties": {
                    "include_exited": { "type": "boolean", "default": false },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_LIST_MAX_LIMIT
                    }
                },
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let include_exited = args
                    .get("include_exited")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(20)
                    .clamp(1, PROCESS_LIST_MAX_LIMIT) as usize;
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.process_list(ctx, include_exited, limit).await })
            }),
        );
    }

    fn register_process_poll(
        &mut self,
        bound: TerminalControllerContext,
        store: TerminalControllerStoreRef,
    ) {
        self.register_tool(
            "process_poll",
            "Poll one local terminal process (status and buffered output logs).",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id from process_list or execute_command result."
                    },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Optional absolute log offset for incremental polling."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_POLL_MAX_LIMIT,
                        "description": "Max logs to fetch."
                    }
                },
                "required": ["terminal_id"],
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let offset = args.get("offset").and_then(Value::as_i64).map(|value| value.max(0));
                let limit = args
                    .get("limit")
                    .and_then(Value::as_i64)
                    .unwrap_or(80)
                    .clamp(1, PROCESS_POLL_MAX_LIMIT);
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.process_poll(ctx, terminal_id, offset, limit).await })
            }),
        );
    }

    fn register_process_log(
        &mut self,
        bound: TerminalControllerContext,
        store: TerminalControllerStoreRef,
    ) {
        self.register_tool(
            "process_log",
            "Read process logs in Hermes-compatible text mode with optional offset pagination.",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id."
                    },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Optional line offset for pagination."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_POLL_MAX_LIMIT,
                        "description": "Maximum lines to return."
                    }
                },
                "required": ["terminal_id"],
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let offset = args
                    .get("offset")
                    .and_then(Value::as_i64)
                    .map(|value| value.max(0));
                let limit = args
                    .get("limit")
                    .and_then(Value::as_i64)
                    .unwrap_or(200)
                    .clamp(1, PROCESS_POLL_MAX_LIMIT);
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.process_log(ctx, terminal_id, offset, limit).await })
            }),
        );
    }

    fn register_process_wait(
        &mut self,
        bound: TerminalControllerContext,
        store: TerminalControllerStoreRef,
    ) {
        self.register_tool(
            "process_wait",
            "Wait until a local terminal process exits or becomes idle.",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1_000,
                        "maximum": PROCESS_WAIT_MAX_TIMEOUT_MS,
                        "description": "Maximum wait time in milliseconds."
                    },
                    "timeout": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_WAIT_MAX_TIMEOUT_MS / 1_000,
                        "description": "Alias of timeout_ms in seconds."
                    }
                },
                "required": ["terminal_id"],
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let timeout_ms = resolve_wait_timeout_ms(&args);
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.process_wait(ctx, terminal_id, timeout_ms).await })
            }),
        );
    }

    fn register_process_write(
        &mut self,
        bound: TerminalControllerContext,
        store: TerminalControllerStoreRef,
    ) {
        self.register_tool(
            "process_write",
            "Write stdin content to a local terminal process.",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id."
                    },
                    "data": {
                        "type": "string",
                        "description": "Raw stdin content to send."
                    },
                    "submit": {
                        "type": "boolean",
                        "default": false,
                        "description": "Append one newline (Enter key) after data."
                    }
                },
                "required": ["terminal_id", "data"],
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let data = args
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "data is required".to_string())?
                    .to_string();
                let submit = args.get("submit").and_then(Value::as_bool).unwrap_or(false);
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.process_write(ctx, terminal_id, data, submit).await })
            }),
        );
    }

    fn register_process_kill(
        &mut self,
        bound: TerminalControllerContext,
        store: TerminalControllerStoreRef,
    ) {
        self.register_tool(
            "process_kill",
            "Terminate a local terminal process session.",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id."
                    }
                },
                "required": ["terminal_id"],
                "additionalProperties": false
            }),
            async_text_tool_handler(move |args| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let ctx = bound.clone();
                let store = store.inner();
                Ok(async move { store.process_kill(ctx, terminal_id).await })
            }),
        );
    }

    fn register_process_compat(
        &mut self,
        bound: TerminalControllerContext,
        store: TerminalControllerStoreRef,
    ) {
        self.register_tool(
            "process",
            "Hermes-compatible process manager. Actions: list/poll/log/wait/kill/write/submit/close.",
            json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "poll", "log", "wait", "kill", "write", "submit", "close"]
                    },
                    "terminal_id": { "type": "string", "description": "Process id." },
                    "include_exited": { "type": "boolean", "default": false },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_POLL_MAX_LIMIT
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1_000,
                        "maximum": PROCESS_WAIT_MAX_TIMEOUT_MS
                    },
                    "timeout": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_WAIT_MAX_TIMEOUT_MS / 1_000,
                        "description": "Alias of timeout_ms in seconds."
                    },
                    "data": { "type": "string" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let action = required_trimmed_string(&args, "action")?.to_ascii_lowercase();
                let terminal_id = args
                    .get("terminal_id")
                    .and_then(|value| coerce_process_identifier(Some(value)));
                let attach_action = |mut value: Value, action_name: &str| {
                    if let Some(map) = value.as_object_mut() {
                        map.insert(
                            "action".to_string(),
                            Value::String(action_name.to_string()),
                        );
                    }
                    value
                };
                let make_missing_err =
                    |action_name: &str| format!("terminal_id is required for {action_name}");

                match action.as_str() {
                    "list" => {
                        let include_exited = args
                            .get("include_exited")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let limit = args
                            .get("limit")
                            .and_then(Value::as_u64)
                            .unwrap_or(20)
                            .clamp(1, PROCESS_LIST_MAX_LIMIT) as usize;
                        let ctx = bound.clone();
                        let store = store.inner();
                        run_process_action("list", attach_action, async move {
                            store.process_list(ctx, include_exited, limit).await
                        })
                    }
                    "poll" => {
                        let id = terminal_id.clone().ok_or_else(|| make_missing_err("poll"))?;
                        let offset =
                            args.get("offset").and_then(Value::as_i64).map(|value| value.max(0));
                        let limit = args
                            .get("limit")
                            .and_then(Value::as_i64)
                            .unwrap_or(80)
                            .clamp(1, PROCESS_POLL_MAX_LIMIT);
                        let ctx = bound.clone();
                        let store = store.inner();
                        run_process_action("poll", attach_action, async move {
                            store.process_poll(ctx, id, offset, limit).await
                        })
                    }
                    "log" => {
                        let id = terminal_id.clone().ok_or_else(|| make_missing_err("log"))?;
                        let offset =
                            args.get("offset").and_then(Value::as_i64).map(|value| value.max(0));
                        let limit = args
                            .get("limit")
                            .and_then(Value::as_i64)
                            .unwrap_or(200)
                            .clamp(1, PROCESS_POLL_MAX_LIMIT);
                        let ctx = bound.clone();
                        let store = store.inner();
                        run_process_action("log", attach_action, async move {
                            store.process_log(ctx, id, offset, limit).await
                        })
                    }
                    "wait" => {
                        let id = terminal_id.clone().ok_or_else(|| make_missing_err("wait"))?;
                        let timeout_ms = resolve_wait_timeout_ms(&args);
                        let ctx = bound.clone();
                        let store = store.inner();
                        run_process_action("wait", attach_action, async move {
                            store.process_wait(ctx, id, timeout_ms).await
                        })
                    }
                    "kill" => {
                        let id = terminal_id.clone().ok_or_else(|| make_missing_err("kill"))?;
                        let ctx = bound.clone();
                        let store = store.inner();
                        run_process_action("kill", attach_action, async move {
                            store.process_kill(ctx, id).await
                        })
                    }
                    "write" => {
                        let id = terminal_id.clone().ok_or_else(|| make_missing_err("write"))?;
                        let data = coerce_process_data(args.get("data"))
                            .ok_or_else(|| "data is required for write".to_string())?
                            .to_string();
                        let ctx = bound.clone();
                        let store = store.inner();
                        run_process_action("write", attach_action, async move {
                            store.process_write(ctx, id, data, false).await
                        })
                    }
                    "submit" => {
                        let id = terminal_id.clone().ok_or_else(|| make_missing_err("submit"))?;
                        let data = coerce_process_data(args.get("data"))
                            .unwrap_or_default()
                            .to_string();
                        let ctx = bound.clone();
                        let store = store.inner();
                        run_process_action("submit", attach_action, async move {
                            store.process_write(ctx, id, data, true).await
                        })
                    }
                    "close" => {
                        let id = terminal_id.clone().ok_or_else(|| make_missing_err("close"))?;
                        let ctx = bound.clone();
                        let store = store.inner();
                        run_process_action("close", attach_action, async move {
                            store.process_write(ctx, id, "\u{4}".to_string(), false).await
                        })
                    }
                    _ => Err(
                        "Unknown process action. Use one of: list, poll, log, wait, kill, write, submit, close"
                            .to_string(),
                    ),
                }
            }),
        );
    }
}

fn run_process_action<Fut>(
    action_name: &str,
    attach_action: impl FnOnce(Value, &str) -> Value,
    future: Fut,
) -> Result<Value, String>
where
    Fut: std::future::Future<Output = Result<Value, String>>,
{
    let result = block_on_result(future)?;
    Ok(text_result(attach_action(result, action_name)))
}

pub fn resolve_wait_timeout_ms(args: &Value) -> u64 {
    args.get("timeout_ms")
        .and_then(Value::as_u64)
        .or_else(|| {
            args.get("timeout")
                .and_then(Value::as_u64)
                .map(|seconds| seconds.saturating_mul(1_000))
        })
        .unwrap_or(30_000)
        .clamp(1_000, PROCESS_WAIT_MAX_TIMEOUT_MS)
}

pub fn coerce_process_identifier(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(Value::Number(raw)) => Some(raw.to_string()),
        _ => None,
    }
}

fn coerce_process_data(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(raw)) => Some(raw.to_string()),
        Some(Value::Number(raw)) => Some(raw.to_string()),
        Some(Value::Bool(raw)) => Some(raw.to_string()),
        Some(Value::Null) => Some(String::new()),
        Some(other) => Some(other.to_string()),
        None => None,
    }
}

fn required_trimmed_string(args: &Value, field: &str) -> Result<String, String> {
    let value = args
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} is required"))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(trimmed.to_string())
    }
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path)
        .map(normalize_canonical_path)
        .map_err(|err| format!("canonicalize {} failed: {err}", path.display()))
}

fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    if !cfg!(windows) {
        return path;
    }
    let raw = path.to_string_lossy().to_string();
    if let Some(stripped) = raw.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{stripped}"));
    }
    if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        return PathBuf::from(stripped);
    }
    path
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use async_trait::async_trait;

    use super::*;

    #[derive(Debug, Clone)]
    struct NoopTerminalStore;

    #[async_trait]
    impl TerminalControllerStore for NoopTerminalStore {
        async fn execute_command(
            &self,
            _context: TerminalControllerContext,
            _path: String,
            command: String,
            _background: bool,
        ) -> Result<Value, String> {
            Ok(json!({ "common": command, "output": "" }))
        }

        async fn get_recent_logs(
            &self,
            _context: TerminalControllerContext,
            _per_terminal_limit: i64,
            _terminal_limit: usize,
        ) -> Result<Value, String> {
            Ok(json!({ "terminals": [] }))
        }

        async fn process_list(
            &self,
            _context: TerminalControllerContext,
            _include_exited: bool,
            _limit: usize,
        ) -> Result<Value, String> {
            Ok(json!({ "processes": [] }))
        }

        async fn process_poll(
            &self,
            _context: TerminalControllerContext,
            terminal_id: String,
            _offset: Option<i64>,
            _limit: i64,
        ) -> Result<Value, String> {
            Ok(json!({ "terminal_id": terminal_id }))
        }

        async fn process_log(
            &self,
            _context: TerminalControllerContext,
            terminal_id: String,
            _offset: Option<i64>,
            _limit: i64,
        ) -> Result<Value, String> {
            Ok(json!({ "terminal_id": terminal_id, "output": "" }))
        }

        async fn process_wait(
            &self,
            _context: TerminalControllerContext,
            terminal_id: String,
            _timeout_ms: u64,
        ) -> Result<Value, String> {
            Ok(json!({ "terminal_id": terminal_id, "wait_status": "completed" }))
        }

        async fn process_write(
            &self,
            _context: TerminalControllerContext,
            terminal_id: String,
            _data: String,
            _submit: bool,
        ) -> Result<Value, String> {
            Ok(json!({ "terminal_id": terminal_id, "operation_status": "ok" }))
        }

        async fn process_kill(
            &self,
            _context: TerminalControllerContext,
            terminal_id: String,
        ) -> Result<Value, String> {
            Ok(json!({ "terminal_id": terminal_id, "operation_status": "killed" }))
        }
    }

    fn temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "terminal-controller-tools-{}-{unique}",
            std::process::id()
        ))
    }

    fn test_service(root: PathBuf) -> TerminalControllerService {
        TerminalControllerService::new(TerminalControllerOptions {
            root,
            user_id: None,
            project_id: None,
            idle_timeout_ms: 1_000,
            max_wait_ms: 5_000,
            max_output_chars: 4_000,
            store: TerminalControllerStoreRef::new(Arc::new(NoopTerminalStore)),
        })
        .expect("create terminal controller")
    }

    #[test]
    fn terminal_controller_registers_process_tools() {
        let root = temp_root();
        std::fs::create_dir_all(&root).expect("create temp root");
        let service = test_service(root.clone());
        let tools = service.list_tools();
        let mut names: Vec<String> = tools
            .iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect();
        names.sort();

        for expected in [
            "execute_command",
            "get_recent_logs",
            "process",
            "process_kill",
            "process_list",
            "process_log",
            "process_poll",
            "process_wait",
            "process_write",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "missing tool: {expected}"
            );
        }

        let poll_schema = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("process_poll"))
            .and_then(|tool| tool.get("inputSchema"))
            .expect("process_poll schema");
        let required = poll_schema
            .get("required")
            .and_then(Value::as_array)
            .expect("process_poll required");
        assert!(
            required
                .iter()
                .any(|value| value.as_str() == Some("terminal_id")),
            "process_poll must require terminal_id"
        );

        let execute_schema = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("execute_command"))
            .and_then(|tool| tool.get("inputSchema"))
            .expect("execute_command schema");
        let execute_required = execute_schema
            .get("required")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            execute_required
                .iter()
                .all(|value| value.as_str() != Some("path")),
            "execute_command should not require path"
        );
        let has_background = execute_schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|props| props.contains_key("background"))
            .unwrap_or(false);
        assert!(
            has_background,
            "execute_command should expose background switch"
        );

        let process_schema = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("process"))
            .and_then(|tool| tool.get("inputSchema"))
            .expect("process schema");
        let process_required = process_schema
            .get("required")
            .and_then(Value::as_array)
            .expect("process required");
        assert!(
            process_required
                .iter()
                .any(|value| value.as_str() == Some("action")),
            "process must require action"
        );
        let process_actions = process_schema
            .get("properties")
            .and_then(Value::as_object)
            .and_then(|props| props.get("action"))
            .and_then(Value::as_object)
            .and_then(|item| item.get("enum"))
            .and_then(Value::as_array)
            .expect("process action enum");
        assert!(
            process_actions
                .iter()
                .any(|value| value.as_str() == Some("log")),
            "process(action) should include log for Hermes compatibility"
        );
        let has_timeout_alias = process_schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|props| props.contains_key("timeout"))
            .unwrap_or(false);
        assert!(
            has_timeout_alias,
            "process schema should expose timeout alias (seconds)"
        );

        let process_wait_schema = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("process_wait"))
            .and_then(|tool| tool.get("inputSchema"))
            .expect("process_wait schema");
        let process_wait_has_timeout_alias = process_wait_schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|props| props.contains_key("timeout"))
            .unwrap_or(false);
        assert!(
            process_wait_has_timeout_alias,
            "process_wait schema should expose timeout alias (seconds)"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn coerce_process_identifier_supports_numeric_value() {
        assert_eq!(
            coerce_process_identifier(Some(&json!(123456))),
            Some("123456".to_string())
        );
        assert_eq!(
            coerce_process_identifier(Some(&json!("  abc-123  "))),
            Some("abc-123".to_string())
        );
        assert!(coerce_process_identifier(Some(&json!("   "))).is_none());
        assert!(coerce_process_identifier(Some(&json!(true))).is_none());
    }

    #[test]
    fn resolve_wait_timeout_ms_supports_timeout_alias_seconds() {
        assert_eq!(resolve_wait_timeout_ms(&json!({})), 30_000);
        assert_eq!(resolve_wait_timeout_ms(&json!({ "timeout": 7 })), 7_000);
        assert_eq!(
            resolve_wait_timeout_ms(&json!({ "timeout_ms": 2_500, "timeout": 7 })),
            2_500
        );
        assert_eq!(
            resolve_wait_timeout_ms(&json!({ "timeout": 999_999 })),
            PROCESS_WAIT_MAX_TIMEOUT_MS
        );
    }
}
