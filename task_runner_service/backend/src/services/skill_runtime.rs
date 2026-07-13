// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use async_trait::async_trait;
use chatos_mcp_runtime::{
    BuiltinToolProvider, McpBuiltinServer, ToolCallContext, ToolStreamChunkCallback,
};
use chatos_plugin_management_sdk::ResolvedSkill;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Value};
use tracing::warn;

use crate::models::{TaskRecord, TaskRunRecord};

use super::{RunService, TaskRunnerCapabilityPolicy};

#[derive(Default)]
pub(super) struct PreparedLocalSkills {
    pub(super) input_items: Vec<Value>,
    pub(super) builtin_servers: Vec<McpBuiltinServer>,
    pub(super) builtin_providers: Vec<LocalSkillBuiltinProvider>,
    pub(super) session_handles: Vec<LocalSkillSessionHandle>,
}

pub(super) async fn prepare_local_skills(
    service: &RunService,
    task: &TaskRecord,
    run: &TaskRunRecord,
    capability_policy: Option<&TaskRunnerCapabilityPolicy>,
) -> Result<PreparedLocalSkills, String> {
    let Some(policy) = capability_policy else {
        if task.mcp_config.selected_skill_ids.is_empty() {
            return Ok(PreparedLocalSkills::default());
        }
        return Err("Plugin Management policy is required to prepare selected Skills".to_string());
    };
    let skills = policy.effective_skills(task)?;
    if skills.is_empty() {
        return Ok(PreparedLocalSkills::default());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|err| format!("build Local Connector Skill client failed: {err}"))?;
    let mut prepared = Vec::with_capacity(skills.len());
    let mut session_handles = Vec::with_capacity(skills.len());
    for skill in skills {
        match prepare_skill(service, &client, task, run, skill).await {
            Ok(item) => {
                session_handles.push(item.session_handle(client.clone()));
                prepared.push(item);
            }
            Err(err) => {
                cleanup_local_skill_sessions(session_handles.as_slice()).await;
                return Err(err);
            }
        }
    }
    let text = prepared
        .iter()
        .map(|item| {
            let tool_note = if item.tools.is_empty() {
                String::new()
            } else {
                format!(
                    "\n\nLocal tools for this Skill are published through the `{}` MCP server and execute on the same Local Connector device.",
                    item.server_name
                )
            };
            format!(
                "## {} ({})\n\n{}{}",
                item.display_name, item.skill_id, item.instructions, tool_note
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    let input_items = vec![json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": format!(
                "[Local Connector Skills]\nThe user explicitly enabled and selected the following signed local Skills for this run. Their instructions and tool descriptors were prepared by the active Local Connector from the exact bundle snapshots recorded for this run. Follow them when relevant. Every published Skill tool executes on that Local Connector; never replace it with server-side shell, file, browser, or desktop execution. Do not assume any unlisted local Skill is available.\n\n{text}"
            )
        }]
    })];
    let mut builtin_servers = Vec::new();
    let mut builtin_providers = Vec::new();
    for (item, session_handle) in prepared.iter().zip(session_handles.iter()) {
        if item.tools.is_empty() {
            continue;
        }
        builtin_servers.push(item.builtin_server());
        builtin_providers.push(item.builtin_provider(session_handle.clone()));
    }
    Ok(PreparedLocalSkills {
        input_items,
        builtin_servers,
        builtin_providers,
        session_handles,
    })
}

struct PreparedSkill {
    skill_id: String,
    display_name: String,
    instructions: String,
    server_name: String,
    tools: Vec<Value>,
    permissions: Vec<String>,
    owner_user_id: String,
    device_id: String,
    workspace_id: String,
    task_id: String,
    run_id: String,
    bundle_id: String,
    version: String,
    bundle_hash: String,
    adapter_session_id: String,
    base_url: String,
    internal_secret: String,
}

