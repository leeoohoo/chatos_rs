use chatos_local_agent_protocol::LocalAgentToolOutcome;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

use crate::{
    host_lifecycle::HostManager,
    plugin_application::{
        PluginApplicationManager, ResolvedAgentPluginExecution, TrustedAgentPluginOption,
    },
    terminal_runtime::process::ProcessTermination,
};

pub(crate) const CAPABILITY_SEARCH_TOOL_NAME: &str = "capability_search";
pub(crate) const CAPABILITY_DESCRIBE_TOOL_NAME: &str = "capability_describe";
pub(crate) const CAPABILITY_INVOKE_TOOL_NAME: &str = "capability_invoke";
pub(crate) const CAPABILITY_BROKER_TOOL_NAMES: [&str; 3] = [
    CAPABILITY_SEARCH_TOOL_NAME,
    CAPABILITY_DESCRIBE_TOOL_NAME,
    CAPABILITY_INVOKE_TOOL_NAME,
];

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MCP_TIMEOUT: Duration = Duration::from_secs(30);
const PROCESS_REAP_TIMEOUT: Duration = Duration::from_secs(2);
const MAXIMUM_MCP_RESPONSE_BYTES: usize = 4 * 1_024 * 1_024;
const MAXIMUM_MCP_TOOL_PAGES: usize = 100;
const MAXIMUM_MCP_TOOLS: usize = 512;
const MAXIMUM_TOOL_SCHEMA_BYTES: usize = 256 * 1_024;
const MAXIMUM_JOURNAL_BYTES: u64 = 8 * 1_024 * 1_024;

#[derive(Clone)]
pub(crate) struct AgentPluginRunContext {
    pub(crate) owner_user_id: String,
    pub(crate) run_id: String,
    pub(crate) execution_root: PathBuf,
    pub(crate) workspace_scope_id: String,
    pub(crate) input: Value,
}

#[derive(Clone)]
pub(crate) struct AgentPluginRuntime {
    plugin_root: PathBuf,
    journal: InvocationJournal,
    sessions: Arc<Mutex<HashMap<String, Arc<McpSession>>>>,
}

