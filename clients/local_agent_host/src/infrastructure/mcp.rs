// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Client-owned MCP stdio process and session runtime.
//!
//! Marketplace and release metadata may come from the retained Plugin control
//! plane, but process startup, MCP initialization, tool discovery and tool
//! execution happen here on the user's device.

use crate::{LocalToolExecutor, LocalToolRegistry};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    validate_identifier, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LocalPluginInstallationRecord,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 300_000;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOOL_PAGES: usize = 100;
const CHILD_REAP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalMcpServerConfig {
    pub server_id: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub environment: HashMap<String, String>,
    pub inherit_environment: bool,
    pub tool_prefix: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub startup_timeout_ms: u64,
    pub call_timeout_ms: u64,
}

impl LocalMcpServerConfig {
    pub fn new(server_id: impl Into<String>, command: impl Into<PathBuf>) -> Self {
        Self {
            server_id: server_id.into(),
            command: command.into(),
            args: Vec::new(),
            cwd: None,
            environment: HashMap::new(),
            inherit_environment: false,
            tool_prefix: None,
            allowed_tools: None,
            startup_timeout_ms: DEFAULT_TIMEOUT_MS,
            call_timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("MCP server_id", &self.server_id)?;
        if self.command.as_os_str().is_empty() {
            return Err("MCP command must not be empty".to_string());
        }
        if let Some(prefix) = self.tool_prefix.as_deref() {
            validate_identifier("MCP tool_prefix", prefix)?;
        }
        validate_timeout("startup_timeout_ms", self.startup_timeout_ms)?;
        validate_timeout("call_timeout_ms", self.call_timeout_ms)?;
        if self.args.len() > 256 {
            return Err("MCP args must contain at most 256 entries".to_string());
        }
        if self.environment.len() > 256 {
            return Err("MCP environment must contain at most 256 entries".to_string());
        }
        for (key, value) in &self.environment {
            if key.trim().is_empty() || key.contains('=') || key.contains('\0') {
                return Err(
                    "MCP environment keys must be non-empty and cannot contain '=' or NUL"
                        .to_string(),
                );
            }
            if value.contains('\0') {
                return Err("MCP environment values cannot contain NUL".to_string());
            }
        }
        if let Some(allowed) = self.allowed_tools.as_deref() {
            if allowed.is_empty() || allowed.len() > 256 {
                return Err("MCP allowed_tools must contain 1..=256 entries".to_string());
            }
            let mut unique = HashSet::with_capacity(allowed.len());
            for name in allowed {
                validate_identifier("MCP allowed tool", name)?;
                if !unique.insert(name.as_str()) {
                    return Err(format!("MCP allowed_tools contains duplicate: {name}"));
                }
            }
        }
        Ok(())
    }

    fn public_tool_name(&self, original_name: &str) -> Result<String, String> {
        let prefix = self.tool_prefix.as_deref().unwrap_or(&self.server_id);
        let name = format!("{prefix}__{original_name}");
        validate_identifier("MCP public tool name", &name)?;
        Ok(name)
    }

    async fn from_installation<R>(
        installation: &LocalPluginInstallationRecord,
        secrets: &R,
    ) -> Result<Self, String>
    where
        R: LocalPluginSecretResolver + ?Sized,
    {
        installation.spec.validate()?;
        if !installation.spec.enabled {
            return Err(format!(
                "plugin installation is disabled: {}",
                installation.spec.installation_id
            ));
        }
        let mut environment = HashMap::new();
        for (variable, secret_ref) in &installation.spec.environment_secret_refs {
            let value = secrets
                .resolve_secret(&installation.spec.owner_user_id, secret_ref)
                .await?;
            if value.contains('\0') {
                return Err(format!("resolved plugin secret contains NUL: {secret_ref}"));
            }
            environment.insert(variable.clone(), value);
        }
        let mut config = Self::new(
            installation.spec.server_id.clone(),
            installation.spec.executable_path.clone(),
        );
        config.args = installation.spec.args.clone();
        config.cwd = installation
            .spec
            .working_directory
            .as_ref()
            .map(PathBuf::from);
        config.environment = environment;
        config.tool_prefix = installation.spec.tool_prefix.clone();
        config.allowed_tools = installation.spec.allowed_tools.clone();
        config.validate()?;
        Ok(config)
    }
}

#[async_trait]
pub trait LocalPluginSecretResolver: Send + Sync {
    /// Resolves a native credential-store reference for one process launch.
    /// Implementations must not persist or log the returned value.
    async fn resolve_secret(&self, owner_user_id: &str, secret_ref: &str)
        -> Result<String, String>;
}

fn validate_timeout(field: &str, value: u64) -> Result<(), String> {
    if !(1_000..=MAX_TIMEOUT_MS).contains(&value) {
        return Err(format!("{field} must be between 1000 and {MAX_TIMEOUT_MS}"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalMcpToolDefinition {
    pub public_name: String,
    pub server_id: String,
    pub original_name: String,
    pub description: String,
    pub input_schema: Value,
}

impl LocalMcpToolDefinition {
    pub fn model_tool(&self) -> Value {
        json!({
            "type": "function",
            "name": self.public_name,
            "description": self.description,
            "parameters": self.input_schema
        })
    }
}

pub struct LocalMcpToolSet {
    session: Arc<dyn LocalMcpClient>,
    definitions: Vec<LocalMcpToolDefinition>,
}

impl LocalMcpToolSet {
    pub fn definitions(&self) -> &[LocalMcpToolDefinition] {
        &self.definitions
    }

    pub fn model_tools(&self) -> Vec<Value> {
        self.definitions
            .iter()
            .map(LocalMcpToolDefinition::model_tool)
            .collect()
    }

    pub fn register_into(&self, registry: &mut LocalToolRegistry) -> Result<(), String> {
        for definition in &self.definitions {
            registry.register_shared(
                definition.public_name.clone(),
                Arc::new(LocalMcpToolExecutor {
                    session: Arc::clone(&self.session),
                    public_name: definition.public_name.clone(),
                    original_name: definition.original_name.clone(),
                }),
            )?;
        }
        Ok(())
    }
}

pub struct LocalMcpStdioSession {
    server_id: String,
    call_timeout: Duration,
    connection: Mutex<StdioConnection>,
}

struct StdioConnection {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_request_id: u64,
    unusable: bool,
}

impl LocalMcpStdioSession {
    pub async fn connect_installation<R>(
        installation: &LocalPluginInstallationRecord,
        secrets: &R,
    ) -> Result<LocalMcpToolSet, String>
    where
        R: LocalPluginSecretResolver + ?Sized,
    {
        Self::connect(LocalMcpServerConfig::from_installation(installation, secrets).await?).await
    }

    pub async fn connect(config: LocalMcpServerConfig) -> Result<LocalMcpToolSet, String> {
        config.validate()?;
        if let Some(cwd) = config.cwd.as_deref() {
            if !cwd.is_dir() {
                return Err(format!("MCP cwd is not a directory: {}", cwd.display()));
            }
        }
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if !config.inherit_environment {
            command.env_clear();
        }
        command.envs(&config.environment);
        if let Some(cwd) = config.cwd.as_deref() {
            command.current_dir(cwd);
        }
        let mut child = command.spawn().map_err(|error| {
            format!(
                "start local MCP server {} failed: {error}",
                config.server_id
            )
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("local MCP server {} has no stdin", config.server_id))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("local MCP server {} has no stdout", config.server_id))?;
        let session = Arc::new(Self {
            server_id: config.server_id.clone(),
            call_timeout: Duration::from_millis(config.call_timeout_ms),
            connection: Mutex::new(StdioConnection {
                child,
                stdin,
                stdout: BufReader::new(stdout),
                next_request_id: 1,
                unusable: false,
            }),
        });
        session
            .initialize(Duration::from_millis(config.startup_timeout_ms))
            .await
            .map_err(|error| error.message)?;
        let tools = session
            .list_tools(Duration::from_millis(config.startup_timeout_ms))
            .await
            .map_err(|error| error.message)?;
        let definitions = decode_tool_definitions(&config, tools)?;
        let client: Arc<dyn LocalMcpClient> = session;
        Ok(LocalMcpToolSet {
            session: client,
            definitions,
        })
    }

    async fn initialize(&self, timeout: Duration) -> Result<(), McpCallError> {
        let result = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "chatos-local-agent-host", "version": env!("CARGO_PKG_VERSION")}
                }),
                timeout,
            )
            .await?;
        if result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .is_none()
        {
            return Err(McpCallError::known(format!(
                "MCP server {} returned an invalid initialize result",
                self.server_id
            )));
        }
        self.notify("notifications/initialized", json!({}), timeout)
            .await
    }

    async fn list_tools(&self, timeout: Duration) -> Result<Vec<Value>, McpCallError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_TOOL_PAGES {
            let params = cursor
                .as_ref()
                .map(|value| json!({"cursor": value}))
                .unwrap_or_else(|| json!({}));
            let result = self.request("tools/list", params, timeout).await?;
            let page = result
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| McpCallError::known("MCP tools/list result is missing tools"))?;
            tools.extend(page.iter().cloned());
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|value| !value.is_empty());
            if cursor.is_none() {
                return Ok(tools);
            }
        }
        Err(McpCallError::known(
            "MCP tools/list exceeded the 100 page safety limit",
        ))
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, McpCallError> {
        let future = async {
            let mut connection = self.connection.lock().await;
            if connection.unusable {
                return Err(McpCallError::unknown(format!(
                    "MCP session {} is no longer usable",
                    self.server_id
                )));
            }
            let request_id = connection.next_request_id;
            connection.next_request_id = connection.next_request_id.saturating_add(1);
            let payload = json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": method,
                "params": params
            });
            write_message(&mut connection.stdin, &payload).await?;
            loop {
                let response = read_message(&mut connection.stdout).await?;
                if response.get("id").and_then(Value::as_u64) != Some(request_id) {
                    continue;
                }
                if let Some(error) = response.get("error") {
                    return Err(McpCallError::known(format!("MCP {method} failed: {error}")));
                }
                return response.get("result").cloned().ok_or_else(|| {
                    McpCallError::known(format!("MCP {method} response is missing result"))
                });
            }
        };
        match tokio::time::timeout(timeout, future).await {
            Ok(Err(error)) if !error.outcome_known => {
                self.invalidate().await;
                Err(error)
            }
            Ok(result) => result,
            Err(_) => {
                self.invalidate().await;
                Err(McpCallError::unknown(format!(
                    "MCP {method} timed out after {} ms",
                    timeout.as_millis()
                )))
            }
        }
    }

    async fn notify(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<(), McpCallError> {
        let future = async {
            let mut connection = self.connection.lock().await;
            if connection.unusable {
                return Err(McpCallError::unknown(format!(
                    "MCP session {} is no longer usable",
                    self.server_id
                )));
            }
            write_message(
                &mut connection.stdin,
                &json!({"jsonrpc": "2.0", "method": method, "params": params}),
            )
            .await
        };
        match tokio::time::timeout(timeout, future).await {
            Ok(Err(error)) if !error.outcome_known => {
                self.invalidate().await;
                Err(error)
            }
            Ok(result) => result,
            Err(_) => {
                self.invalidate().await;
                Err(McpCallError::unknown(format!(
                    "MCP {method} notification timed out"
                )))
            }
        }
    }

    async fn invalidate(&self) {
        let mut connection = self.connection.lock().await;
        connection.unusable = true;
        let _ = connection.child.start_kill();
        let _ = tokio::time::timeout(CHILD_REAP_TIMEOUT, connection.child.wait()).await;
    }
}

