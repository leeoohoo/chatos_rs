mod folder_tools;
mod note_tools;
mod support;

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

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
            tools: HashMap::new(),
        };

        register_folder_tools(&mut out, service.clone(), opts.server_name.as_str());
        register_note_tools(&mut out, service);

        Ok(out)
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
}