impl AgentPluginRuntime {
    pub(crate) fn new(plugin_root: PathBuf, journal_root: PathBuf) -> Self {
        Self {
            plugin_root,
            journal: InvocationJournal::new(journal_root),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn reset(&self) {
        let sessions = {
            let mut state = self.sessions.lock().await;
            state
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };
        for session in sessions {
            session.invalidate().await;
        }
    }

    pub(crate) async fn execute(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: AgentPluginRunContext,
        invocation_id: &str,
        tool_name: &str,
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        let selections = frozen_plugin_selections(&context)?;
        match tool_name {
            CAPABILITY_SEARCH_TOOL_NAME => {
                self.search(manager, host, &context, &selections, arguments)
                    .await
            }
            CAPABILITY_DESCRIBE_TOOL_NAME => {
                self.describe(manager, host, &context, &selections, arguments)
                    .await
            }
            CAPABILITY_INVOKE_TOOL_NAME => {
                self.invoke(
                    manager,
                    host,
                    &context,
                    &selections,
                    invocation_id,
                    arguments,
                )
                .await
            }
            _ => Err("The requested Plugin capability tool is unavailable.".into()),
        }
    }

    async fn search(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selections: &[TrustedAgentPluginOption],
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        require_exact_keys(arguments, &["query"])?;
        let query = required_string(arguments, "query", 200)?;
        let terms = query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut available = Vec::new();
        for selection in selections {
            if let Ok(resolved) = manager
                .resolve_agent_plugin(host, &context.owner_user_id, selection)
                .await
            {
                available.push(resolved.identity);
            }
        }
        let matches = available
            .iter()
            .filter(|selection| {
                let searchable =
                    format!("{} {}", selection.display_name, selection.description).to_lowercase();
                terms.iter().any(|term| searchable.contains(term))
            })
            .collect::<Vec<_>>();
        let visible = if matches.is_empty() {
            available.iter().collect::<Vec<_>>()
        } else {
            matches
        };
        Ok(LocalAgentToolOutcome::Succeeded {
            output: json!({
                "matches": visible.into_iter().take(12).map(|selection| json!({
                    "plugin_option": plugin_option(context, selection),
                    "name": selection.display_name,
                    "description": selection.description,
                })).collect::<Vec<_>>()
            }),
        })
    }

    async fn describe(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selections: &[TrustedAgentPluginOption],
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        require_exact_keys(
            arguments,
            &["plugin_option", "tool_option", "query", "offset", "limit"],
        )?;
        let requested_plugin = required_string(arguments, "plugin_option", 80)?;
        let selection = selections
            .iter()
            .find(|selection| plugin_option(context, selection) == requested_plugin)
            .ok_or_else(stale_capability_option)?;
        let session = self.session(manager, host, context, selection).await?;
        let tool_option = optional_nullable_string(arguments, "tool_option", 80)?;
        let query = optional_nullable_string(arguments, "query", 200)?;
        if tool_option.is_some() && query.is_some() {
            return Err(invalid_capability_arguments());
        }
        if let Some(tool_option) = tool_option {
            if arguments
                .get("offset")
                .is_some_and(|value| !value.is_null())
                || arguments.get("limit").is_some_and(|value| !value.is_null())
            {
                return Err(invalid_capability_arguments());
            }
            let definition = session
                .definition_by_option(&tool_option)
                .ok_or_else(stale_capability_option)?;
            session.mark_described(&tool_option).await;
            return Ok(LocalAgentToolOutcome::Succeeded {
                output: json!({
                    "view": "tool",
                    "binding_schema_version": 1,
                    "provider_id": format!("plugin:{}:{}", selection.plugin_id, selection.component_key),
                    "plugin_option": requested_plugin,
                    "name": selection.display_name,
                    "tools": [{
                        "tool_option": tool_option,
                        "name": definition.name,
                        "description": definition.description,
                        "input_schema": definition.input_schema,
                        "effect": "write",
                        "required_skill_refs": [],
                        "invocation_mode": "capability_invoke",
                    }],
                    "skills": [],
                }),
            });
        }
        let offset =
            optional_nullable_u64(arguments, "offset", 0, usize::MAX as u64)?.unwrap_or(0) as usize;
        let limit = optional_nullable_u64(arguments, "limit", 1, 20)?.unwrap_or(8) as usize;
        let query = query.unwrap_or_default().to_lowercase();
        let filtered = session
            .definitions
            .iter()
            .filter(|definition| {
                query.is_empty()
                    || format!("{} {}", definition.name, definition.description)
                        .to_lowercase()
                        .contains(&query)
            })
            .collect::<Vec<_>>();
        if offset > filtered.len() {
            return Err(invalid_capability_arguments());
        }
        let page = filtered
            .iter()
            .skip(offset)
            .take(limit)
            .map(|definition| {
                json!({
                    "tool_option": definition.option,
                    "name": definition.name,
                    "description": definition.description,
                    "effect": "write",
                    "required_skill_refs": [],
                    "invocation_mode": "capability_invoke",
                })
            })
            .collect::<Vec<_>>();
        let next_offset = (offset + page.len() < filtered.len()).then_some(offset + page.len());
        Ok(LocalAgentToolOutcome::Succeeded {
            output: json!({
                "view": "index",
                "binding_schema_version": 1,
                "provider_id": format!("plugin:{}:{}", selection.plugin_id, selection.component_key),
                "plugin_option": requested_plugin,
                "name": selection.display_name,
                "tools": page,
                "skills": [],
                "offset": offset,
                "next_offset": next_offset,
                "guidance": "This is an index, not callable tool schemas. Read the chosen tool with capability_describe(plugin_option, tool_option) before invoking it.",
            }),
        })
    }

    async fn invoke(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selections: &[TrustedAgentPluginOption],
        invocation_id: &str,
        arguments: &Map<String, Value>,
    ) -> Result<LocalAgentToolOutcome, String> {
        require_exact_keys(arguments, &["plugin_option", "tool_option", "arguments"])?;
        let requested_plugin = required_string(arguments, "plugin_option", 80)?;
        let requested_tool = required_string(arguments, "tool_option", 80)?;
        let tool_arguments = arguments
            .get("arguments")
            .and_then(Value::as_object)
            .ok_or_else(invalid_capability_arguments)?;
        let selection = selections
            .iter()
            .find(|selection| plugin_option(context, selection) == requested_plugin)
            .ok_or_else(stale_capability_option)?;
        let session = self.session(manager, host, context, selection).await?;
        let definition = session
            .definition_by_option(&requested_tool)
            .ok_or_else(stale_capability_option)?;
        if !session.was_described(&requested_tool).await {
            return Ok(LocalAgentToolOutcome::Failed {
                error: "Call capability_describe with this exact plugin_option and tool_option before invoking the Plugin tool.".into(),
                detail: json!({"code": "capability_describe_required"}),
            });
        }
        let signature = invocation_signature(context, selection, &requested_tool, tool_arguments)?;
        match self
            .journal
            .begin(
                &context.owner_user_id,
                &context.run_id,
                invocation_id,
                &signature,
            )
            .await?
        {
            JournalBegin::Replay(outcome) => return Ok(outcome),
            JournalBegin::Uncertain => {
                return Ok(uncertain_outcome("The previous Plugin invocation has an uncertain outcome and will not be replayed automatically."));
            }
            JournalBegin::Execute => {}
        }
        let outcome = match session
            .call_tool(&definition.name, Value::Object(tool_arguments.clone()))
            .await
        {
            Ok(result) if result.get("isError").and_then(Value::as_bool) == Some(true) => {
                LocalAgentToolOutcome::Failed {
                    error: mcp_error_text(&result),
                    detail: result,
                }
            }
            Ok(result) => LocalAgentToolOutcome::Succeeded { output: result },
            Err(error) if error.outcome_known => LocalAgentToolOutcome::Failed {
                error: error.message,
                detail: json!({"phase": "plugin_mcp_call", "outcome_known": true}),
            },
            Err(_) => {
                self.journal
                    .mark_uncertain(
                        &context.owner_user_id,
                        &context.run_id,
                        invocation_id,
                        &signature,
                    )
                    .await?;
                return Ok(uncertain_outcome("The Plugin invocation may have taken effect, so it will not be replayed automatically."));
            }
        };
        if self
            .journal
            .complete(
                &context.owner_user_id,
                &context.run_id,
                invocation_id,
                &signature,
                outcome.clone(),
            )
            .await
            .is_err()
        {
            return Ok(uncertain_outcome("The Plugin result could not be durably recorded; the invocation will not be replayed automatically."));
        }
        Ok(outcome)
    }

    async fn session(
        &self,
        manager: &PluginApplicationManager,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selection: &TrustedAgentPluginOption,
    ) -> Result<Arc<McpSession>, String> {
        let resolved = manager
            .resolve_agent_plugin(host, &context.owner_user_id, selection)
            .await?;
        let key = session_key(context, selection);
        if let Some(existing) = self.sessions.lock().await.get(&key).cloned() {
            return Ok(existing);
        }
        let session = Arc::new(McpSession::connect(&self.plugin_root, context, resolved).await?);
        let mut sessions = self.sessions.lock().await;
        if let Some(existing) = sessions.get(&key).cloned() {
            drop(sessions);
            session.invalidate().await;
            return Ok(existing);
        }
        if sessions.len() >= 32 {
            drop(sessions);
            session.invalidate().await;
            return Err("Too many Plugin sessions are active.".into());
        }
        sessions.insert(key, session.clone());
        Ok(session)
    }
}

#[derive(Clone, Debug)]
struct ToolDefinition {
    option: String,
    name: String,
    description: String,
    input_schema: Value,
}

struct McpSession {
    definitions: Vec<ToolDefinition>,
    described: Mutex<HashSet<String>>,
    connection: Mutex<McpConnection>,
}

struct McpConnection {
    child: Child,
    _termination: ProcessTermination,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_request_id: u64,
    unusable: bool,
}

impl McpSession {
    async fn connect(
        plugin_root: &Path,
        context: &AgentPluginRunContext,
        resolved: ResolvedAgentPluginExecution,
    ) -> Result<Self, String> {
        let executable = resolved
            .executable
            .canonicalize()
            .map_err(|_| plugin_runtime_error())?;
        let plugin_root = plugin_root
            .canonicalize()
            .map_err(|_| plugin_runtime_error())?;
        if !executable.is_file() || !executable.starts_with(&plugin_root) {
            return Err(plugin_runtime_error());
        }
        let mut command = Command::new(&executable);
        command
            .args(&resolved.args)
            .current_dir(
                resolved
                    .working_directory
                    .as_deref()
                    .unwrap_or(&context.execution_root),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(false)
            .env_clear();
        command.envs(safe_environment(context, &resolved.environment));
        #[cfg(not(windows))]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }
        let mut child = command.spawn().map_err(|_| plugin_runtime_error())?;
        let termination = ProcessTermination::attach(&child).map_err(|_| {
            let _ = child.start_kill();
            plugin_runtime_error()
        })?;
        let stdin = child.stdin.take().ok_or_else(plugin_runtime_error)?;
        let stdout = child.stdout.take().ok_or_else(plugin_runtime_error)?;
        let session = Self {
            definitions: Vec::new(),
            described: Mutex::new(HashSet::new()),
            connection: Mutex::new(McpConnection {
                child,
                _termination: termination,
                stdin,
                stdout: BufReader::new(stdout),
                next_request_id: 1,
                unusable: false,
            }),
        };
        let initialization = session
            .request(
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "chatos-windows-react", "version": env!("CARGO_PKG_VERSION")}
                }),
            )
            .await
            .map_err(|error| error.message)?;
        if initialization
            .get("protocolVersion")
            .and_then(Value::as_str)
            != Some(MCP_PROTOCOL_VERSION)
            || !initialization
                .get("serverInfo")
                .is_some_and(Value::is_object)
        {
            session.invalidate().await;
            return Err(plugin_runtime_error());
        }
        session
            .notify("notifications/initialized", json!({}))
            .await
            .map_err(|error| error.message)?;
        let tools = session.list_tools().await.map_err(|error| error.message)?;
        let definitions = decode_tool_definitions(context, &resolved, tools)?;
        Ok(Self {
            definitions,
            described: session.described,
            connection: session.connection,
        })
    }

