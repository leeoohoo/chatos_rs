use std::collections::HashMap;
use std::process::Stdio;

use serde_json::{Value, json};
use serde::Serialize;
use tracing::{info, warn};
use uuid::Uuid;

use crate::builtin::code_maintainer::{CodeMaintainerOptions, CodeMaintainerService};
use crate::services::mcp_loader::{McpBuiltinServer, McpHttpServer, McpStdioServer};
use crate::utils::abort_registry;

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub original_name: String,
    pub server_name: String,
    pub server_type: String,
    pub server_url: Option<String>,
    pub server_config: Option<McpStdioServer>,
    pub tool_info: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub success: bool,
    pub is_error: bool,
    pub content: String,
}

#[derive(Clone)]
pub struct McpToolExecute {
    pub mcp_servers: Vec<McpHttpServer>,
    pub stdio_mcp_servers: Vec<McpStdioServer>,
    pub builtin_mcp_servers: Vec<McpBuiltinServer>,
    pub tools: Vec<Value>,
    pub tool_metadata: HashMap<String, ToolInfo>,
    builtin_services: HashMap<String, CodeMaintainerService>,
}

impl McpToolExecute {
    pub fn new(
        mcp_servers: Vec<McpHttpServer>,
        stdio_mcp_servers: Vec<McpStdioServer>,
        builtin_mcp_servers: Vec<McpBuiltinServer>,
    ) -> Self {
        Self {
            mcp_servers,
            stdio_mcp_servers,
            builtin_mcp_servers,
            tools: Vec::new(),
            tool_metadata: HashMap::new(),
            builtin_services: HashMap::new(),
        }
    }

    pub async fn init(&mut self) -> Result<(), String> {
        self.build_tools().await
    }

    pub async fn build_tools(&mut self) -> Result<(), String> {
        self.tools.clear();
        self.tool_metadata.clear();
        self.builtin_services.clear();

        let http_servers = self.mcp_servers.clone();
        for server in &http_servers {
            if let Err(err) = self.build_tools_from_http(server).await {
                warn!("failed to build tools from http {}: {}", server.name, err);
            }
        }
        let stdio_servers = self.stdio_mcp_servers.clone();
        for server in &stdio_servers {
            if let Err(err) = self.build_tools_from_stdio(server).await {
                warn!("failed to build tools from stdio {}: {}", server.name, err);
            }
        }
        let builtin_servers = self.builtin_mcp_servers.clone();
        for server in &builtin_servers {
            if let Err(err) = self.build_tools_from_builtin(server) {
                warn!("failed to build tools from builtin {}: {}", server.name, err);
            }
        }
        info!("MCP tools built: {}", self.tools.len());
        Ok(())
    }

    async fn build_tools_from_http(&mut self, server: &McpHttpServer) -> Result<(), String> {
        let tools = list_tools_http(&server.url).await?;
        for tool in tools {
            let tool_name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if tool_name.is_empty() { continue; }
            let prefixed = format!("{}_{}", server.name, tool_name);
            let parameters = tool.get("inputSchema").cloned().unwrap_or(json!({"type":"object","properties":{},"required":[]}));
            let description = tool.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let openai_tool = json!({
                "type": "function",
                "function": {
                    "name": prefixed,
                    "description": description,
                    "parameters": parameters
                }
            });
            self.tools.push(openai_tool);
            self.tool_metadata.insert(prefixed.clone(), ToolInfo {
                original_name: tool_name,
                server_name: server.name.clone(),
                server_type: "http".to_string(),
                server_url: Some(server.url.clone()),
                server_config: None,
                tool_info: tool.clone(),
            });
        }
        Ok(())
    }

    async fn build_tools_from_stdio(&mut self, server: &McpStdioServer) -> Result<(), String> {
        let tools = list_tools_stdio(server).await?;
        for tool in tools {
            let tool_name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if tool_name.is_empty() { continue; }
            let prefixed = format!("{}_{}", server.name, tool_name);
            let parameters = tool.get("inputSchema").cloned().unwrap_or(json!({"type":"object","properties":{},"required":[]}));
            let description = tool.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let openai_tool = json!({
                "type": "function",
                "function": {
                    "name": prefixed,
                    "description": description,
                    "parameters": parameters
                }
            });
            self.tools.push(openai_tool);
            self.tool_metadata.insert(prefixed.clone(), ToolInfo {
                original_name: tool_name,
                server_name: server.name.clone(),
                server_type: "stdio".to_string(),
                server_url: None,
                server_config: Some(server.clone()),
                tool_info: tool.clone(),
            });
        }
        Ok(())
    }

