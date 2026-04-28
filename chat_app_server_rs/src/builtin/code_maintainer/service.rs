use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::core::tool_registry::ToolRegistry;

use super::aliases::{append_compat_aliases, maybe_call_compat_tool};
use super::fs_ops::FsOps;
use super::registration_read::register_read_tools;
use super::registration_write::register_write_tools;
use super::storage::ChangeLogStore;
use super::utils::{ensure_dir, generate_id, normalize_name};

pub struct CodeMaintainerOptions {
    pub server_name: String,
    pub root: PathBuf,
    pub project_id: Option<String>,
    pub allow_writes: bool,
    pub max_file_bytes: i64,
    pub max_write_bytes: i64,
    pub search_limit: usize,
    pub enable_read_tools: bool,
    pub enable_write_tools: bool,
    pub conversation_id: Option<String>,
    pub run_id: Option<String>,
    pub db_path: Option<String>,
}

#[derive(Clone)]
pub struct CodeMaintainerService {
    registry: ToolRegistry<ToolHandler>,
    default_conversation_id: String,
    default_run_id: String,
}

type ToolHandler = Arc<dyn Fn(Value, &ToolContext) -> Result<Value, String> + Send + Sync>;

pub(super) struct ToolContext<'a> {
    pub(super) conversation_id: &'a str,
    pub(super) run_id: &'a str,
}

impl CodeMaintainerService {
    pub fn new(opts: CodeMaintainerOptions) -> Result<Self, String> {
        let server_name = normalize_name(&opts.server_name);
        let root = opts.root;
        ensure_dir(&root)
            .map_err(|err| format!("create workspace dir {} failed: {}", root.display(), err))?;

        let change_log =
            ChangeLogStore::new(&server_name, opts.project_id.clone(), opts.db_path.clone())?;
        let change_log = Arc::new(Mutex::new(change_log));

        let fs_ops = FsOps::new(
            root.clone(),
            opts.allow_writes,
            opts.max_file_bytes,
            opts.max_write_bytes,
            opts.search_limit,
        );

        let default_conversation_id = opts
            .conversation_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| generate_id("conversation"));
        let default_run_id = opts.run_id.unwrap_or_default();

        let mut service = Self {
            registry: ToolRegistry::new(),
            default_conversation_id,
            default_run_id,
        };

        let workspace_note = format!(
            "Workspace root: {}. Paths must stay inside this directory.",
            root.display()
        );
        let writes_note = if opts.allow_writes {
            "Writes enabled"
        } else {
            "Writes disabled"
        };
        let enable_read_tools = opts.enable_read_tools;
        let enable_write_tools = opts.enable_write_tools;

        if !enable_read_tools && !enable_write_tools {
            return Err("No tools are enabled for this code maintainer instance".to_string());
        }

        if enable_read_tools {
            register_read_tools(
                &mut service,
                fs_ops.clone(),
                workspace_note.as_str(),
                opts.max_file_bytes,
            );
        }

        if enable_write_tools {
            register_write_tools(
                &mut service,
                fs_ops,
                change_log,
                root,
                opts.allow_writes,
                opts.max_file_bytes,
                opts.max_write_bytes,
                writes_note,
                workspace_note.as_str(),
            );
        }

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        let mut tools: Vec<Value> = self.registry.list_tools();
        append_compat_aliases(self, &mut tools);
        tools
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        conversation_id: Option<&str>,
    ) -> Result<Value, String> {
        let conversation = conversation_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(self.default_conversation_id.as_str());
        let run = if self.default_run_id.trim().is_empty() {
            conversation
        } else {
            self.default_run_id.as_str()
        };
        let ctx = ToolContext {
            conversation_id: conversation,
            run_id: run,
        };

        if let Some(result) = maybe_call_compat_tool(self, name, &args, &ctx)? {
            return Ok(result);
        }

        self.call_registered_tool(name, args, &ctx)
    }

    pub(super) fn has_tool(&self, name: &str) -> bool {
        self.registry.get(name).is_some()
    }

    pub(super) fn call_registered_tool(
        &self,
        name: &str,
        args: Value,
        ctx: &ToolContext<'_>,
    ) -> Result<Value, String> {
        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args, ctx)
    }

    pub(super) fn register_tool(
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