    fn definition_by_option(&self, option: &str) -> Option<&ToolDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.option == option)
    }

    async fn mark_described(&self, option: &str) {
        self.described.lock().await.insert(option.to_owned());
    }

    async fn was_described(&self, option: &str) -> bool {
        self.described.lock().await.contains(option)
    }

    async fn list_tools(&self) -> Result<Vec<Value>, McpError> {
        let mut tools = Vec::new();
        let mut cursor = None;
        let mut cursors = HashSet::new();
        for _ in 0..MAXIMUM_MCP_TOOL_PAGES {
            let parameters = cursor
                .as_ref()
                .map(|cursor| json!({"cursor": cursor}))
                .unwrap_or_else(|| json!({}));
            let result = self.request("tools/list", parameters).await?;
            let page = result
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| McpError::known("Plugin tools/list response is invalid."))?;
            if tools.len().saturating_add(page.len()) > MAXIMUM_MCP_TOOLS {
                return Err(McpError::known(
                    "Plugin tool catalog exceeds the safety limit.",
                ));
            }
            tools.extend(page.iter().cloned());
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let Some(next) = cursor.as_ref() else {
                return Ok(tools);
            };
            if !cursors.insert(next.clone()) {
                return Err(McpError::known("Plugin returned a repeated tools cursor."));
            }
        }
        Err(McpError::known("Plugin tool catalog has too many pages."))
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, McpError> {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
            .await
    }

    async fn request(&self, method: &str, parameters: Value) -> Result<Value, McpError> {
        let future = async {
            let mut connection = self.connection.lock().await;
            if connection.unusable {
                return Err(McpError::unknown("Plugin MCP session is unavailable."));
            }
            let request_id = connection.next_request_id;
            connection.next_request_id = connection.next_request_id.saturating_add(1);
            write_message(
                &mut connection.stdin,
                &json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "method": method,
                    "params": parameters,
                }),
            )
            .await?;
            loop {
                let response = read_message(&mut connection.stdout).await?;
                if response.get("id").and_then(Value::as_u64) != Some(request_id) {
                    continue;
                }
                if let Some(error) = response.get("error") {
                    return Err(McpError::known(format!(
                        "Plugin MCP {method} failed: {error}"
                    )));
                }
                return response
                    .get("result")
                    .cloned()
                    .ok_or_else(|| McpError::known("Plugin MCP response has no result."));
            }
        };
        match tokio::time::timeout(MCP_TIMEOUT, future).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) if error.outcome_known => Err(error),
            Ok(Err(error)) => {
                self.invalidate().await;
                Err(error)
            }
            Err(_) => {
                self.invalidate().await;
                Err(McpError::unknown("Plugin MCP request timed out."))
            }
        }
    }

    async fn notify(&self, method: &str, parameters: Value) -> Result<(), McpError> {
        let future = async {
            let mut connection = self.connection.lock().await;
            if connection.unusable {
                return Err(McpError::unknown("Plugin MCP session is unavailable."));
            }
            write_message(
                &mut connection.stdin,
                &json!({"jsonrpc": "2.0", "method": method, "params": parameters}),
            )
            .await
        };
        match tokio::time::timeout(MCP_TIMEOUT, future).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) if error.outcome_known => Err(error),
            Ok(Err(error)) => {
                self.invalidate().await;
                Err(error)
            }
            Err(_) => {
                self.invalidate().await;
                Err(McpError::unknown("Plugin MCP notification timed out."))
            }
        }
    }

    async fn invalidate(&self) {
        let mut connection = self.connection.lock().await;
        if connection.unusable {
            return;
        }
        connection.unusable = true;
        let _ = connection._termination.terminate(true);
        let _ = connection.child.start_kill();
        let _ = tokio::time::timeout(PROCESS_REAP_TIMEOUT, connection.child.wait()).await;
    }
}

