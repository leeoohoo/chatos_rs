// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{
    collections::{BTreeMap, BTreeSet},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

use crate::{
    schema::{function_schema, public_tool_name},
    LocalMcpExecutor, LocalMcpServerConfig, LocalMcpToolCall, LocalMcpToolResult,
};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(30);
const TOOL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const RESPONSE_LINE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const STDERR_TAIL_LIMIT_BYTES: usize = 16 * 1024;
const MAX_ARGUMENTS: usize = 128;
const MAX_ARGUMENT_BYTES: usize = 8 * 1024;
const MAX_ENVIRONMENT_ENTRIES: usize = 128;
const MAX_ENVIRONMENT_BYTES: usize = 64 * 1024;

struct ToolRegistration {
    schema: Value,
    upstream_name: String,
    server_name: String,
    session: Arc<Mutex<StdioSession>>,
}

pub struct StdioMcpExecutor {
    tools: BTreeMap<String, ToolRegistration>,
}

impl StdioMcpExecutor {
    pub async fn connect(
        servers: Vec<LocalMcpServerConfig>,
        allowed_tool_names: BTreeSet<String>,
    ) -> Result<Self, String> {
        if servers.is_empty() {
            return Err("local MCP executor requires at least one stdio server".to_string());
        }
        if allowed_tool_names.is_empty() {
            return Err("local MCP executor requires at least one frozen tool".to_string());
        }
        let mut server_names = BTreeSet::new();
        let mut tools = BTreeMap::new();
        for server in servers {
            validate_server(&server)?;
            if !server_names.insert(server.name.clone()) {
                return Err(format!("duplicate local MCP server {}", server.name));
            }
            let server_name = server.name.clone();
            let (session, definitions) = StdioSession::spawn(server).await?;
            let session = Arc::new(Mutex::new(session));
            for definition in definitions {
                let upstream_name = definition
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        format!("stdio MCP {server_name} returned a tool without a name")
                    })?
                    .to_string();
                let public_name = public_tool_name(server_name.as_str(), upstream_name.as_str());
                if !allowed_tool_names.contains(public_name.as_str()) {
                    continue;
                }
                let schema = function_schema(&definition, public_name.as_str())?;
                if tools
                    .insert(
                        public_name.clone(),
                        ToolRegistration {
                            schema,
                            upstream_name,
                            server_name: server_name.clone(),
                            session: session.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(format!("duplicate local MCP tool {public_name}"));
                }
            }
        }
        let missing = allowed_tool_names
            .iter()
            .filter(|name| !tools.contains_key(name.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "frozen local MCP tools are unavailable: {}",
                missing.join(",")
            ));
        }
        Ok(Self { tools })
    }
}

#[async_trait]
impl LocalMcpExecutor for StdioMcpExecutor {
    fn available_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|registration| registration.schema.clone())
            .collect()
    }

    async fn execute_tool(
        &self,
        call: LocalMcpToolCall,
        cancellation: CancellationToken,
    ) -> Result<LocalMcpToolResult, String> {
        if cancellation.is_cancelled() {
            return Err("local MCP invocation was cancelled".to_string());
        }
        validate_call(&call)?;
        let registration = self
            .tools
            .get(call.tool_name.as_str())
            .ok_or_else(|| format!("frozen local MCP tool {} is unavailable", call.tool_name))?;
        let request = json!({
            "name": registration.upstream_name,
            "arguments": call.arguments,
            "_meta": {
                "chatos/runId": call.run_id,
                "chatos/turnId": call.turn_id,
                "chatos/toolCallId": call.tool_call_id,
            }
        });
        let mut session = tokio::select! {
            () = cancellation.cancelled() => {
                return Err("local MCP invocation was cancelled".to_string());
            }
            session = registration.session.lock() => session,
        };
        let response = tokio::select! {
            () = cancellation.cancelled() => {
                return Err("local MCP invocation was cancelled".to_string());
            }
            result = tokio::time::timeout(
                TOOL_EXECUTION_TIMEOUT,
                session.request("tools/call", request),
            ) => match result {
                Ok(result) => result.map_err(|error| format!(
                    "stdio MCP {} tool {} failed: {error}",
                    registration.server_name,
                    call.tool_name,
                ))?,
                Err(_) => return Err(format!(
                    "stdio MCP {} tool {} timed out after {} seconds",
                    registration.server_name,
                    call.tool_name,
                    TOOL_EXECUTION_TIMEOUT.as_secs(),
                )),
            },
        };
        Ok(result_from_response(&response))
    }
}

struct StdioSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: AtomicU64,
    stderr_tail: Arc<Mutex<Vec<u8>>>,
}