impl PreparedSkill {
    fn builtin_server(&self) -> McpBuiltinServer {
        McpBuiltinServer {
            name: self.server_name.clone(),
            kind: "local_connector_skill".to_string(),
            workspace_dir: if self.workspace_id.is_empty() {
                String::new()
            } else {
                format!("local://connector/{}/{}", self.device_id, self.workspace_id)
            },
            user_id: Some(self.owner_user_id.clone()),
            project_id: Some(self.task_id.clone()),
            remote_connection_id: None,
            contact_agent_id: None,
            auto_create_task: false,
            allow_writes: self
                .permissions
                .iter()
                .any(|permission| permission == "workspace.write"),
            max_file_bytes: 2 * 1024 * 1024,
            max_write_bytes: 2 * 1024 * 1024,
            search_limit: 0,
        }
    }

    fn session_handle(&self, client: reqwest::Client) -> LocalSkillSessionHandle {
        LocalSkillSessionHandle {
            client,
            base_url: self.base_url.clone(),
            internal_secret: self.internal_secret.clone(),
            owner_user_id: self.owner_user_id.clone(),
            device_id: self.device_id.clone(),
            workspace_id: self.workspace_id.clone(),
            task_id: self.task_id.clone(),
            run_id: self.run_id.clone(),
            skill_id: self.skill_id.clone(),
            bundle_id: self.bundle_id.clone(),
            version: self.version.clone(),
            bundle_hash: self.bundle_hash.clone(),
            adapter_session_id: self.adapter_session_id.clone(),
        }
    }

    fn builtin_provider(&self, session: LocalSkillSessionHandle) -> LocalSkillBuiltinProvider {
        LocalSkillBuiltinProvider {
            server_name: self.server_name.clone(),
            tools: self.tools.clone(),
            session,
        }
    }
}

#[derive(Clone)]
pub(super) struct LocalSkillSessionHandle {
    client: reqwest::Client,
    base_url: String,
    internal_secret: String,
    owner_user_id: String,
    device_id: String,
    workspace_id: String,
    task_id: String,
    run_id: String,
    skill_id: String,
    bundle_id: String,
    version: String,
    bundle_hash: String,
    adapter_session_id: String,
}

impl LocalSkillSessionHandle {
    async fn cancel(&self) -> Result<bool, String> {
        let response = self
            .client
            .post(skill_relay_url(
                self.base_url.as_str(),
                self.device_id.as_str(),
                self.workspace_id.as_str(),
                "cancel",
            ))
            .headers(internal_skill_headers(
                self.internal_secret.as_str(),
                self.owner_user_id.as_str(),
            )?)
            .json(&json!({
                "task_id": self.task_id,
                "run_id": self.run_id,
                "workspace_id": self.workspace_id,
                "skill_id": self.skill_id,
                "bundle_id": self.bundle_id,
                "version": self.version,
                "bundle_hash": self.bundle_hash,
                "adapter_session_id": self.adapter_session_id,
            }))
            .send()
            .await
            .map_err(|err| format!("Local Connector Skill cancel request failed: {err}"))?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .map_err(|err| format!("decode Local Connector Skill cancel response failed: {err}"))?;
        if !status.is_success() {
            return Err(body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Local Connector rejected Skill session cleanup")
                .to_string());
        }
        Ok(body
            .get("cancelled")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }
}

pub(super) async fn cleanup_local_skill_sessions(sessions: &[LocalSkillSessionHandle]) {
    let results =
        futures_util::future::join_all(sessions.iter().map(|session| session.cancel())).await;
    for (session, result) in sessions.iter().zip(results) {
        match result {
            Ok(true) => {}
            Ok(false) => warn!(
                run_id = session.run_id.as_str(),
                skill_id = session.skill_id.as_str(),
                "Local Connector Skill session was already absent during cleanup"
            ),
            Err(err) => warn!(
                run_id = session.run_id.as_str(),
                skill_id = session.skill_id.as_str(),
                error = err.as_str(),
                "failed to clean up Local Connector Skill session"
            ),
        }
    }
}

#[derive(Clone)]
pub(super) struct LocalSkillBuiltinProvider {
    server_name: String,
    tools: Vec<Value>,
    session: LocalSkillSessionHandle,
}

#[async_trait]
impl BuiltinToolProvider for LocalSkillBuiltinProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        self.tools.clone()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        if !self.tools.iter().any(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|tool_name| tool_name == name)
        }) {
            return Err(format!(
                "Local Skill operation was not published during prepare: {name}"
            ));
        }
        let url = skill_relay_url(
            self.session.base_url.as_str(),
            self.session.device_id.as_str(),
            self.session.workspace_id.as_str(),
            "execute",
        );
        let response = self
            .session
            .client
            .post(url)
            .headers(internal_skill_headers(
                self.session.internal_secret.as_str(),
                self.session.owner_user_id.as_str(),
            )?)
            .json(&json!({
                "task_id": self.session.task_id,
                "run_id": self.session.run_id,
                "workspace_id": self.session.workspace_id,
                "skill_id": self.session.skill_id,
                "bundle_id": self.session.bundle_id,
                "version": self.session.version,
                "bundle_hash": self.session.bundle_hash,
                "adapter_session_id": self.session.adapter_session_id,
                "operation": name,
                "arguments": args,
            }))
            .send()
            .await
            .map_err(|err| format!("Local Connector Skill execute request failed: {err}"))?;
        let status = response.status();
        let body = response.json::<Value>().await.map_err(|err| {
            format!("decode Local Connector Skill execute response failed: {err}")
        })?;
        if !status.is_success() {
            return Err(body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Local Connector rejected Skill execution")
                .to_string());
        }
        for (field, expected) in [
            ("skill_id", self.session.skill_id.as_str()),
            ("bundle_id", self.session.bundle_id.as_str()),
            ("version", self.session.version.as_str()),
            ("bundle_hash", self.session.bundle_hash.as_str()),
            (
                "adapter_session_id",
                self.session.adapter_session_id.as_str(),
            ),
            ("operation", name),
        ] {
            if body.get(field).and_then(Value::as_str) != Some(expected) {
                return Err(format!(
                    "Local Connector Skill execute response has mismatched {field}"
                ));
            }
        }
        body.get("result")
            .cloned()
            .ok_or_else(|| "Local Connector Skill execute response is missing result".to_string())
    }
}