#[derive(Debug)]
struct McpError {
    message: String,
    outcome_known: bool,
}

impl McpError {
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

async fn write_message(stdin: &mut ChildStdin, value: &Value) -> Result<(), McpError> {
    let mut payload = serde_json::to_vec(value)
        .map_err(|_| McpError::known("Plugin MCP request could not be encoded."))?;
    payload.push(b'\n');
    stdin
        .write_all(&payload)
        .await
        .map_err(|_| McpError::unknown("Plugin MCP request could not be written."))?;
    stdin
        .flush()
        .await
        .map_err(|_| McpError::unknown("Plugin MCP request could not be flushed."))
}

async fn read_message(stdout: &mut BufReader<ChildStdout>) -> Result<Value, McpError> {
    let mut payload = Vec::new();
    loop {
        let available = stdout
            .fill_buf()
            .await
            .map_err(|_| McpError::unknown("Plugin MCP response could not be read."))?;
        if available.is_empty() {
            return Err(McpError::unknown(
                "Plugin MCP process closed before responding.",
            ));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let content_length = newline.unwrap_or(available.len());
        if payload.len().saturating_add(content_length) > MAXIMUM_MCP_RESPONSE_BYTES {
            return Err(McpError::unknown("Plugin MCP response is too large."));
        }
        payload.extend_from_slice(&available[..content_length]);
        stdout.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    serde_json::from_slice(&payload)
        .map_err(|_| McpError::unknown("Plugin MCP response is not valid JSON."))
}

fn decode_tool_definitions(
    context: &AgentPluginRunContext,
    resolved: &ResolvedAgentPluginExecution,
    tools: Vec<Value>,
) -> Result<Vec<ToolDefinition>, String> {
    let allowed = resolved
        .allowed_tools
        .as_ref()
        .map(|values| values.iter().map(String::as_str).collect::<HashSet<_>>());
    let mut names = HashSet::new();
    let mut definitions = Vec::new();
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| valid_identifier(name))
            .ok_or_else(plugin_runtime_error)?;
        if allowed
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(name))
        {
            continue;
        }
        if !names.insert(name.to_owned()) {
            return Err(plugin_runtime_error());
        }
        let description = tool
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if description.chars().count() > 16_000 || description.contains('\0') {
            return Err(plugin_runtime_error());
        }
        let input_schema = tool
            .get("inputSchema")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
        if !input_schema.is_object()
            || serde_json::to_vec(&input_schema)
                .map_err(|_| plugin_runtime_error())?
                .len()
                > MAXIMUM_TOOL_SCHEMA_BYTES
        {
            return Err(plugin_runtime_error());
        }
        let schema_digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&input_schema).map_err(|_| plugin_runtime_error())?)
        );
        definitions.push(ToolDefinition {
            option: opaque_option(
                "tool",
                &context.owner_user_id,
                &context.run_id,
                &format!(
                    "{}\0{}\0{}",
                    resolved.identity.frozen_identity(),
                    name,
                    schema_digest
                ),
            ),
            name: name.to_owned(),
            description: description.to_owned(),
            input_schema,
        });
    }
    if let Some(allowed) = allowed {
        if allowed.iter().any(|name| !names.contains(*name)) {
            return Err(plugin_runtime_error());
        }
    }
    definitions.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(definitions)
}

