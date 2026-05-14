mod actions;
mod capture;
mod context;
mod registration_execute;
mod registration_logs;
mod registration_process;
mod registration_process_compat;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use crate::core::tool_registry::ToolRegistry;
use serde_json::Value;

use self::context::canonicalize_path;
#[cfg(test)]
use self::registration_process::resolve_wait_timeout_ms;
#[cfg(test)]
use self::registration_process_compat::coerce_process_identifier;

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
    registry: ToolRegistry<ToolHandler>,
}

type ToolHandler = Arc<dyn Fn(Value, Option<&str>) -> Result<Value, String> + Send + Sync>;

pub(super) const RECENT_LOGS_MAX_PER_TERMINAL_LIMIT: i64 = 50;
pub(super) const RECENT_LOGS_MAX_TERMINAL_LIMIT: u64 = 20;
pub(super) const RECENT_LOGS_PER_ENTRY_MAX_CHARS: usize = 1_500;
pub(super) const RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL: usize = 16_000;
pub(super) const PROCESS_LIST_MAX_LIMIT: u64 = 100;
pub(super) const PROCESS_POLL_MAX_LIMIT: i64 = 200;
pub(super) const PROCESS_WAIT_MAX_TIMEOUT_MS: u64 = 600_000;

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
            registry: ToolRegistry::new(),
        };

        let bound = BoundContext {
            root: root.clone(),
            user_id: opts.user_id.clone(),
            project_id: opts
                .project_id
                .as_deref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            idle_timeout_ms: opts.idle_timeout_ms.max(1_000),
            max_wait_ms: opts.max_wait_ms.max(5_000),
            max_output_chars: opts.max_output_chars.max(1_000),
        };

        let root_for_desc = root.to_string_lossy().to_string();
        service.register_execute_command(bound.clone(), root_for_desc.as_str());
        service.register_get_recent_logs(bound.clone());
        service.register_process_list(bound.clone());
        service.register_process_poll(bound.clone());
        service.register_process_log(bound.clone());
        service.register_process_wait(bound.clone());
        service.register_process_write(bound.clone());
        service.register_process_kill(bound.clone());
        service.register_process_compat(bound);

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        conversation_id: Option<&str>,
    ) -> Result<Value, String> {
        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args, conversation_id)
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
}
