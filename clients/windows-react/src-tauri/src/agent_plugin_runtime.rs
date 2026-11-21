use chatos_local_agent_protocol::{
    GetRunCapabilityDisclosuresCommand, GetRunCommand, GetRunSkillActivationsCommand, HostCommand,
    HostResult, LocalAgentToolOutcome,
};
use chatos_plugin_management_sdk::SkillGateDeclaration;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;

use crate::{
    host_lifecycle::HostManager,
    machine_resource_governor::{
        MachineResourceGovernor, MachineResourceKind, MachineResourcePriority,
    },
    plugin_application::{
        PluginApplicationManager, ResolvedAgentPluginExecution, TrustedAgentPluginOption,
    },
};

mod broker;
mod journal;
mod mcp;
mod skills;

#[cfg(test)]
use journal::{cleanup_stale_journal_temporaries_before, journal_stage_path};
use journal::{InvocationJournal, JournalBegin};
use mcp::McpSession;
pub(crate) use skills::{load_agent_plugin_skill_snapshot, AgentPluginSkillSnapshot};

pub(crate) const CAPABILITY_SEARCH_TOOL_NAME: &str = "capability_search";
pub(crate) const CAPABILITY_DESCRIBE_TOOL_NAME: &str = "capability_describe";
pub(crate) const AGENT_SKILL_ACTIVATE_TOOL_NAME: &str = "agent_skill_activate";
pub(crate) const AGENT_SKILL_READ_RESOURCE_TOOL_NAME: &str = "agent_skill_read_resource";
pub(crate) const CAPABILITY_INVOKE_TOOL_NAME: &str = "capability_invoke";
pub(crate) const CAPABILITY_BROKER_TOOL_NAMES: [&str; 5] = [
    CAPABILITY_SEARCH_TOOL_NAME,
    CAPABILITY_DESCRIBE_TOOL_NAME,
    AGENT_SKILL_ACTIVATE_TOOL_NAME,
    AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
    CAPABILITY_INVOKE_TOOL_NAME,
];

const MAXIMUM_JOURNAL_BYTES: u64 = 8 * 1_024 * 1_024;
const STALE_JOURNAL_TEMPORARY_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAXIMUM_JOURNAL_TEMPORARY_ENTRIES: usize = 2_000;
const MAXIMUM_SKILL_PAGE_CHARACTERS: usize = 160_000;

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
    resources: MachineResourceGovernor,
}

impl AgentPluginRuntime {
    #[cfg(test)]
    pub(crate) fn new(plugin_root: PathBuf, journal_root: PathBuf) -> Self {
        Self::new_with_resources(
            plugin_root,
            journal_root,
            MachineResourceGovernor::default(),
        )
    }