fn frozen_plugin_selections(
    context: &AgentPluginRunContext,
) -> Result<Vec<TrustedAgentPluginOption>, String> {
    let plugins = context
        .input
        .pointer("/team_executor_context/execution_plan/plugins")
        .and_then(Value::as_array)
        .filter(|plugins| !plugins.is_empty() && plugins.len() <= 32)
        .ok_or_else(plugin_runtime_error)?;
    let mut identities = HashSet::new();
    plugins
        .iter()
        .map(|plugin| {
            let plugin = plugin.as_object().ok_or_else(plugin_runtime_error)?;
            require_exact_keys(
                plugin,
                &[
                    "installation_id",
                    "plugin_id",
                    "component_key",
                    "component_revision",
                    "release_id",
                    "release_digest",
                    "manifest_version",
                    "display_name",
                    "reason",
                ],
            )?;
            let option = TrustedAgentPluginOption {
                installation_id: required_identifier(plugin, "installation_id", 1_024)?,
                plugin_id: required_identifier(plugin, "plugin_id", 1_024)?,
                component_key: required_identifier(plugin, "component_key", 1_024)?,
                component_revision: required_identifier(plugin, "component_revision", 1_024)?,
                release_id: required_identifier(plugin, "release_id", 1_024)?,
                release_digest: required_release_digest(plugin, "release_digest")?,
                manifest_version: required_string(plugin, "manifest_version", 128)?,
                display_name: required_string(plugin, "display_name", 240)?,
                description: String::new(),
            };
            if !identities.insert(option.selection_id()) {
                return Err(plugin_runtime_error());
            }
            Ok(option)
        })
        .collect()
}

fn safe_environment(
    context: &AgentPluginRunContext,
    manifest_environment: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut environment = BTreeMap::new();
    for key in ["SystemRoot", "WINDIR", "ComSpec", "PATHEXT", "TEMP", "TMP"] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.to_owned(), value);
        }
    }
    #[cfg(not(windows))]
    environment.insert("PATH".into(), "/usr/bin:/bin".into());
    environment.extend(manifest_environment.clone());
    environment.insert(
        "CHATOS_WORKSPACE_ROOT".into(),
        context.execution_root.to_string_lossy().into_owned(),
    );
    environment.insert(
        "CHATOS_WORKSPACE_ID".into(),
        context.workspace_scope_id.clone(),
    );
    environment.insert("CHATOS_RUN_ID".into(), context.run_id.clone());
    environment
}

fn plugin_option(context: &AgentPluginRunContext, selection: &TrustedAgentPluginOption) -> String {
    opaque_option(
        "plugin",
        &context.owner_user_id,
        &context.run_id,
        &selection.frozen_identity(),
    )
}

fn session_key(context: &AgentPluginRunContext, selection: &TrustedAgentPluginOption) -> String {
    opaque_option(
        "session",
        &context.owner_user_id,
        &context.run_id,
        &format!(
            "{}\0{}\0{}",
            selection.frozen_identity(),
            context.workspace_scope_id,
            context.execution_root.to_string_lossy()
        ),
    )
}

fn opaque_option(prefix: &str, owner: &str, run_id: &str, identity: &str) -> String {
    let digest = Sha256::digest(
        format!("chatos-agent-plugin-option-v1\n{owner}\n{run_id}\n{identity}").as_bytes(),
    );
    format!("{prefix}_{digest:x}")
}

