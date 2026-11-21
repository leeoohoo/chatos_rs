// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::skills::{
    normalized_skill_relative_path, path_to_slashes, AgentPluginSkillDocument, SkillResourcePage,
};
use super::{
    invalid_capability_arguments, plugin_runtime_error, safe_environment, skill_option,
    AgentPluginRunContext,
};
use serde_json::{json, Value};
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(not(windows))]
use std::process::Stdio;
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};
#[cfg(not(windows))]
use tokio::process::Command;
use tokio::{io::BufReader, sync::Mutex};

#[cfg(not(windows))]
use crate::terminal_runtime::process::ProcessTermination;
#[cfg(windows)]
use crate::windows_appcontainer::{WorkspaceAccess, WorkspaceLeaseManager};
use crate::{
    machine_resource_governor::MachineResourcePermit,
    plugin_application::ResolvedAgentPluginExecution,
};

mod catalog;
mod transport;

use catalog::{decode_tool_definitions, ToolDefinition};
pub(in crate::agent_plugin_runtime) use transport::McpConnection;

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

pub(super) struct McpSession {
    pub(super) owner_user_id: String,
    pub(super) run_id: String,
    pub(super) runtime_fingerprint: String,
    pub(super) definitions: Vec<ToolDefinition>,
    pub(super) skills_by_option: BTreeMap<String, AgentPluginSkillDocument>,
    described: Mutex<HashSet<String>>,
    disclosed_skills: Mutex<HashSet<String>>,
    pub(super) activated_skills: Mutex<HashSet<String>>,
    pub(super) connection: Option<Mutex<McpConnection>>,
}

impl McpSession {
    pub(super) async fn connect(
        plugin_root: &Path,
        context: &AgentPluginRunContext,
        resolved: ResolvedAgentPluginExecution,
        runtime_fingerprint: String,
        resource_permit: Option<MachineResourcePermit>,
        #[cfg(windows)] workspace_leases: &WorkspaceLeaseManager,
    ) -> Result<Self, String> {
        let skills_by_option = resolved
            .skill_snapshot
            .documents
            .values()
            .map(|document| {
                (
                    skill_option(context, &resolved.identity, &document.name),
                    document.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let Some(executable) = resolved.executable.as_ref() else {
            if skills_by_option.is_empty()
                || !resolved.args.is_empty()
                || !resolved.environment.is_empty()
                || resolved.allowed_tools.is_some()
            {
                return Err(plugin_runtime_error());
            }
            return Ok(Self {
                owner_user_id: context.owner_user_id.clone(),
                run_id: context.run_id.clone(),
                runtime_fingerprint,
                definitions: Vec::new(),
                skills_by_option,
                described: Mutex::new(HashSet::new()),
                disclosed_skills: Mutex::new(HashSet::new()),
                activated_skills: Mutex::new(HashSet::new()),
                connection: None,
            });
        };
        let executable = executable
            .canonicalize()
            .map_err(|_| plugin_runtime_error())?;
        let plugin_root = plugin_root
            .canonicalize()
            .map_err(|_| plugin_runtime_error())?;
        if !executable.is_file() || !executable.starts_with(&plugin_root) {
            return Err(plugin_runtime_error());
        }
        #[cfg(windows)]
        let connection = {
            let installation_root = resolved
                .installation_root
                .canonicalize()
                .map_err(|_| plugin_runtime_error())?;
            if !installation_root.is_dir()
                || !installation_root.starts_with(&plugin_root)
                || !executable.starts_with(&installation_root)
            {
                return Err(plugin_runtime_error());
            }
            let current_directory = resolved
                .working_directory
                .as_deref()
                .unwrap_or(&context.execution_root);
            let environment = safe_environment(context, &resolved.environment)
                .into_iter()
                .map(|(key, value)| (OsString::from(key), OsString::from(value)))
                .collect::<Vec<_>>();
            let mut child = workspace_leases.spawn_workspace_command_with_leases(
                current_directory,
                &executable,
                &resolved.args,
                &[
                    (installation_root, WorkspaceAccess::ReadOnly),
                    (context.execution_root.clone(), WorkspaceAccess::ReadWrite),
                ],
                resolved.network_policy,
                &environment,
            )?;
            let stdin = child.take_stdin().ok_or_else(plugin_runtime_error)?;
            let stdout = child.take_stdout().ok_or_else(plugin_runtime_error)?;
            let stderr = child.take_stderr().ok_or_else(plugin_runtime_error)?;
            let stderr_task = tokio::spawn(async move {
                let mut stderr = tokio::fs::File::from_std(stderr);
                let mut sink = tokio::io::sink();
                let _ = tokio::io::copy(&mut stderr, &mut sink).await;
            });
            McpConnection {
                child: Some(child),
                stdin: tokio::fs::File::from_std(stdin),
                stdout: BufReader::new(tokio::fs::File::from_std(stdout)),
                stderr_task,
                resource_permit,
                next_request_id: 1,
                unusable: false,
            }
        };
        #[cfg(not(windows))]
        let connection = {
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
            McpConnection {
                child,
                _termination: termination,
                stdin,
                stdout: BufReader::new(stdout),
                resource_permit,
                next_request_id: 1,
                unusable: false,
            }
        };
        let session = Self {
            owner_user_id: context.owner_user_id.clone(),
            run_id: context.run_id.clone(),
            runtime_fingerprint: runtime_fingerprint.clone(),
            definitions: Vec::new(),
            skills_by_option: BTreeMap::new(),
            described: Mutex::new(HashSet::new()),
            disclosed_skills: Mutex::new(HashSet::new()),
            activated_skills: Mutex::new(HashSet::new()),
            connection: Some(Mutex::new(connection)),
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
            owner_user_id: session.owner_user_id,
            run_id: session.run_id,
            runtime_fingerprint,
            definitions,
            skills_by_option,
            described: session.described,
            disclosed_skills: session.disclosed_skills,
            activated_skills: session.activated_skills,
            connection: session.connection,
        })
    }

    pub(super) fn definition_by_option(&self, option: &str) -> Option<&ToolDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.option == option)
    }