impl StdioSession {
    async fn spawn(config: LocalMcpServerConfig) -> Result<(Self, Vec<Value>), String> {
        let mut command = build_command(&config);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        configure_process_group(&mut command);
        let mut child = command.spawn().map_err(|error| {
            format!(
                "stdio MCP {} could not start executable {}: {error}",
                config.name,
                config.executable.display()
            )
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("stdio MCP {} has no stdin", config.name))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("stdio MCP {} has no stdout", config.name))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("stdio MCP {} has no stderr", config.name))?;
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));
        tokio::spawn(drain_stderr(stderr, stderr_tail.clone()));
        let mut session = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: AtomicU64::new(1),
            stderr_tail,
        };
        let initialize = tokio::time::timeout(
            INITIALIZATION_TIMEOUT,
            session.request(
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {
                        "name": "chatos-local-agent-host",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            ),
        )
        .await
        .map_err(|_| format!("stdio MCP {} initialize timed out", config.name))??;
        validate_initialize(&config.name, &initialize)?;
        session
            .notify("notifications/initialized", None)
            .await
            .map_err(|error| {
                format!(
                    "stdio MCP {} initialization notification failed: {error}",
                    config.name
                )
            })?;
        let listed = tokio::time::timeout(
            INITIALIZATION_TIMEOUT,
            session.request("tools/list", json!({})),
        )
        .await
        .map_err(|_| format!("stdio MCP {} tools/list timed out", config.name))??;
        let tools = listed
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| format!("stdio MCP {} tools/list has no tools array", config.name))?;
        Ok((session, tools))
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if self.child.try_wait().is_ok_and(|status| status.is_some()) {
            return Err(self.error_with_stderr("process exited").await);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;
        loop {
            let line = read_line_limited(&mut self.stdout, RESPONSE_LINE_LIMIT_BYTES)
                .await?
                .ok_or_else(|| "stdio MCP closed stdout before replying".to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(line.as_str())
                .map_err(|error| format!("stdio MCP emitted invalid JSON: {error}"))?;
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(format!("JSON-RPC error: {error}"));
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| "stdio MCP response has no result".to_string());
        }
    }

    async fn notify(&mut self, method: &str, params: Option<Value>) -> Result<(), String> {
        let mut value = json!({"jsonrpc": "2.0", "method": method});
        if let Some(params) = params {
            value["params"] = params;
        }
        self.write_json(&value).await
    }

    async fn write_json(&mut self, value: &Value) -> Result<(), String> {
        let mut encoded = serde_json::to_vec(value)
            .map_err(|error| format!("stdio MCP request could not be encoded: {error}"))?;
        encoded.push(b'\n');
        self.stdin
            .write_all(encoded.as_slice())
            .await
            .map_err(|error| format!("stdio MCP stdin write failed: {error}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| format!("stdio MCP stdin flush failed: {error}"))
    }

    async fn error_with_stderr(&self, message: &str) -> String {
        let tail = self.stderr_tail.lock().await;
        if tail.is_empty() {
            message.to_string()
        } else {
            format!("{message}; stderr_tail={}", String::from_utf8_lossy(&tail))
        }
    }
}

impl Drop for StdioSession {
    fn drop(&mut self) {
        terminate_process_tree(&mut self.child);
    }
}

fn validate_server(config: &LocalMcpServerConfig) -> Result<(), String> {
    if config.name.is_empty()
        || config.name.trim() != config.name
        || config.name.chars().any(char::is_control)
    {
        return Err("local MCP server name is invalid".to_string());
    }
    if !config.executable.is_absolute() || !config.working_directory.is_absolute() {
        return Err(format!(
            "local MCP server {} requires absolute executable and working directory paths",
            config.name
        ));
    }
    if config.arguments.len() > MAX_ARGUMENTS
        || config
            .arguments
            .iter()
            .any(|value| value.len() > MAX_ARGUMENT_BYTES || value.contains('\0'))
    {
        return Err(format!(
            "local MCP server {} arguments are invalid",
            config.name
        ));
    }
    if config.environment.len() > MAX_ENVIRONMENT_ENTRIES
        || config
            .environment
            .iter()
            .map(|(name, value)| name.len().saturating_add(value.len()))
            .sum::<usize>()
            > MAX_ENVIRONMENT_BYTES
    {
        return Err(format!(
            "local MCP server {} environment is too large",
            config.name
        ));
    }
    for (name, value) in &config.environment {
        if !valid_environment_name(name) || value.contains('\0') {
            return Err(format!(
                "local MCP server {} environment entry is invalid",
                config.name
            ));
        }
    }
    Ok(())
}

fn validate_call(call: &LocalMcpToolCall) -> Result<(), String> {
    for (field, value) in [
        ("tool_call_id", call.tool_call_id.as_str()),
        ("tool_name", call.tool_name.as_str()),
        ("run_id", call.run_id.as_str()),
        ("turn_id", call.turn_id.as_str()),
    ] {
        if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
            return Err(format!("local MCP {field} is invalid"));
        }
    }
    if !call.arguments.is_object() {
        return Err("local MCP tool arguments must be an object".to_string());
    }
    Ok(())
}