    fn build_tools_from_builtin(&mut self, server: &McpBuiltinServer) -> Result<(), String> {
        let service = CodeMaintainerService::new(CodeMaintainerOptions {
            server_name: server.name.clone(),
            root: std::path::PathBuf::from(&server.workspace_dir),
            allow_writes: server.allow_writes,
            max_file_bytes: server.max_file_bytes,
            max_write_bytes: server.max_write_bytes,
            search_limit: server.search_limit,
            session_id: None,
            run_id: None,
            db_path: None,
        })?;
        let tools = service.list_tools();
        self.builtin_services.insert(server.name.clone(), service);
        for tool in tools {
            let tool_name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if tool_name.is_empty() { continue; }
            let prefixed = format!("{}_{}", server.name, tool_name);
            let parameters = tool.get("inputSchema").cloned().unwrap_or(json!({"type":"object","properties":{},"required":[]}));
            let description = tool.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let openai_tool = json!({
                "type": "function",
                "function": {
                    "name": prefixed,
                    "description": description,
                    "parameters": parameters
                }
            });
            self.tools.push(openai_tool);
            self.tool_metadata.insert(prefixed.clone(), ToolInfo {
                original_name: tool_name,
                server_name: server.name.clone(),
                server_type: "builtin".to_string(),
                server_url: None,
                server_config: None,
                tool_info: tool.clone(),
            });
        }
        Ok(())
    }

    pub fn get_available_tools(&self) -> Vec<Value> {
        self.tools.clone()
    }

    pub async fn execute_tools_stream(
        &self,
        tool_calls: &[Value],
        session_id: Option<&str>,
        on_tool_result: Option<std::sync::Arc<dyn Fn(&ToolResult) + Send + Sync>>,
    ) -> Vec<ToolResult> {
        let mut results = Vec::new();
        for tc in tool_calls {
            if let Some(sid) = session_id {
                if abort_registry::is_aborted(sid) { break; }
            }
            let tool_name = tc.get("function").and_then(|f| f.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let call_id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if tool_name.is_empty() {
                let result = ToolResult { tool_call_id: call_id, name: "unknown".to_string(), success: false, is_error: true, content: "工具名称不能为空".to_string() };
                results.push(result);
                if let Some(cb) = &on_tool_result {
                    let should_call = session_id.map(|sid| !abort_registry::is_aborted(sid)).unwrap_or(true);
                    if should_call {
                        if let Some(last) = results.last() { cb(last); }
                    }
                }
                continue;
            }
            let args_val = tc.get("function").and_then(|f| f.get("arguments")).cloned().unwrap_or(Value::String("{}".to_string()));
            let args: Value = if let Some(s) = args_val.as_str() {
                match serde_json::from_str::<Value>(s) {
                    Ok(v) => v,
                    Err(err) => {
                        let result = ToolResult {
                            tool_call_id: call_id.clone(),
                            name: tool_name.clone(),
                            success: false,
                            is_error: true,
                            content: format!("参数解析失败: {}", err),
                        };
                        results.push(result);
                        if let Some(cb) = &on_tool_result {
                            let should_call = session_id.map(|sid| !abort_registry::is_aborted(sid)).unwrap_or(true);
                            if should_call {
                                if let Some(last) = results.last() { cb(last); }
                            }
                        }
                        continue;
                    }
                }
            } else {
                args_val
            };
            match self.call_tool_once(&tool_name, args, session_id).await {
                Ok(text) => {
                    let result = ToolResult { tool_call_id: call_id, name: tool_name, success: true, is_error: false, content: text };
                    results.push(result);
                    if let Some(cb) = &on_tool_result {
                        let should_call = session_id.map(|sid| !abort_registry::is_aborted(sid)).unwrap_or(true);
                        if should_call {
                            if let Some(last) = results.last() { cb(last); }
                        }
                    }
                }
                Err(err) => {
                    if err == "aborted" {
                        break;
                    }
                    let result = ToolResult { tool_call_id: call_id, name: tool_name, success: false, is_error: true, content: format!("工具执行失败: {}", err) };
                    results.push(result);
                    if let Some(cb) = &on_tool_result {
                        let should_call = session_id.map(|sid| !abort_registry::is_aborted(sid)).unwrap_or(true);
                        if should_call {
                            if let Some(last) = results.last() { cb(last); }
                        }
                    }
                }
            }
        }
        results
    }

    async fn call_tool_once(&self, tool_name: &str, args: Value, session_id: Option<&str>) -> Result<String, String> {
        let info = self.tool_metadata.get(tool_name).ok_or_else(|| format!("工具未找到: {}", tool_name))?;
        if info.server_type == "http" {
            let url = info.server_url.clone().ok_or("missing server url")?;
            let result = jsonrpc_http_call(&url, "tools/call", json!({"name": info.original_name, "arguments": args})).await?;
            Ok(to_text(&result))
        } else if info.server_type == "builtin" {
            let service = self
                .builtin_services
                .get(&info.server_name)
                .ok_or_else(|| "missing builtin service".to_string())?;
            let result = service.call_tool(&info.original_name, args, session_id)?;
            Ok(to_text(&result))
        } else {
            let config = info.server_config.clone().ok_or("missing server config")?;
            let result = jsonrpc_stdio_call(&config, "tools/call", json!({"name": info.original_name, "arguments": args}), session_id).await?;
            Ok(to_text(&result))
        }
    }
}

async fn list_tools_http(url: &str) -> Result<Vec<Value>, String> {
    let resp = jsonrpc_http_call(url, "tools/list", json!({})).await?;
    extract_tools(&resp)
}

async fn list_tools_stdio(cfg: &McpStdioServer) -> Result<Vec<Value>, String> {
    let resp = jsonrpc_stdio_call(cfg, "tools/list", json!({}), None).await?;
    extract_tools(&resp)
}

fn extract_tools(resp: &Value) -> Result<Vec<Value>, String> {
    if let Some(arr) = resp.get("tools").and_then(|v| v.as_array()) {
        return Ok(arr.clone());
    }
    if let Some(arr) = resp.get("result").and_then(|r| r.get("tools")).and_then(|v| v.as_array()) {
        return Ok(arr.clone());
    }
    Err("tools not found in response".to_string())
}

fn to_text(result: &Value) -> String {
    if let Some(s) = result.as_str() { return s.to_string(); }
    if let Some(content) = result.get("content").and_then(|v| v.as_array()) {
        for c in content {
            if c.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(t) = c.get("text").and_then(|v| v.as_str()) { return t.to_string(); }
                if let Some(t) = c.get("value").and_then(|v| v.as_str()) { return t.to_string(); }
            }
        }
    }
    if let Some(text) = result.get("text").and_then(|v| v.as_str()) { return text.to_string(); }
    if let Some(v) = result.get("value").and_then(|v| v.as_str()) { return v.to_string(); }
    result.to_string()
}

