// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::{api_url, ClientConfig};
use crate::relay::{RelayRequest, RelayResponse};
use crate::{LocalRuntime, LocalState};

mod bundled;
mod native;

use bundled::{internal_skill_bundle_hash, internal_skill_instructions, internal_skill_manifest};

const PREPARED_SKILL_SESSION_TTL_SECONDS: i64 = 2 * 60 * 60;

#[derive(Debug, Clone)]
struct PreparedSkillSession {
    skill_id: String,
    bundle_id: String,
    version: String,
    bundle_hash: String,
    workspace_id: String,
    allowed_operations: HashSet<String>,
    expires_at: i64,
}

static PREPARED_SKILL_SESSIONS: OnceLock<Mutex<HashMap<String, PreparedSkillSession>>> =
    OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InternalSkillCatalog {
    pub(crate) schema_version: u32,
    pub(crate) catalog_revision: String,
    pub(crate) skills: Vec<InternalSkillCatalogItem>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct InternalSkillCatalogItem {
    pub(crate) skill_id: String,
    pub(crate) bundle_id: String,
    pub(crate) version: String,
    pub(crate) name: String,
    pub(crate) display_name: String,
    pub(crate) description: String,
    pub(crate) category: String,
    pub(crate) entrypoint_kind: String,
    pub(crate) implementation_status: String,
    #[serde(default)]
    pub(crate) requires_workspace: bool,
    #[serde(default)]
    pub(crate) permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalSkillInventoryItem {
    pub(crate) skill_id: String,
    pub(crate) bundle_id: String,
    pub(crate) version: String,
    pub(crate) bundle_hash: String,
    pub(crate) status: String,
    pub(crate) dependency_status: String,
    pub(crate) last_error: Option<String>,
}

pub(crate) fn internal_skill_catalog() -> Result<InternalSkillCatalog> {
    let catalog = serde_json::from_str::<InternalSkillCatalog>(include_str!(
        "../../../skill_bundles/catalog/internal-skill-catalog.json"
    ))
    .context("decode embedded internal Skill catalog")?;
    if catalog.schema_version != 1 {
        return Err(anyhow!(
            "unsupported internal Skill catalog schema version: {}",
            catalog.schema_version
        ));
    }
    if catalog.catalog_revision.trim().is_empty() || catalog.skills.len() != 27 {
        return Err(anyhow!(
            "embedded internal Skill catalog is incomplete; expected 27 entries"
        ));
    }
    if catalog.skills.iter().any(|item| {
        item.skill_id.trim().is_empty()
            || item.bundle_id.trim().is_empty()
            || item.version.trim().is_empty()
            || item.name.trim().is_empty()
            || item.display_name.trim().is_empty()
            || item.description.trim().is_empty()
            || item.category.trim().is_empty()
            || item.entrypoint_kind.trim().is_empty()
    }) {
        return Err(anyhow!(
            "embedded internal Skill catalog contains an invalid entry"
        ));
    }
    for item in &catalog.skills {
        let manifest_text = internal_skill_manifest(item.skill_id.as_str()).ok_or_else(|| {
            anyhow!(
                "bundled internal Skill is missing skill.json: {}",
                item.skill_id
            )
        })?;
        if internal_skill_instructions(item.skill_id.as_str()).is_none() {
            return Err(anyhow!(
                "bundled internal Skill is missing instructions.md: {}",
                item.skill_id
            ));
        }
        let manifest = serde_json::from_str::<Value>(manifest_text)
            .with_context(|| format!("decode bundled skill.json for {}", item.skill_id))?;
        for (field, expected) in [
            ("skill_id", item.skill_id.as_str()),
            ("bundle_id", item.bundle_id.as_str()),
            ("version", item.version.as_str()),
            ("name", item.name.as_str()),
        ] {
            if manifest.get(field).and_then(Value::as_str) != Some(expected) {
                return Err(anyhow!(
                    "bundled skill.json has mismatched {field}: {}",
                    item.skill_id
                ));
            }
        }
    }
    Ok(catalog)
}

pub(crate) fn local_skill_inventory() -> Result<Vec<LocalSkillInventoryItem>> {
    Ok(internal_skill_catalog()?
        .skills
        .iter()
        .map(|item| {
            let implementation_ready = item.implementation_status == "ready";
            let bundle_complete = internal_skill_instructions(item.skill_id.as_str()).is_some()
                && internal_skill_manifest(item.skill_id.as_str()).is_some();
            let dependency_error = native::dependency_error(item.skill_id.as_str());
            let ready = implementation_ready && bundle_complete && dependency_error.is_none();
            LocalSkillInventoryItem {
                skill_id: item.skill_id.clone(),
                bundle_id: item.bundle_id.clone(),
                version: item.version.clone(),
                bundle_hash: internal_skill_bundle_hash(item),
                status: if ready {
                    "available"
                } else if implementation_ready && bundle_complete {
                    "unavailable"
                } else {
                    "unsupported"
                }
                .to_string(),
                dependency_status: if ready {
                    "available"
                } else if dependency_error.is_some() {
                    "missing"
                } else {
                    "unsupported"
                }
                .to_string(),
                last_error: (!ready).then(|| {
                    if let Some(error) = dependency_error {
                        error
                    } else if implementation_ready {
                        format!("{} has an incomplete bundled manifest", item.display_name)
                    } else {
                        format!(
                            "{} is bundled in the native catalog but its Local Connector adapter is not implemented yet",
                            item.display_name
                        )
                    }
                }),
            }
        })
        .collect())
}

pub(crate) fn skill_inventory_status_message() -> Result<Value> {
    Ok(json!({
        "type": "skill_inventory_status",
        "platform": local_platform(),
        "items": local_skill_inventory()?,
    }))
}

pub(crate) fn handle_skill_prepare(value: Value, state: &LocalState) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return skill_error_response("skill_prepare_response", "", 400, err.to_string())
        }
    };
    match prepare_skill(&request, state) {
        Ok(body) => RelayResponse {
            message_type: "skill_prepare_response".to_string(),
            request_id: request.request_id,
            status: 200,
            headers: BTreeMap::new(),
            body,
        }
        .into_value(),
        Err((status, message)) => skill_error_response(
            "skill_prepare_response",
            request.request_id.as_str(),
            status,
            message,
        ),
    }
}