fn valid_environment_name(name: &str) -> bool {
    let valid = !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    let upper = name.to_ascii_uppercase();
    let controlled = matches!(
        upper.as_str(),
        "PATH"
            | "HOME"
            | "SHELL"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "COMSPEC"
            | "PATHEXT"
            | "SYSTEMROOT"
            | "WINDIR"
            | "USERPROFILE"
            | "APPDATA"
            | "LOCALAPPDATA"
            | "NODE_OPTIONS"
            | "PYTHONHOME"
            | "PYTHONPATH"
            | "RUBYOPT"
            | "PERL5OPT"
            | "BASH_ENV"
            | "ENV"
            | "PROMPT_COMMAND"
    ) || upper.starts_with("LD_")
        || upper.starts_with("DYLD_")
        || upper.starts_with("XDG_");
    valid && !controlled
}

fn build_command(config: &LocalMcpServerConfig) -> Command {
    let mut command = Command::new(config.executable.as_os_str());
    command
        .args(&config.arguments)
        .current_dir(config.working_directory.as_path())
        .env_clear();
    for name in baseline_environment_names() {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.envs(&config.environment);
    command
}

fn baseline_environment_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &[
            "PATH",
            "SYSTEMROOT",
            "WINDIR",
            "COMSPEC",
            "PATHEXT",
            "TEMP",
            "TMP",
        ]
    }
    #[cfg(not(windows))]
    {
        &["PATH", "TMPDIR", "TMP", "TEMP"]
    }
}

fn validate_initialize(server_name: &str, result: &Value) -> Result<(), String> {
    let protocol = result
        .get("protocolVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("stdio MCP {server_name} did not return protocolVersion"))?;
    if !SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol) {
        return Err(format!(
            "stdio MCP {server_name} returned unsupported protocolVersion {protocol}"
        ));
    }
    if !result.get("capabilities").is_some_and(Value::is_object) {
        return Err(format!(
            "stdio MCP {server_name} did not return object capabilities"
        ));
    }
    Ok(())
}

fn result_from_response(response: &Value) -> LocalMcpToolResult {
    let content = response
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                (item.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| item.get("text").and_then(Value::as_str))
                    .flatten()
            })
        })
        .or_else(|| response.get("text").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| response.to_string());
    let structured_result = response
        .get("structuredContent")
        .or_else(|| response.get("_structured_result"))
        .cloned();
    LocalMcpToolResult {
        content,
        structured_result,
        is_error: response
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fatal_error: false,
    }
}

async fn read_line_limited<R>(
    reader: &mut R,
    maximum_bytes: usize,
) -> Result<Option<String>, String>
where
    R: AsyncBufRead + Unpin,
{
    let mut bytes = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|error| format!("stdio MCP stdout read failed: {error}"))?;
        if available.is_empty() {
            if bytes.is_empty() {
                return Ok(None);
            }
            break;
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|position| position + 1)
            .unwrap_or(available.len());
        if bytes.len().saturating_add(count) > maximum_bytes {
            return Err(format!(
                "stdio MCP response line exceeds {maximum_bytes} bytes"
            ));
        }
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if bytes.last() == Some(&b'\n') {
            break;
        }
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| format!("stdio MCP response is not UTF-8: {error}"))
}

async fn drain_stderr(mut stderr: tokio::process::ChildStderr, tail: Arc<Mutex<Vec<u8>>>) {
    let mut chunk = [0_u8; 1024];
    loop {
        let Ok(count) = stderr.read(&mut chunk).await else {
            break;
        };
        if count == 0 {
            break;
        }
        let mut tail = tail.lock().await;
        tail.extend_from_slice(&chunk[..count]);
        if tail.len() > STDERR_TAIL_LIMIT_BYTES {
            let excess = tail.len() - STDERR_TAIL_LIMIT_BYTES;
            tail.drain(..excess);
        }
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.as_std_mut().process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_tree(child: &mut Child) {
    if let Some(process_id) = child.id() {
        unsafe {
            libc::kill(-(process_id as i32), libc::SIGKILL);
        }
    }
    let _ = child.start_kill();
}

#[cfg(not(unix))]
fn terminate_process_tree(child: &mut Child) {
    let _ = child.start_kill();
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf};

    use super::{result_from_response, validate_server};
    use crate::LocalMcpServerConfig;
    use serde_json::json;

    #[test]
    fn server_debug_and_validation_do_not_expose_environment_values() {
        let config = LocalMcpServerConfig {
            name: "plugin-test-tools".to_string(),
            executable: PathBuf::from("/absolute/plugin"),
            arguments: Vec::new(),
            working_directory: PathBuf::from("/absolute"),
            environment: BTreeMap::from([(
                "PLUGIN_TOKEN".to_string(),
                "private-value".to_string(),
            )]),
        };
        validate_server(&config).expect("valid server");
        let debug = format!("{config:?}");
        assert!(debug.contains("PLUGIN_TOKEN"));
        assert!(!debug.contains("private-value"));
    }

    #[test]
    fn result_preserves_text_structured_content_and_error_state() {
        let result = result_from_response(&json!({
            "content": [{"type": "text", "text": "done"}],
            "structuredContent": {"changed": true},
            "isError": true
        }));
        assert_eq!(result.content, "done");
        assert_eq!(result.structured_result, Some(json!({"changed": true})));
        assert!(result.is_error);
    }
}