#[async_trait]
trait LocalMcpClient: Send + Sync {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, McpCallError>;
}

#[async_trait]
impl LocalMcpClient for LocalMcpStdioSession {
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, McpCallError> {
        self.request(
            "tools/call",
            json!({"name": name, "arguments": arguments}),
            self.call_timeout,
        )
        .await
    }
}

struct LocalMcpToolExecutor {
    session: Arc<dyn LocalMcpClient>,
    public_name: String,
    original_name: String,
}

#[async_trait]
impl LocalToolExecutor for LocalMcpToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        if invocation.tool_name != self.public_name {
            return Err(format!(
                "MCP executor for {} received {}",
                self.public_name, invocation.tool_name
            ));
        }
        match self
            .session
            .call_tool(&self.original_name, invocation.arguments.clone())
            .await
        {
            Ok(result) if result.get("isError").and_then(Value::as_bool) == Some(true) => {
                Ok(LocalAgentToolOutcome::Failed {
                    error: mcp_error_text(&result),
                    detail: result,
                })
            }
            Ok(result) => Ok(LocalAgentToolOutcome::Succeeded { output: result }),
            Err(error) if error.outcome_known => Ok(LocalAgentToolOutcome::Failed {
                error: error.message,
                detail: json!({"phase": "local_mcp_tool_call"}),
            }),
            Err(error) => Err(error.message),
        }
    }
}

