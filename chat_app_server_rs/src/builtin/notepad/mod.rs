mod folder_tools;
mod note_tools;
mod support;

use std::sync::Arc;

use serde_json::Value;

use crate::core::tool_registry::ToolRegistry;
use crate::services::notepad::NotepadService;

use self::folder_tools::register_folder_tools;
use self::note_tools::register_note_tools;

#[derive(Debug, Clone)]
pub struct NotepadOptions {
    pub server_name: String,
    pub user_id: Option<String>,
}

#[derive(Clone)]
pub struct NotepadBuiltinService {
    registry: ToolRegistry<ToolHandler>,
}

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

impl NotepadBuiltinService {
    pub fn new(opts: NotepadOptions) -> Result<Self, String> {
        let user_id = opts
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("builtin");
        let service = NotepadService::new(user_id)?;

        let mut out = Self {
            registry: ToolRegistry::new(),
        };

        register_folder_tools(&mut out, service.clone(), opts.server_name.as_str());
        register_note_tools(&mut out, service);

        Ok(out)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
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
}