async fn prepare_skill(
    service: &RunService,
    client: &reqwest::Client,
    task: &TaskRecord,
    run: &TaskRunRecord,
    skill: &ResolvedSkill,
) -> Result<PreparedSkill, String> {
    let installation = skill
        .installation
        .as_ref()
        .ok_or_else(|| format!("Skill installation is missing for {}", skill.resource.id))?;
    let owner_user_id = task_owner_user_id(task)
        .ok_or_else(|| "task owner user id is required for Skill preparation".to_string())?;
    let base_url = local_connector_service_base_url();
    let workspace_id =
        local_connector_workspace_id(task, installation.device_id.as_str()).unwrap_or_default();
    let secret = service
        .config
        .local_connector_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required for Skill preparation"
                .to_string()
        })?
        .to_string();
    let response = client
        .post(skill_relay_url(
            base_url.as_str(),
            installation.device_id.as_str(),
            workspace_id.as_str(),
            "prepare",
        ))
        .headers(internal_skill_headers(
            secret.as_str(),
            owner_user_id.as_str(),
        )?)
        .json(&json!({
            "task_id": task.id,
            "run_id": run.id,
            "workspace_id": workspace_id,
            "skill_id": skill.resource.id,
            "bundle_id": installation.bundle_id,
            "version": installation.version,
            "bundle_hash": installation.bundle_hash,
            "requested_permissions": [],
            "locale": task.mcp_config.builtin_prompt_locale,
        }))
        .send()
        .await
        .map_err(|err| format!("Local Connector Skill prepare request failed: {err}"))?;
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|err| format!("decode Local Connector Skill prepare response failed: {err}"))?;
    if !status.is_success() {
        let message = body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Local Connector rejected Skill preparation");
        return Err(format!(
            "prepare Skill {} failed: {message}",
            skill.resource.id
        ));
    }
    let adapter_session_id = body
        .get("adapter_session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "Local Connector Skill prepare response is missing adapter_session_id: {}",
                skill.resource.id
            )
        })?
        .to_string();
    let session_handle = LocalSkillSessionHandle {
        client: client.clone(),
        base_url: base_url.clone(),
        internal_secret: secret.clone(),
        owner_user_id: owner_user_id.clone(),
        device_id: installation.device_id.clone(),
        workspace_id: workspace_id.clone(),
        task_id: task.id.clone(),
        run_id: run.id.clone(),
        skill_id: skill.resource.id.clone(),
        bundle_id: installation.bundle_id.clone(),
        version: installation.version.clone(),
        bundle_hash: installation.bundle_hash.clone(),
        adapter_session_id: adapter_session_id.clone(),
    };
    let decoded = (|| -> Result<(String, Vec<Value>, Vec<String>), String> {
        for (field, expected) in [
            ("skill_id", skill.resource.id.as_str()),
            ("bundle_id", installation.bundle_id.as_str()),
            ("version", installation.version.as_str()),
            ("bundle_hash", installation.bundle_hash.as_str()),
        ] {
            if body.get(field).and_then(Value::as_str) != Some(expected) {
                return Err(format!(
                    "Local Connector Skill prepare response has mismatched {field}: {}",
                    skill.resource.id
                ));
            }
        }
        let instructions = body
            .get("instructions")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "Local Connector Skill prepare response is missing instructions: {}",
                    skill.resource.id
                )
            })?
            .to_string();
        let tools = normalized_skill_tools(body.get("tools"), skill.resource.id.as_str())?;
        let permissions = body
            .get("permissions")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok((instructions, tools, permissions))
    })();
    let (instructions, tools, permissions) = match decoded {
        Ok(decoded) => decoded,
        Err(err) => {
            cleanup_local_skill_sessions(std::slice::from_ref(&session_handle)).await;
            return Err(err);
        }
    };
    Ok(PreparedSkill {
        skill_id: skill.resource.id.clone(),
        display_name: skill.resource.display_name.clone(),
        instructions,
        server_name: local_skill_server_name(skill.resource.id.as_str()),
        tools,
        permissions,
        owner_user_id,
        device_id: installation.device_id.clone(),
        workspace_id,
        task_id: task.id.clone(),
        run_id: run.id.clone(),
        bundle_id: installation.bundle_id.clone(),
        version: installation.version.clone(),
        bundle_hash: installation.bundle_hash.clone(),
        adapter_session_id,
        base_url,
        internal_secret: secret,
    })
}