pub(crate) fn handle_skill_execute(value: Value, state: &LocalState) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return skill_error_response("skill_execute_response", "", 400, err.to_string())
        }
    };
    match execute_skill(&request, state) {
        Ok(body) => RelayResponse {
            message_type: "skill_execute_response".to_string(),
            request_id: request.request_id,
            status: 200,
            headers: BTreeMap::new(),
            body,
        }
        .into_value(),
        Err((status, message)) => skill_error_response(
            "skill_execute_response",
            request.request_id.as_str(),
            status,
            message,
        ),
    }
}

pub(crate) fn handle_skill_cancel(value: Value) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(err) => return skill_error_response("skill_cancel_response", "", 400, err.to_string()),
    };
    match cancel_skill(&request) {
        Ok(body) => RelayResponse {
            message_type: "skill_cancel_response".to_string(),
            request_id: request.request_id,
            status: 200,
            headers: BTreeMap::new(),
            body,
        }
        .into_value(),
        Err((status, message)) => skill_error_response(
            "skill_cancel_response",
            request.request_id.as_str(),
            status,
            message,
        ),
    }
}

pub(crate) async fn fetch_user_skill_catalog(runtime: &LocalRuntime) -> Result<Value> {
    let (config, device_id) = configured_runtime(runtime).await?;
    let url = api_url(
        config.cloud_base_url.as_str(),
        format!(
            "/api/plugin-management/skills?device_id={}",
            urlencoding::encode(device_id.as_str())
        )
        .as_str(),
    );
    send_json(
        runtime
            .http_client
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {}", config.access_token)),
    )
    .await
}

pub(crate) async fn update_user_skill_preference(
    runtime: &LocalRuntime,
    skill_id: &str,
    enabled: bool,
) -> Result<Value> {
    let (config, device_id) = configured_runtime(runtime).await?;
    let url = api_url(
        config.cloud_base_url.as_str(),
        format!(
            "/api/plugin-management/skills/{}/preference",
            urlencoding::encode(skill_id)
        )
        .as_str(),
    );
    send_json(
        runtime
            .http_client
            .put(url)
            .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
            .json(&json!({ "device_id": device_id, "enabled": enabled })),
    )
    .await
}