fn invocation_signature(
    context: &AgentPluginRunContext,
    selection: &TrustedAgentPluginOption,
    tool_option: &str,
    arguments: &Map<String, Value>,
) -> Result<String, String> {
    let encoded = serde_json::to_vec(arguments).map_err(|_| plugin_runtime_error())?;
    let mut hasher = Sha256::new();
    for value in [
        context.owner_user_id.as_bytes(),
        context.run_id.as_bytes(),
        selection.frozen_identity().as_bytes(),
        tool_option.as_bytes(),
        encoded.as_slice(),
    ] {
        hasher.update(value.len().to_le_bytes());
        hasher.update(value);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

#[derive(Clone)]
struct InvocationJournal {
    root: PathBuf,
}

enum JournalBegin {
    Execute,
    Replay(LocalAgentToolOutcome),
    Uncertain,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct JournalRecord {
    signature: String,
    status: JournalStatus,
    outcome: Option<LocalAgentToolOutcome>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum JournalStatus {
    Running,
    Completed,
    Uncertain,
}

impl InvocationJournal {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }

    async fn begin(
        &self,
        owner: &str,
        run_id: &str,
        invocation_id: &str,
        signature: &str,
    ) -> Result<JournalBegin, String> {
        let base = self.path(owner, run_id, invocation_id).await?;
        if let Some(record) = read_journal_record(&journal_stage_path(&base, "result")).await? {
            if record.signature != signature {
                return Err(
                    "Plugin invocation identity was reused with different arguments.".into(),
                );
            }
            return Ok(match (record.status, record.outcome) {
                (JournalStatus::Completed, Some(outcome)) => JournalBegin::Replay(outcome),
                _ => JournalBegin::Uncertain,
            });
        }
        for stage in ["uncertain", "intent"] {
            if let Some(record) = read_journal_record(&journal_stage_path(&base, stage)).await? {
                if record.signature != signature {
                    return Err(
                        "Plugin invocation identity was reused with different arguments.".into(),
                    );
                }
                return Ok(JournalBegin::Uncertain);
            }
        }
        write_journal_record(
            &journal_stage_path(&base, "intent"),
            &JournalRecord {
                signature: signature.to_owned(),
                status: JournalStatus::Running,
                outcome: None,
            },
        )
        .await?;
        Ok(JournalBegin::Execute)
    }

    async fn complete(
        &self,
        owner: &str,
        run_id: &str,
        invocation_id: &str,
        signature: &str,
        outcome: LocalAgentToolOutcome,
    ) -> Result<(), String> {
        let base = self.path(owner, run_id, invocation_id).await?;
        write_journal_record(
            &journal_stage_path(&base, "result"),
            &JournalRecord {
                signature: signature.to_owned(),
                status: JournalStatus::Completed,
                outcome: Some(outcome),
            },
        )
        .await
    }

    async fn mark_uncertain(
        &self,
        owner: &str,
        run_id: &str,
        invocation_id: &str,
        signature: &str,
    ) -> Result<(), String> {
        let base = self.path(owner, run_id, invocation_id).await?;
        write_journal_record(
            &journal_stage_path(&base, "uncertain"),
            &JournalRecord {
                signature: signature.to_owned(),
                status: JournalStatus::Uncertain,
                outcome: None,
            },
        )
        .await
    }

    async fn path(
        &self,
        owner: &str,
        run_id: &str,
        invocation_id: &str,
    ) -> Result<PathBuf, String> {
        ensure_private_journal_root(&self.root).await?;
        let digest = Sha256::digest(format!("{owner}\0{run_id}\0{invocation_id}").as_bytes());
        Ok(self.root.join(format!("{digest:x}")))
    }
}

async fn ensure_private_journal_root(root: &Path) -> Result<(), String> {
    match tokio::fs::symlink_metadata(root).await {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err(journal_error()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::create_dir_all(root)
                .await
                .map_err(|_| journal_error())?;
        }
        Err(_) => return Err(journal_error()),
    }
    let metadata = tokio::fs::symlink_metadata(root)
        .await
        .map_err(|_| journal_error())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(journal_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|_| journal_error())?;
    }
    Ok(())
}

fn journal_stage_path(base: &Path, stage: &str) -> PathBuf {
    base.with_extension(format!("{stage}.json"))
}

async fn read_journal_record(path: &Path) -> Result<Option<JournalRecord>, String> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(journal_error()),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAXIMUM_JOURNAL_BYTES
    {
        return Err(journal_error());
    }
    let bytes = tokio::fs::read(path).await.map_err(|_| journal_error())?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| journal_error())
}

async fn write_journal_record(path: &Path, record: &JournalRecord) -> Result<(), String> {
    let bytes = serde_json::to_vec(record).map_err(|_| journal_error())?;
    if bytes.len() as u64 > MAXIMUM_JOURNAL_BYTES {
        return Err(journal_error());
    }
    let parent = path.parent().ok_or_else(journal_error)?;
    ensure_private_journal_root(parent).await?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let mut options = tokio::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .await
        .map_err(|_| journal_error())?;
    if file.write_all(&bytes).await.is_err() || file.sync_all().await.is_err() {
        drop(file);
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(journal_error());
    }
    drop(file);
    match tokio::fs::hard_link(&temporary, path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_journal_record(path).await?.as_ref() != Some(record) {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(journal_error());
            }
        }
        Err(_) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(journal_error());
        }
    }
    tokio::fs::remove_file(&temporary)
        .await
        .map_err(|_| journal_error())?;
    Ok(())
}

fn require_exact_keys(arguments: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    if arguments.keys().any(|key| !allowed.contains(&key.as_str())) {
        Err(invalid_capability_arguments())
    } else {
        Ok(())
    }
}

fn required_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<String, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.trim() == *value
                && value.chars().count() <= maximum
                && !value.chars().any(char::is_control)
        })
        .map(str::to_owned)
        .ok_or_else(invalid_capability_arguments)
}