fn normalized_skill_tools(value: Option<&Value>, skill_id: &str) -> Result<Vec<Value>, String> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut tools = Vec::with_capacity(items.len());
    for tool in items {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("Local Connector Skill {skill_id} returned an invalid tool"))?;
        if !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(format!(
                "Local Connector Skill {skill_id} returned an unsafe tool name: {name}"
            ));
        }
        if tool.get("inputSchema").and_then(Value::as_object).is_none() {
            return Err(format!(
                "Local Connector Skill {skill_id} tool {name} is missing inputSchema"
            ));
        }
        tools.push(tool.clone());
    }
    Ok(tools)
}

fn internal_skill_headers(secret: &str, owner_user_id: &str) -> Result<HeaderMap, String> {
    let token = chatos_service_runtime::issue_internal_service_token(
        secret,
        "task-runner",
        "local-connector-service",
        "relay.skill",
        60,
    )?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-local-connector-caller",
        HeaderValue::from_static("task-runner"),
    );
    headers.insert(
        "x-local-connector-internal-token",
        HeaderValue::from_str(token.as_str()).map_err(|err| err.to_string())?,
    );
    headers.insert(
        "x-local-connector-owner-user-id",
        HeaderValue::from_str(owner_user_id).map_err(|err| err.to_string())?,
    );
    Ok(headers)
}

fn skill_relay_url(base_url: &str, device_id: &str, workspace_id: &str, action: &str) -> String {
    let mut url = format!(
        "{}/api/local-connectors/relay/{}/skills/{action}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(device_id),
    );
    if !workspace_id.trim().is_empty() {
        url.push_str("?workspace_id=");
        url.push_str(urlencoding::encode(workspace_id.trim()).as_ref());
    }
    url
}

fn local_connector_workspace_id(task: &TaskRecord, device_id: &str) -> Option<String> {
    local_connector_workspace_id_from_config(&task.mcp_config, device_id)
}