pub(crate) async fn sync_skill_inventory(runtime: &LocalRuntime) -> Result<Value> {
    let (config, device_id) = configured_runtime(runtime).await?;
    let url = api_url(
        config.cloud_base_url.as_str(),
        "/api/plugin-management/skills/inventory",
    );
    send_json(
        runtime
            .http_client
            .put(url)
            .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
            .json(&json!({
                "device_id": device_id,
                "platform": local_platform(),
                "items": local_skill_inventory()?,
            })),
    )
    .await
}

fn prepare_skill(request: &RelayRequest, state: &LocalState) -> Result<Value, (u16, String)> {
    let skill_id = required_body_text(&request.body, "skill_id")?;
    let bundle_id = required_body_text(&request.body, "bundle_id")?;
    let version = required_body_text(&request.body, "version")?;
    let bundle_hash = required_body_text(&request.body, "bundle_hash")?;
    let catalog = internal_skill_catalog().map_err(|err| (500, err.to_string()))?;
    let item = catalog
        .skills
        .iter()
        .find(|item| item.skill_id == skill_id)
        .ok_or_else(|| (404, format!("unknown bundled Skill: {skill_id}")))?;
    if item.implementation_status != "ready" {
        return Err((
            409,
            format!(
                "Local Connector adapter is not implemented for {}",
                item.display_name
            ),
        ));
    }
    if item.bundle_id != bundle_id
        || item.version != version
        || internal_skill_bundle_hash(item) != bundle_hash
    {
        return Err((
            409,
            "Skill bundle snapshot does not match the installed bundle".to_string(),
        ));
    }
    let instructions = internal_skill_instructions(skill_id.as_str())
        .ok_or_else(|| (500, "bundled Skill instructions are missing".to_string()))?;
    if item.requires_workspace {
        let workspace_id = request.workspace_id.trim();
        if workspace_id.is_empty() {
            return Err((
                400,
                format!(
                    "{} requires an authorized local workspace",
                    item.display_name
                ),
            ));
        }
        if state.workspace_by_id(workspace_id).is_none() {
            return Err((404, "Skill workspace is not registered locally".to_string()));
        }
    }
    let tools = native::tool_definitions(skill_id.as_str(), state, request)
        .map_err(|err| (409, err.to_string()))?;
    let adapter_session_id = Uuid::new_v4().to_string();
    let session = PreparedSkillSession {
        skill_id: item.skill_id.clone(),
        bundle_id: item.bundle_id.clone(),
        version: item.version.clone(),
        bundle_hash: bundle_hash.clone(),
        workspace_id: request.workspace_id.trim().to_string(),
        allowed_operations: tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect(),
        expires_at: Utc::now().timestamp() + PREPARED_SKILL_SESSION_TTL_SECONDS,
    };
    let mut sessions = prepared_skill_sessions()
        .lock()
        .map_err(|_| (500, "Skill session store is unavailable".to_string()))?;
    prune_expired_sessions(&mut sessions);
    sessions.insert(adapter_session_id.clone(), session);
    Ok(json!({
        "skill_id": item.skill_id,
        "bundle_id": item.bundle_id,
        "version": item.version,
        "bundle_hash": bundle_hash,
        "entrypoint_kind": item.entrypoint_kind,
        "instructions": instructions,
        "tools": tools,
        "permissions": item.permissions.clone(),
        "requires_workspace": item.requires_workspace,
        "adapter_session_id": adapter_session_id,
        "dependency_status": "available",
        "permission_status": "available",
    }))
}