fn required_identifier(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<String, String> {
    let value = required_string(arguments, field, maximum)?;
    valid_identifier(&value)
        .then_some(value)
        .ok_or_else(plugin_runtime_error)
}

fn required_release_digest(arguments: &Map<String, Value>, field: &str) -> Result<String, String> {
    let value = required_string(arguments, field, 512)?;
    let digest = value.strip_prefix("sha256:").unwrap_or(&value);
    (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(value)
        .ok_or_else(plugin_runtime_error)
}

fn optional_nullable_string(
    arguments: &Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<Option<String>, String> {
    match arguments.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => required_string(arguments, field, maximum).map(Some),
    }
}

fn optional_nullable_u64(
    arguments: &Map<String, Value>,
    field: &str,
    minimum: u64,
    maximum: u64,
) -> Result<Option<u64>, String> {
    match arguments.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|value| (minimum..=maximum).contains(value))
            .map(Some)
            .ok_or_else(invalid_capability_arguments),
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1_024
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:|".contains(&byte))
}

fn uncertain_outcome(message: &str) -> LocalAgentToolOutcome {
    LocalAgentToolOutcome::Failed {
        error: message.into(),
        detail: json!({"code": "plugin_outcome_uncertain", "retry_automatically": false}),
    }
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
        .unwrap_or("Plugin MCP tool returned isError=true.")
        .chars()
        .take(8_000)
        .collect()
}

fn invalid_capability_arguments() -> String {
    "The Plugin capability arguments are invalid.".into()
}

fn stale_capability_option() -> String {
    "The Plugin capability option is unavailable or expired.".into()
}

fn plugin_runtime_error() -> String {
    "The selected Plugin no longer matches the frozen Todo execution plan.".into()
}

fn journal_error() -> String {
    "The Plugin invocation journal is unavailable.".into()
}

pub(crate) fn capability_broker_tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": CAPABILITY_SEARCH_TOOL_NAME,
            "description": "按任务关键词搜索当前 Todo 已冻结的本机 Plugin 能力。只返回本轮临时选项和简介，不启动未选择的 Plugin。",
            "parameters": {
                "type": "object",
                "properties": {"query": {"type": "string", "minLength": 1, "maxLength": 200}},
                "required": ["query"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": CAPABILITY_DESCRIBE_TOOL_NAME,
            "description": "分页查看一个 Plugin 的工具索引；选中工具后必须用精确 plugin_option 与 tool_option 再读取完整 schema，之后才能调用。",
            "parameters": {
                "type": "object",
                "properties": {
                    "plugin_option": {"type": "string", "minLength": 1, "maxLength": 80},
                    "tool_option": {"type": ["string", "null"], "minLength": 1, "maxLength": 80},
                    "query": {"type": ["string", "null"], "minLength": 1, "maxLength": 200},
                    "offset": {"type": ["integer", "null"], "minimum": 0},
                    "limit": {"type": ["integer", "null"], "minimum": 1, "maximum": 20}
                },
                "required": ["plugin_option"],
                "additionalProperties": false
            },
            "strict": false
        }),
        json!({
            "type": "function",
            "name": CAPABILITY_INVOKE_TOOL_NAME,
            "description": "调用已经通过 capability_describe 展开完整 schema 的本机 Plugin 工具。只能使用当前 Todo 冻结计划中的本轮临时选项。",
            "parameters": {
                "type": "object",
                "properties": {
                    "plugin_option": {"type": "string", "minLength": 1, "maxLength": 80},
                    "tool_option": {"type": "string", "minLength": 1, "maxLength": 80},
                    "arguments": {"type": "object"}
                },
                "required": ["plugin_option", "tool_option", "arguments"],
                "additionalProperties": false
            }
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> AgentPluginRunContext {
        AgentPluginRunContext {
            owner_user_id: "owner-1".into(),
            run_id: "run-1".into(),
            execution_root: PathBuf::from("/project"),
            workspace_scope_id: "workspace-1".into(),
            input: json!({
                "team_executor_context": {
                    "execution_plan": {
                        "plugins": [{
                            "installation_id": "installation-1",
                            "plugin_id": "document-plugin",
                            "component_key": "document-mcp",
                            "component_revision": "revision-1",
                            "release_id": "release-1",
                            "release_digest": format!("sha256:{}", "a".repeat(64)),
                            "manifest_version": "1.2.3",
                            "display_name": "Document Tools",
                            "reason": "Create a document"
                        }]
                    }
                }
            }),
        }
    }

    #[test]
    fn frozen_plan_parser_rejects_unknown_fields_and_opaque_options_are_run_scoped() {
        let first = context();
        let selection = frozen_plugin_selections(&first).unwrap().remove(0);
        let first_option = plugin_option(&first, &selection);
        let mut second = first.clone();
        second.run_id = "run-2".into();
        assert_ne!(first_option, plugin_option(&second, &selection));
        assert!(!first_option.contains(&selection.plugin_id));

        let mut forged = first;
        forged.input["team_executor_context"]["execution_plan"]["plugins"][0]["path"] =
            json!("C:\\private\\plugin.exe");
        assert!(frozen_plugin_selections(&forged).is_err());
    }

    #[tokio::test]
    async fn journal_replays_completed_results_and_never_replays_running_intents() {
        let directory = tempfile::tempdir().unwrap();
        let journal = InvocationJournal::new(directory.path().join("journal"));
        assert!(matches!(
            journal
                .begin("owner-1", "run-1", "call-1", "sig-1")
                .await
                .unwrap(),
            JournalBegin::Execute
        ));
        assert!(matches!(
            journal
                .begin("owner-1", "run-1", "call-1", "sig-1")
                .await
                .unwrap(),
            JournalBegin::Uncertain
        ));
        let outcome = LocalAgentToolOutcome::Succeeded {
            output: json!({"ok": true}),
        };
        journal
            .complete("owner-1", "run-1", "call-1", "sig-1", outcome.clone())
            .await
            .unwrap();
        match journal
            .begin("owner-1", "run-1", "call-1", "sig-1")
            .await
            .unwrap()
        {
            JournalBegin::Replay(replayed) => assert_eq!(replayed, outcome),
            _ => panic!("expected durable replay"),
        }
        assert!(journal
            .begin("owner-1", "run-1", "call-1", "changed")
            .await
            .is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let root_mode = std::fs::metadata(&journal.root)
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(root_mode, 0o700);
            let base = journal.path("owner-1", "run-1", "call-1").await.unwrap();
            let result_mode = std::fs::metadata(journal_stage_path(&base, "result"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(result_mode, 0o600);
        }
    }

    #[tokio::test]
    async fn journal_stage_records_are_append_only() {
        let directory = tempfile::tempdir().unwrap();
        let journal = InvocationJournal::new(directory.path().join("journal"));
        journal
            .begin("owner-1", "run-1", "call-1", "sig-1")
            .await
            .unwrap();
        journal
            .complete(
                "owner-1",
                "run-1",
                "call-1",
                "sig-1",
                LocalAgentToolOutcome::Succeeded {
                    output: json!({"value": 1}),
                },
            )
            .await
            .unwrap();
        assert!(journal
            .complete(
                "owner-1",
                "run-1",
                "call-1",
                "sig-1",
                LocalAgentToolOutcome::Succeeded {
                    output: json!({"value": 2}),
                },
            )
            .await
            .is_err());
        match journal
            .begin("owner-1", "run-1", "call-1", "sig-1")
            .await
            .unwrap()
        {
            JournalBegin::Replay(LocalAgentToolOutcome::Succeeded { output }) => {
                assert_eq!(output, json!({"value": 1}));
            }
            _ => panic!("expected the original append-only result"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn journal_rejects_a_symlinked_root() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let actual = directory.path().join("actual");
        let linked = directory.path().join("journal");
        std::fs::create_dir(&actual).unwrap();
        symlink(&actual, &linked).unwrap();
        let journal = InvocationJournal::new(linked);
        assert!(journal
            .begin("owner-1", "run-1", "call-1", "sig-1")
            .await
            .is_err());
    }

    #[test]
    fn broker_catalog_keeps_invoke_out_of_the_read_only_surface() {
        let schemas = capability_broker_tool_schemas();
        assert_eq!(schemas.len(), 3);
        assert_eq!(schemas[0]["name"], CAPABILITY_SEARCH_TOOL_NAME);
        assert_eq!(schemas[1]["name"], CAPABILITY_DESCRIBE_TOOL_NAME);
        assert_eq!(schemas[2]["name"], CAPABILITY_INVOKE_TOOL_NAME);
    }

    #[tokio::test]
    async fn stdio_session_initializes_lists_and_calls_a_real_plugin_process() {
        let directory = tempfile::tempdir().unwrap();
        let plugin_root = directory.path().join("Plugins");
        let installation = plugin_root.join("plugin-1").join("1.0.0");
        let project_root = directory.path().join("project");
        std::fs::create_dir_all(&installation).unwrap();
        std::fs::create_dir_all(&project_root).unwrap();
        let executable = installation.join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        let fixture_source = installation.join("fixture.rs");
        std::fs::write(
            &fixture_source,
            r###"
use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let Some(id_tail) = line.split("\"id\":").nth(1) else { continue };
        let id = id_tail.chars().take_while(|value| value.is_ascii_digit()).collect::<String>();
        let response = if line.contains("\"method\":\"initialize\"") {
            r#"{"jsonrpc":"2.0","id":0,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}}}"#
        } else if line.contains("\"method\":\"tools/list\"") {
            r#"{"jsonrpc":"2.0","id":0,"result":{"tools":[{"name":"echo_document","description":"Echo a bounded document request.","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false}}]}}"#
        } else if line.contains("\"method\":\"tools/call\"") {
            r#"{"jsonrpc":"2.0","id":0,"result":{"content":[{"type":"text","text":"verified"}],"isError":false}}"#
        } else {
            continue;
        };
        writeln!(stdout, "{}", response.replacen("\"id\":0", &format!("\"id\":{id}"), 1)).unwrap();
        stdout.flush().unwrap();
    }
}
"###,
        )
        .unwrap();
        let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let compiled = std::process::Command::new(compiler)
            .arg(&fixture_source)
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap();
        assert!(compiled.success());
        let context = AgentPluginRunContext {
            execution_root: project_root,
            ..context()
        };
        let resolved = ResolvedAgentPluginExecution {
            identity: frozen_plugin_selections(&context).unwrap().remove(0),
            executable,
            args: Vec::new(),
            working_directory: Some(installation),
            environment: BTreeMap::new(),
            allowed_tools: Some(vec!["echo_document".into()]),
        };

        let session = McpSession::connect(&plugin_root, &context, resolved)
            .await
            .unwrap();

        assert_eq!(session.definitions.len(), 1);
        assert_eq!(session.definitions[0].name, "echo_document");
        let result = session
            .call_tool("echo_document", json!({"text": "verified"}))
            .await
            .unwrap();
        assert_eq!(result["content"][0]["text"], "verified");
        session.invalidate().await;
    }
}