#[derive(Debug)]
struct McpCallError {
    message: String,
    outcome_known: bool,
}

impl McpCallError {
    fn known(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            outcome_known: true,
        }
    }

    fn unknown(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            outcome_known: false,
        }
    }
}

fn decode_tool_definitions(
    config: &LocalMcpServerConfig,
    tools: Vec<Value>,
) -> Result<Vec<LocalMcpToolDefinition>, String> {
    let allowed = config
        .allowed_tools
        .as_ref()
        .map(|values| values.iter().map(String::as_str).collect::<HashSet<_>>());
    let mut names = HashSet::new();
    let mut definitions = Vec::new();
    for tool in tools {
        let original_name = tool
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "MCP tool definition is missing name".to_string())?;
        validate_identifier("MCP tool name", original_name)?;
        if allowed
            .as_ref()
            .is_some_and(|values| !values.contains(original_name))
        {
            continue;
        }
        if !names.insert(original_name.to_string()) {
            return Err(format!(
                "MCP server {} returned duplicate tool: {original_name}",
                config.server_id
            ));
        }
        definitions.push(LocalMcpToolDefinition {
            public_name: config.public_tool_name(original_name)?,
            server_id: config.server_id.clone(),
            original_name: original_name.to_string(),
            description: tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            input_schema: tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
        });
    }
    if let Some(allowed) = allowed {
        let missing = allowed
            .difference(&names.iter().map(String::as_str).collect())
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "MCP server {} did not publish allowed tools: {}",
                config.server_id,
                missing.join(", ")
            ));
        }
    }
    Ok(definitions)
}