async fn jsonrpc_http_call(url: &str, method: &str, params: Value) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    let resp = reqwest::Client::new().post(url).json(&payload).send().await.map_err(|e| e.to_string())?;
    let val: Value = resp.json().await.map_err(|e| e.to_string())?;
    if val.get("error").is_some() {
        return Err(val.to_string());
    }
    Ok(val.get("result").cloned().unwrap_or(val))
}

async fn jsonrpc_stdio_call(cfg: &McpStdioServer, method: &str, params: Value, session_id: Option<&str>) -> Result<Value, String> {
    let id = Uuid::new_v4().to_string();
    let payload = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});

    let mut cmd = tokio::process::Command::new(&cfg.command);
    if let Some(args) = &cfg.args {
        cmd.args(args);
    }
    if let Some(env) = &cfg.env {
        cmd.envs(env);
    }
    if let Some(cwd) = &cfg.cwd {
        cmd.current_dir(cwd);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let data = payload.to_string() + "\n";
        use tokio::io::AsyncWriteExt;
        stdin.write_all(data.as_bytes()).await.map_err(|e| e.to_string())?;
    }

    use tokio::io::{AsyncBufReadExt, BufReader};
    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let mut reader = BufReader::new(stdout).lines();
    loop {
        if let Some(sid) = session_id {
            if abort_registry::is_aborted(sid) { return Err("aborted".to_string()); }
        }
        match reader.next_line().await {
            Ok(Some(line)) => {
                if line.trim().is_empty() { continue; }
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if v.get("id").and_then(|v| v.as_str()) == Some(&id) {
                        if v.get("error").is_some() { return Err(v.to_string()); }
                        return Ok(v.get("result").cloned().unwrap_or(v));
                    }
                }
            }
            Ok(None) => break,
            Err(err) => return Err(err.to_string()),
        }
    }
    Err("no response from stdio server".to_string())
}