fn local_connector_workspace_id_from_config(
    config: &crate::models::TaskMcpConfig,
    device_id: &str,
) -> Option<String> {
    for server in &config.ephemeral_http_servers {
        if !server.name.trim().eq_ignore_ascii_case("local_connector")
            && !server.url.contains("/api/local-connectors/relay/")
        {
            continue;
        }
        let encoded_device = urlencoding::encode(device_id);
        if !server
            .url
            .contains(format!("/relay/{encoded_device}/").as_str())
        {
            continue;
        }
        if let Some(workspace_id) = query_parameter(server.url.as_str(), "workspace_id") {
            return Some(workspace_id);
        }
    }
    config
        .workspace_dir
        .as_deref()
        .and_then(|value| value.trim().strip_prefix("local://connector/"))
        .and_then(|rest| {
            let mut parts = rest.split('/');
            let task_device_id = parts.next()?.trim();
            let workspace_id = parts.next()?.trim();
            (task_device_id == device_id && !workspace_id.is_empty())
                .then(|| workspace_id.to_string())
        })
}

fn query_parameter(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1.split('#').next().unwrap_or_default();
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name == key {
            return urlencoding::decode(value)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
        }
    }
    None
}

fn local_skill_server_name(skill_id: &str) -> String {
    let suffix = skill_id
        .trim()
        .strip_prefix("internal_skill_")
        .unwrap_or(skill_id)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("local_skill_{suffix}")
}

fn task_owner_user_id(task: &TaskRecord) -> Option<String> {
    task.owner_user_id
        .as_deref()
        .or(task.creator_user_id.as_deref())
        .or(Some(task.subject_id.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn local_connector_service_base_url() -> String {
    std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_BASE_URL")
        .ok()
        .or_else(|| std::env::var("TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL").ok())
        .or_else(|| std::env::var("LOCAL_CONNECTOR_SERVICE_BASE_URL").ok())
        .or_else(|| std::env::var("CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL").ok())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:39230".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TaskEphemeralHttpMcpServer, TaskMcpConfig};
    use chatos_mcp_runtime::{BuiltinToolRegistry, McpExecutorBuilder};
    use std::collections::BTreeMap;

    #[test]
    fn resolves_workspace_from_local_connector_server() {
        let mut config = TaskMcpConfig::default();
        config.ephemeral_http_servers.push(TaskEphemeralHttpMcpServer {
            name: "local_connector".to_string(),
            url: "http://connector/api/local-connectors/relay/device-1/mcp?workspace_id=workspace%201&cwd=app".to_string(),
            headers: BTreeMap::new(),
            auth_mode: None,
        });
        assert_eq!(
            local_connector_workspace_id_from_config(&config, "device-1").as_deref(),
            Some("workspace 1")
        );
        assert_eq!(
            local_connector_workspace_id_from_config(&config, "device-2"),
            None
        );
    }

    #[test]
    fn skill_server_name_is_stable_and_safe() {
        assert_eq!(
            local_skill_server_name("internal_skill_plugin_creator"),
            "local_skill_plugin_creator"
        );
        assert_eq!(
            local_skill_server_name("internal_skill_figma-use"),
            "local_skill_figma_use"
        );
    }

    #[test]
    fn prepared_skill_tools_are_registered_with_the_model_runtime() {
        let prepared = PreparedSkill {
            skill_id: "internal_skill_visualize".to_string(),
            display_name: "Visualize".to_string(),
            instructions: "Create a local visualization.".to_string(),
            server_name: "local_skill_visualize".to_string(),
            tools: vec![json!({
                "name": "write_visualization_html",
                "description": "Write HTML locally.",
                "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
            })],
            permissions: vec!["workspace.write".to_string()],
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            bundle_id: "chatos.internal.visualize".to_string(),
            version: "1.0.0".to_string(),
            bundle_hash: "hash-1".to_string(),
            adapter_session_id: "session-1".to_string(),
            base_url: "http://127.0.0.1:39230".to_string(),
            internal_secret: "secret".to_string(),
        };
        let server = prepared.builtin_server();
        let session = prepared.session_handle(reqwest::Client::new());
        let provider = prepared.builtin_provider(session);
        let mut registry = BuiltinToolRegistry::new();
        registry.register(provider);
        let executor = McpExecutorBuilder::new()
            .with_builtin_servers([server])
            .with_builtin_registry(registry)
            .build_builtin_only()
            .expect("executor");
        let available_tools = executor.available_tools();
        let names = available_tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(names.contains(&"local_skill_visualize_write_visualization_html"));
    }
}