async fn write_message(stdin: &mut ChildStdin, value: &Value) -> Result<(), McpCallError> {
    let mut payload = serde_json::to_vec(value)
        .map_err(|error| McpCallError::known(format!("encode MCP request failed: {error}")))?;
    payload.push(b'\n');
    stdin
        .write_all(&payload)
        .await
        .map_err(|error| McpCallError::unknown(format!("write MCP request failed: {error}")))?;
    stdin
        .flush()
        .await
        .map_err(|error| McpCallError::unknown(format!("flush MCP request failed: {error}")))
}

async fn read_message(stdout: &mut BufReader<ChildStdout>) -> Result<Value, McpCallError> {
    let mut payload = Vec::new();
    loop {
        let available = stdout
            .fill_buf()
            .await
            .map_err(|error| McpCallError::unknown(format!("read MCP response failed: {error}")))?;
        if available.is_empty() {
            return Err(McpCallError::unknown(
                "MCP server closed stdout before responding",
            ));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let content_length = newline.unwrap_or(available.len());
        if payload.len().saturating_add(content_length) > MAX_RESPONSE_BYTES {
            return Err(McpCallError::unknown(format!(
                "MCP response exceeds {MAX_RESPONSE_BYTES} bytes"
            )));
        }
        payload.extend_from_slice(&available[..content_length]);
        stdout.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    serde_json::from_slice(&payload)
        .map_err(|error| McpCallError::unknown(format!("decode MCP response failed: {error}")))
}

fn mcp_error_text(result: &Value) -> String {
    result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                (item.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| item.get("text").and_then(Value::as_str))
                    .flatten()
            })
        })
        .unwrap_or("MCP tool returned isError=true")
        .to_string()
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