fn execute_skill(request: &RelayRequest, state: &LocalState) -> Result<Value, (u16, String)> {
    let adapter_session_id = required_body_text(&request.body, "adapter_session_id")?;
    let skill_id = required_body_text(&request.body, "skill_id")?;
    let bundle_id = required_body_text(&request.body, "bundle_id")?;
    let version = required_body_text(&request.body, "version")?;
    let bundle_hash = required_body_text(&request.body, "bundle_hash")?;
    let operation = required_body_text(&request.body, "operation")?;
    let arguments = request
        .body
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let session = {
        let mut sessions = prepared_skill_sessions()
            .lock()
            .map_err(|_| (500, "Skill session store is unavailable".to_string()))?;
        prune_expired_sessions(&mut sessions);
        sessions
            .get(adapter_session_id.as_str())
            .cloned()
            .ok_or_else(|| {
                (
                    410,
                    "Skill adapter session is missing or expired".to_string(),
                )
            })?
    };
    if session.skill_id != skill_id
        || session.bundle_id != bundle_id
        || session.version != version
        || session.bundle_hash != bundle_hash
    {
        return Err((
            409,
            "Skill execute snapshot does not match the prepared session".to_string(),
        ));
    }
    if session.workspace_id != request.workspace_id.trim() {
        return Err((
            409,
            "Skill execute workspace does not match the prepared session".to_string(),
        ));
    }
    if !session.allowed_operations.contains(operation.as_str()) {
        return Err((
            403,
            format!("Skill operation was not published during prepare: {operation}"),
        ));
    }
    let result = native::execute(
        skill_id.as_str(),
        operation.as_str(),
        &arguments,
        state,
        request,
    )
    .map_err(|err| (400, err.to_string()))?;
    Ok(json!({
        "skill_id": skill_id,
        "bundle_id": bundle_id,
        "version": version,
        "bundle_hash": bundle_hash,
        "adapter_session_id": adapter_session_id,
        "operation": operation,
        "result": result,
    }))
}

fn cancel_skill(request: &RelayRequest) -> Result<Value, (u16, String)> {
    let adapter_session_id = required_body_text(&request.body, "adapter_session_id")?;
    let skill_id = required_body_text(&request.body, "skill_id")?;
    let bundle_id = required_body_text(&request.body, "bundle_id")?;
    let version = required_body_text(&request.body, "version")?;
    let bundle_hash = required_body_text(&request.body, "bundle_hash")?;
    let mut sessions = prepared_skill_sessions()
        .lock()
        .map_err(|_| (500, "Skill session store is unavailable".to_string()))?;
    prune_expired_sessions(&mut sessions);
    let Some(session) = sessions.get(adapter_session_id.as_str()) else {
        return Ok(json!({
            "adapter_session_id": adapter_session_id,
            "cancelled": false,
        }));
    };
    if session.skill_id != skill_id
        || session.bundle_id != bundle_id
        || session.version != version
        || session.bundle_hash != bundle_hash
        || session.workspace_id != request.workspace_id.trim()
    {
        return Err((
            409,
            "Skill cancel snapshot does not match the prepared session".to_string(),
        ));
    }
    sessions.remove(adapter_session_id.as_str());
    Ok(json!({
        "adapter_session_id": adapter_session_id,
        "cancelled": true,
    }))
}

fn prepared_skill_sessions() -> &'static Mutex<HashMap<String, PreparedSkillSession>> {
    PREPARED_SKILL_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prune_expired_sessions(sessions: &mut HashMap<String, PreparedSkillSession>) {
    let now = Utc::now().timestamp();
    sessions.retain(|_, session| session.expires_at > now);
}

fn required_body_text(body: &Value, field: &str) -> Result<String, (u16, String)> {
    body.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| (400, format!("{field} is required")))
}

fn skill_error_response(
    message_type: &str,
    request_id: &str,
    status: u16,
    message: String,
) -> Value {
    RelayResponse {
        message_type: message_type.to_string(),
        request_id: request_id.to_string(),
        status,
        headers: BTreeMap::new(),
        body: json!({ "error": message }),
    }
    .into_value()
}

fn local_platform() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x64"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "windows-arm64"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x64"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

async fn configured_runtime(runtime: &LocalRuntime) -> Result<(ClientConfig, String)> {
    let state = runtime.state.read().await;
    let config = ClientConfig::from_state(&state, runtime.state_path.clone())
        .ok_or_else(|| anyhow!("Local Connector is not configured"))?;
    let device_id = state
        .device_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("Local Connector device is not registered"))?;
    Ok((config, device_id))
}

async fn send_json(request: reqwest::RequestBuilder) -> Result<Value> {
    let response = request
        .send()
        .await
        .context("request Local Connector Skill API")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("read Local Connector Skill API response")?;
    let value =
        serde_json::from_str::<Value>(body.as_str()).unwrap_or_else(|_| json!({ "error": body }));
    if !status.is_success() {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| value.get("message").and_then(Value::as_str))
            .unwrap_or("Local Connector Skill request was rejected");
        return Err(anyhow!("{message}"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