    pub(crate) fn new_with_resources(
        plugin_root: PathBuf,
        journal_root: PathBuf,
        resources: MachineResourceGovernor,
    ) -> Self {
        Self {
            plugin_root,
            journal: InvocationJournal::new(journal_root),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            resources,
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
        let runtime_fingerprint = resolved_runtime_fingerprint(&resolved)?;
        let key = session_key(context, selection);
        let (cached, stale) = {
            let mut sessions = self.sessions.lock().await;
            match sessions.get(&key) {
                Some(existing) if existing.runtime_fingerprint == runtime_fingerprint => {
                    (Some(existing.clone()), None)
                }
                Some(_) => (None, sessions.remove(&key)),
                None => (None, None),
            }
        };
        if let Some(cached) = cached {
            self.restore_session_state(host, context, selection, &cached)
                .await?;
            return Ok(cached);
        }
        if let Some(stale) = stale {
            stale.invalidate().await;
        }
        if self.sessions.lock().await.len() >= 32 {
            self.prune_terminal_sessions(host).await;
        }
        let resource_permit = if resolved.executable.is_some() {
            Some(
                self.resources
                    .acquire(
                        MachineResourceKind::CodeReadWorker,
                        MachineResourcePriority::Interactive,
                    )
                    .await?,
            )
        } else {
            None
        };
        let session = Arc::new(
            McpSession::connect(
                &self.plugin_root,
                context,
                resolved,
                runtime_fingerprint.clone(),
                resource_permit,
                #[cfg(windows)]
                manager.workspace_leases(),
            )
            .await?,
        );
        let mut sessions = self.sessions.lock().await;
        if let Some(existing) = sessions.get(&key).cloned() {
            if existing.runtime_fingerprint == runtime_fingerprint {
                drop(sessions);
                session.invalidate().await;
                self.restore_session_state(host, context, selection, &existing)
                    .await?;
                return Ok(existing);
            }
        }
        let replaced = sessions.remove(&key);
        if sessions.len() >= 32 {
            drop(sessions);
            session.invalidate().await;
            return Err("Too many Plugin sessions are active.".into());
        }
        sessions.insert(key, session.clone());
        drop(sessions);
        if let Some(replaced) = replaced {
            replaced.invalidate().await;
        }
        self.restore_session_state(host, context, selection, &session)
            .await?;
        Ok(session)
    }

    async fn prune_terminal_sessions(&self, host: &HostManager) {
        let candidates = self
            .sessions
            .lock()
            .await
            .iter()
            .map(|(key, session)| (key.clone(), Arc::clone(session)))
            .collect::<Vec<_>>();
        let mut terminal = Vec::new();
        for (key, session) in candidates {
            let command = HostCommand::GetRun(GetRunCommand {
                owner_user_id: session.owner_user_id.clone(),
                run_id: session.run_id.clone(),
            });
            if matches!(
                host.execute(command).await,
                Ok(HostResult::Run { run }) if run.status.is_terminal()
            ) {
                terminal.push((key, session));
            }
        }
        if terminal.is_empty() {
            return;
        }
        let removed = {
            let mut sessions = self.sessions.lock().await;
            terminal
                .into_iter()
                .filter_map(|(key, expected)| {
                    sessions
                        .get(&key)
                        .is_some_and(|current| Arc::ptr_eq(current, &expected))
                        .then(|| sessions.remove(&key))
                        .flatten()
                })
                .collect::<Vec<_>>()
        };
        for session in removed {
            session.invalidate().await;
        }
    }

    async fn restore_session_state(
        &self,
        host: &HostManager,
        context: &AgentPluginRunContext,
        selection: &TrustedAgentPluginOption,
        session: &McpSession,
    ) -> Result<(), String> {
        let result = host
            .execute(HostCommand::GetRunCapabilityDisclosures(
                GetRunCapabilityDisclosuresCommand {
                    owner_user_id: context.owner_user_id.clone(),
                    run_id: context.run_id.clone(),
                },
            ))
            .await?;
        let HostResult::RunCapabilityDisclosures {
            run_id,
            disclosures,
        } = result
        else {
            return Err(plugin_runtime_error());
        };
        if run_id != context.run_id {
            return Err(plugin_runtime_error());
        }
        let plugin_option = plugin_option(context, selection);
        let provider_id = format!("plugin:{}:{}", selection.plugin_id, selection.component_key);
        for disclosure in disclosures {
            if disclosure.plugin_option == plugin_option && disclosure.provider_id == provider_id {
                session
                    .restore_disclosure(&disclosure.tool_option, &disclosure.schema_sha256)
                    .await;
            }
        }
        let result = host
            .execute(HostCommand::GetRunSkillActivations(
                GetRunSkillActivationsCommand {
                    owner_user_id: context.owner_user_id.clone(),
                    run_id: context.run_id.clone(),
                },
            ))
            .await?;
        let HostResult::RunSkillActivations { run_id, skill_refs } = result else {
            return Err(plugin_runtime_error());
        };
        if run_id != context.run_id {
            return Err(plugin_runtime_error());
        }
        session.restore_skill_activations(&skill_refs).await;
        Ok(())
    }
}

fn frozen_plugin_selections(
    context: &AgentPluginRunContext,
) -> Result<Vec<TrustedAgentPluginOption>, String> {
    let plugins = context
        .input
        .pointer("/team_executor_context/execution_plan/plugins")
        .and_then(Value::as_array)
        .filter(|plugins| plugins.len() <= 32)
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
    #[cfg(windows)]
    const ALLOWED_HOST_ENVIRONMENT: &[&str] = &[
        "ALLUSERSPROFILE",
        "APPDATA",
        "ComSpec",
        "HOME",
        "LANG",
        "LC_ALL",
        "LOCALAPPDATA",
        "PATH",
        "PATHEXT",
        "PROGRAMDATA",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "SystemDrive",
        "SystemRoot",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "WINDIR",
    ];
    #[cfg(not(windows))]
    const ALLOWED_HOST_ENVIRONMENT: &[&str] = &["HOME", "LANG", "LC_ALL", "TEMP", "TMP"];
    for key in ALLOWED_HOST_ENVIRONMENT {
        if let Ok(value) = std::env::var(key) {
            environment.insert((*key).to_owned(), value);
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

fn skill_option(
    context: &AgentPluginRunContext,
    selection: &TrustedAgentPluginOption,
    skill_name: &str,
) -> String {
    opaque_option(
        "skill",
        &context.owner_user_id,
        &context.run_id,
        &format!("{}\0{skill_name}", selection.frozen_identity()),
    )
}

fn project_skill_gate(
    context: &AgentPluginRunContext,
    selection: &TrustedAgentPluginOption,
    gate: &SkillGateDeclaration,
) -> Value {
    let mut projected = Map::from_iter([(
        "all_of".into(),
        Value::Array(
            gate.all_of
                .iter()
                .map(|name| Value::String(skill_option(context, selection, name)))
                .collect(),
        ),
    )]);
    if let Some(selector) = gate.select_by_argument.as_ref() {
        projected.insert(
            "select_by_argument".into(),
            json!({
                "pointer": selector.pointer,
                "map": selector.map.iter().map(|(key, name)| {
                    (key.clone(), skill_option(context, selection, name))
                }).collect::<BTreeMap<_, _>>(),
            }),
        );
    }
    Value::Object(projected)
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

fn resolved_runtime_fingerprint(resolved: &ResolvedAgentPluginExecution) -> Result<String, String> {
    fn update(hasher: &mut Sha256, value: &[u8]) {
        hasher.update(value.len().to_le_bytes());
        hasher.update(value);
    }

    let installation_root = resolved
        .installation_root
        .canonicalize()
        .map_err(|_| plugin_runtime_error())?;
    let executable = resolved
        .executable
        .as_ref()
        .map(|path| path.canonicalize().map_err(|_| plugin_runtime_error()))
        .transpose()?;
    let working_directory = resolved
        .working_directory
        .as_ref()
        .map(|path| path.canonicalize().map_err(|_| plugin_runtime_error()))
        .transpose()?;
    let mut allowed_tools = resolved.allowed_tools.clone();
    if let Some(allowed_tools) = allowed_tools.as_mut() {
        allowed_tools.sort();
    }
    let identity = resolved.identity.frozen_identity();
    let installation_root = installation_root.to_string_lossy().into_owned();
    let executable = executable
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let working_directory = working_directory
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let arguments = serde_json::to_string(&resolved.args).map_err(|_| plugin_runtime_error())?;
    let environment =
        serde_json::to_string(&resolved.environment).map_err(|_| plugin_runtime_error())?;
    let allowed_tools =
        serde_json::to_string(&allowed_tools).map_err(|_| plugin_runtime_error())?;
    let mut hasher = Sha256::new();
    for value in [
        identity.as_bytes(),
        installation_root.as_bytes(),
        executable.as_bytes(),
        working_directory.as_bytes(),
        arguments.as_bytes(),
        environment.as_bytes(),
        resolved.network_policy.as_bytes(),
        allowed_tools.as_bytes(),
    ] {
        update(&mut hasher, value);
    }
    for document in resolved.skill_snapshot.documents.values() {
        for value in [
            document.name.as_bytes(),
            document.description.as_bytes(),
            document.role.as_bytes(),
            document.instructions.as_bytes(),
        ] {
            update(&mut hasher, value);
        }
        for (path, content) in &document.resources {
            update(&mut hasher, path.as_bytes());
            update(&mut hasher, content.as_bytes());
        }
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
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
            "name": AGENT_SKILL_ACTIVATE_TOOL_NAME,
            "description": "激活当前 Run 已披露的精确 Product 或 Plugin skill_ref，并加载核心指令。Skill 不会扩大当前 Todo 的权限。",
            "parameters": {
                "type": "object",
                "properties": {
                    "skill_ref": {"type": "string", "minLength": 1, "maxLength": 512}
                },
                "required": ["skill_ref"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
            "description": "分页读取一个已激活 Product 或 Plugin Skill 的参考资料。只能读取激活结果列出的资源。",
            "parameters": {
                "type": "object",
                "properties": {
                    "skill_ref": {"type": "string", "minLength": 1, "maxLength": 512},
                    "relative_path": {"type": "string", "minLength": 1, "maxLength": 1024},
                    "offset": {"type": "integer", "minimum": 0, "default": 0},
                    "max_chars": {"type": "integer", "minimum": 1, "maximum": 160000, "default": 160000}
                },
                "required": ["skill_ref", "relative_path"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": CAPABILITY_INVOKE_TOOL_NAME,
            "description": "调用已经通过 capability_describe 展开完整 schema 的本机 Plugin 工具。若工具返回 required_skill_refs，必须先逐个激活。",
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
#[path = "agent_plugin_runtime/tests.rs"]
mod tests;
