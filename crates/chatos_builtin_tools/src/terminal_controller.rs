// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::tool_registry::{async_text_tool_handler, block_on_result, text_result, ToolRegistry};

mod parsing;
mod schema;

use self::parsing::{coerce_process_data, required_trimmed_string};
pub use self::parsing::{coerce_process_identifier, resolve_wait_timeout_ms};
use self::schema::{
    execute_command_schema, process_compat_schema, process_kill_schema, process_list_schema,
    process_log_schema, process_poll_schema, process_wait_schema, process_write_schema,
    recent_logs_schema,
};

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

        service.register_execute_command(bound.clone(), opts.store.clone());
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
    ) {
        self.register_tool(
            "execute_command",
            "LOCAL ONLY: execute shell command in the local project terminal with path switching. Relative path is resolved from the current project workspace (`/workspace`). This tool does NOT execute on remote SSH hosts. For remote servers, use builtin_remote_connection_controller.run_command instead.",
            execute_command_schema(),
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
            recent_logs_schema(),
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
            process_list_schema(),
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
            process_poll_schema(),
            async_text_tool_handler(move |args| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let offset = args
                    .get("offset")
                    .and_then(Value::as_i64)
                    .map(|value| value.max(0));
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
            process_log_schema(),
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
            process_wait_schema(),
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
            process_write_schema(),
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
            process_kill_schema(),
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
            process_compat_schema(),
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
mod tests;