    pub(super) async fn mark_described(&self, option: &str) {
        self.described.lock().await.insert(option.to_owned());
    }

    pub(super) async fn was_described(&self, option: &str) -> bool {
        self.described.lock().await.contains(option)
    }

    pub(super) async fn restore_disclosure(&self, option: &str, schema_sha256: &str) {
        let Some(definition) = self.definition_by_option(option) else {
            return;
        };
        if definition.schema_sha256 != schema_sha256 {
            return;
        }
        let required_skill_refs = definition.required_skill_refs.clone();
        self.described.lock().await.insert(option.to_owned());
        self.mark_disclosed_skills(&required_skill_refs).await;
    }

    pub(super) async fn restore_skill_activations(&self, options: &[String]) {
        let valid = options
            .iter()
            .filter(|option| self.skills_by_option.contains_key(option.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        self.activated_skills.lock().await.extend(valid);
    }

    pub(super) async fn mark_disclosed_skills(&self, options: &[String]) {
        self.disclosed_skills
            .lock()
            .await
            .extend(options.iter().cloned());
    }

    pub(super) fn skill_summaries(&self, options: &[String]) -> Vec<Value> {
        let mut options = options.iter().collect::<Vec<_>>();
        options.sort();
        options.dedup();
        options
            .into_iter()
            .filter_map(|option| {
                self.skills_by_option.get(option).map(|document| {
                    json!({
                        "skill_ref": option,
                        "name": document.name,
                        "role": document.role,
                        "description": document.description,
                    })
                })
            })
            .collect()
    }

    pub(super) async fn describe_skill_catalog(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Value>, Option<usize>), String> {
        let filtered = self
            .skills_by_option
            .iter()
            .filter(|(_, document)| {
                query.is_empty()
                    || format!("{} {}", document.name, document.description)
                        .to_lowercase()
                        .contains(query)
            })
            .map(|(option, _)| option.clone())
            .collect::<Vec<_>>();
        if offset > filtered.len() || limit == 0 || limit > 20 {
            return Err(invalid_capability_arguments());
        }
        let page = filtered
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        self.mark_disclosed_skills(&page).await;
        let next_offset = (offset + page.len() < filtered.len()).then_some(offset + page.len());
        Ok((self.skill_summaries(&page), next_offset))
    }

    pub(super) async fn activate_skill(
        &self,
        option: &str,
    ) -> Result<Option<AgentPluginSkillDocument>, String> {
        let Some(document) = self.skills_by_option.get(option).cloned() else {
            return Ok(None);
        };
        let already_activated = self.activated_skills.lock().await.contains(option);
        if !already_activated && !self.disclosed_skills.lock().await.contains(option) {
            return Err("Call capability_describe and activate only a returned skill_ref.".into());
        }
        self.activated_skills.lock().await.insert(option.to_owned());
        Ok(Some(document))
    }

    pub(super) async fn read_skill_resource(
        &self,
        option: &str,
        relative_path: &str,
        offset: usize,
        maximum_characters: usize,
    ) -> Result<Option<SkillResourcePage>, String> {
        let Some(document) = self.skills_by_option.get(option) else {
            return Ok(None);
        };
        if !self.activated_skills.lock().await.contains(option) {
            return Err("Activate the Plugin Skill before reading its resources.".into());
        }
        let path = path_to_slashes(&normalized_skill_relative_path(relative_path)?)?;
        let content = document
            .resources
            .get(&path)
            .ok_or_else(plugin_runtime_error)?;
        let scalars = content.chars().collect::<Vec<_>>();
        if offset > scalars.len() || maximum_characters == 0 {
            return Err(invalid_capability_arguments());
        }
        let end = offset.saturating_add(maximum_characters).min(scalars.len());
        Ok(Some(SkillResourcePage {
            content: scalars[offset..end].iter().collect(),
            offset,
            next_offset: (end < scalars.len()).then_some(end),
            truncated: end < scalars.len(),
        }))
    }

    pub(super) async fn missing_skill_refs(
        &self,
        definition: &ToolDefinition,
        arguments: &Value,
    ) -> Result<Vec<String>, String> {
        let Some(gate) = definition.skill_gate.as_ref() else {
            return Ok(Vec::new());
        };
        let required_names = gate
            .required_skill_names(arguments)
            .map_err(|_| invalid_capability_arguments())?;
        let activated = self.activated_skills.lock().await;
        let missing = required_names
            .into_iter()
            .filter_map(|name| {
                self.skills_by_option.iter().find_map(|(option, document)| {
                    (document.name == name && !activated.contains(option)).then(|| option.clone())
                })
            })
            .collect();
        Ok(missing)
    }
}
