use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

pub type ToolHandler = Box<dyn Fn(Value) -> Result<Value, String>>;

pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub handler: ToolHandler,
}

pub struct McpServer {
    name: String,
    version: String,
    tools: HashMap<String, Tool>,
}

impl McpServer {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            tools: HashMap::new(),
        }
    }

    pub fn register_tool(
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

    pub fn run_stdio(&self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        for line in stdin.lock().lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let request: Value = match serde_json::from_str(trimmed) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("[mcp] invalid json: {err}");
                    continue;
                }
            };
            if let Some(response) = self.handle_request(request) {
                let serialized = match serde_json::to_string(&response) {
                    Ok(text) => text,
                    Err(err) => {
                        eprintln!("[mcp] failed to serialize response: {err}");
                        continue;
                    }
                };
                stdout.write_all(serialized.as_bytes())?;
                stdout.write_all(b"\n")?;
                stdout.flush()?;
            }
        }
        Ok(())
    }

    fn handle_request(&self, request: Value) -> Option<Value> {
        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if id.is_none() {
            return None;
        }
        let id_val = id.unwrap();
        match method {
            "initialize" => {
                let result = json!({
                    "serverInfo": {
                        "name": self.name,
                        "version": self.version,
                    },
                    "capabilities": {
                        "tools": { "list": true, "call": true }
                    }
                });
                Some(ok(id_val, result))
            }
            "tools/list" => {
                let tools: Vec<Value> = self
                    .tools
                    .values()
                    .map(|tool| {
                        json!({
                            "name": tool.name,
                            "description": tool.description,
                            "inputSchema": tool.input_schema,
                        })
                    })
                    .collect();
                Some(ok(id_val, json!({ "tools": tools })))
            }
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let tool = match self.tools.get(name) {
                    Some(tool) => tool,
                    None => {
                        return Some(err(
                            id_val,
                            -32601,
                            format!("Tool not found: {name}"),
                        ));
                    }
                };
                match (tool.handler)(args) {
                    Ok(result) => Some(ok(id_val, result)),
                    Err(message) => Some(err(id_val, -32603, message)),
                }
            }
            "ping" => Some(ok(id_val, json!({}))),
            _ => Some(err(
                id_val,
                -32601,
                format!("Method not found: {method}"),
            )),
        }
    }
}

fn ok(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn err(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}
