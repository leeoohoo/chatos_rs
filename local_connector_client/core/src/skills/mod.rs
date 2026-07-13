// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::{api_url, ClientConfig};
use crate::relay::{RelayRequest, RelayResponse};
use crate::{LocalRuntime, LocalState};

mod native;

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

fn internal_skill_bundle_hash(item: &InternalSkillCatalogItem) -> String {
    let instructions_hash = internal_skill_instructions(item.skill_id.as_str())
        .map(|value| hex::encode(Sha256::digest(value.as_bytes())))
        .unwrap_or_else(|| "none".to_string());
    let manifest_hash = internal_skill_manifest(item.skill_id.as_str())
        .map(|value| hex::encode(Sha256::digest(value.as_bytes())))
        .unwrap_or_else(|| "none".to_string());
    let payload = format!(
        "chatos-internal-skill-bundle-v2\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        item.skill_id,
        item.bundle_id,
        item.version,
        item.entrypoint_kind,
        item.implementation_status,
        instructions_hash,
        manifest_hash,
        item.requires_workspace,
        item.permissions.join(","),
    );
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn internal_skill_manifest(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "internal_skill_plugin_creator" => Some(include_str!(
            "../../../skill_bundles/internal/plugin-creator/1.0.0/skill.json"
        )),
        "internal_skill_openai_docs" => Some(include_str!(
            "../../../skill_bundles/internal/openai-docs/1.0.0/skill.json"
        )),
        "internal_skill_skill_creator" => Some(include_str!(
            "../../../skill_bundles/internal/skill-creator/1.0.0/skill.json"
        )),
        "internal_skill_skill_installer" => Some(include_str!(
            "../../../skill_bundles/internal/skill-installer/1.0.0/skill.json"
        )),
        "internal_skill_remotion" => Some(include_str!(
            "../../../skill_bundles/internal/remotion-best-practices/1.0.0/skill.json"
        )),
        "internal_skill_visualize" => Some(include_str!(
            "../../../skill_bundles/internal/visualize/1.0.0/skill.json"
        )),
        "internal_skill_documents" => Some(include_str!(
            "../../../skill_bundles/internal/documents/1.0.0/skill.json"
        )),
        "internal_skill_pdf" => Some(include_str!(
            "../../../skill_bundles/internal/pdf/1.0.0/skill.json"
        )),
        "internal_skill_presentations" => Some(include_str!(
            "../../../skill_bundles/internal/presentations/1.0.0/skill.json"
        )),
        "internal_skill_spreadsheets" => Some(include_str!(
            "../../../skill_bundles/internal/spreadsheets/1.0.0/skill.json"
        )),
        "internal_skill_template_creator" => Some(include_str!(
            "../../../skill_bundles/internal/template-creator/1.0.0/skill.json"
        )),
        "internal_skill_imagegen" => Some(include_str!(
            "../../../skill_bundles/internal/imagegen/1.0.0/skill.json"
        )),
        "internal_skill_figma_code_connect" => Some(include_str!(
            "../../../skill_bundles/internal/figma-code-connect/1.0.0/skill.json"
        )),
        "internal_skill_figma_create_new_file" => Some(include_str!(
            "../../../skill_bundles/internal/figma-create-new-file/1.0.0/skill.json"
        )),
        "internal_skill_figma_design_to_code" => Some(include_str!(
            "../../../skill_bundles/internal/figma-design-to-code/1.0.0/skill.json"
        )),
        "internal_skill_figma_generate_design" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-design/1.0.0/skill.json"
        )),
        "internal_skill_figma_generate_diagram" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-diagram/1.0.0/skill.json"
        )),
        "internal_skill_figma_generate_library" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-library/1.0.0/skill.json"
        )),
        "internal_skill_figma_implement_motion" => Some(include_str!(
            "../../../skill_bundles/internal/figma-implement-motion/1.0.0/skill.json"
        )),
        "internal_skill_figma_swiftui" => Some(include_str!(
            "../../../skill_bundles/internal/figma-swiftui/1.0.0/skill.json"
        )),
        "internal_skill_figma_use" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use/1.0.0/skill.json"
        )),
        "internal_skill_figma_use_figjam" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-figjam/1.0.0/skill.json"
        )),
        "internal_skill_figma_use_motion" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-motion/1.0.0/skill.json"
        )),
        "internal_skill_figma_use_slides" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-slides/1.0.0/skill.json"
        )),
        "internal_skill_browser" => Some(include_str!(
            "../../../skill_bundles/internal/control-in-app-browser/1.0.0/skill.json"
        )),
        "internal_skill_computer_use" => Some(include_str!(
            "../../../skill_bundles/internal/computer-use/1.0.0/skill.json"
        )),
        "internal_skill_excel_live_control" => Some(include_str!(
            "../../../skill_bundles/internal/excel-live-control/1.0.0/skill.json"
        )),
        _ => None,
    }
}

fn internal_skill_instructions(skill_id: &str) -> Option<&'static str> {
    match skill_id {
        "internal_skill_plugin_creator" => Some(include_str!(
            "../../../skill_bundles/internal/plugin-creator/1.0.0/instructions.md"
        )),
        "internal_skill_openai_docs" => Some(include_str!(
            "../../../skill_bundles/internal/openai-docs/1.0.0/instructions.md"
        )),
        "internal_skill_skill_creator" => Some(include_str!(
            "../../../skill_bundles/internal/skill-creator/1.0.0/instructions.md"
        )),
        "internal_skill_skill_installer" => Some(include_str!(
            "../../../skill_bundles/internal/skill-installer/1.0.0/instructions.md"
        )),
        "internal_skill_remotion" => Some(include_str!(
            "../../../skill_bundles/internal/remotion-best-practices/1.0.0/instructions.md"
        )),
        "internal_skill_visualize" => Some(include_str!(
            "../../../skill_bundles/internal/visualize/1.0.0/instructions.md"
        )),
        "internal_skill_documents" => Some(include_str!(
            "../../../skill_bundles/internal/documents/1.0.0/instructions.md"
        )),
        "internal_skill_pdf" => Some(include_str!(
            "../../../skill_bundles/internal/pdf/1.0.0/instructions.md"
        )),
        "internal_skill_presentations" => Some(include_str!(
            "../../../skill_bundles/internal/presentations/1.0.0/instructions.md"
        )),
        "internal_skill_spreadsheets" => Some(include_str!(
            "../../../skill_bundles/internal/spreadsheets/1.0.0/instructions.md"
        )),
        "internal_skill_template_creator" => Some(include_str!(
            "../../../skill_bundles/internal/template-creator/1.0.0/instructions.md"
        )),
        "internal_skill_imagegen" => Some(include_str!(
            "../../../skill_bundles/internal/imagegen/1.0.0/instructions.md"
        )),
        "internal_skill_figma_code_connect" => Some(include_str!(
            "../../../skill_bundles/internal/figma-code-connect/1.0.0/instructions.md"
        )),
        "internal_skill_figma_create_new_file" => Some(include_str!(
            "../../../skill_bundles/internal/figma-create-new-file/1.0.0/instructions.md"
        )),
        "internal_skill_figma_design_to_code" => Some(include_str!(
            "../../../skill_bundles/internal/figma-design-to-code/1.0.0/instructions.md"
        )),
        "internal_skill_figma_generate_design" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-design/1.0.0/instructions.md"
        )),
        "internal_skill_figma_generate_diagram" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-diagram/1.0.0/instructions.md"
        )),
        "internal_skill_figma_generate_library" => Some(include_str!(
            "../../../skill_bundles/internal/figma-generate-library/1.0.0/instructions.md"
        )),
        "internal_skill_figma_implement_motion" => Some(include_str!(
            "../../../skill_bundles/internal/figma-implement-motion/1.0.0/instructions.md"
        )),
        "internal_skill_figma_swiftui" => Some(include_str!(
            "../../../skill_bundles/internal/figma-swiftui/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use_figjam" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-figjam/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use_motion" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-motion/1.0.0/instructions.md"
        )),
        "internal_skill_figma_use_slides" => Some(include_str!(
            "../../../skill_bundles/internal/figma-use-slides/1.0.0/instructions.md"
        )),
        "internal_skill_browser" => Some(include_str!(
            "../../../skill_bundles/internal/control-in-app-browser/1.0.0/instructions.md"
        )),
        "internal_skill_computer_use" => Some(include_str!(
            "../../../skill_bundles/internal/computer-use/1.0.0/instructions.md"
        )),
        "internal_skill_excel_live_control" => Some(include_str!(
            "../../../skill_bundles/internal/excel-live-control/1.0.0/instructions.md"
        )),
        _ => None,
    }
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
mod tests {
    use super::*;
    use crate::WorkspaceState;
    use std::fs;

    #[test]
    fn embedded_catalog_contains_all_expected_skills() {
        let catalog = internal_skill_catalog().expect("catalog");
        assert_eq!(catalog.skills.len(), 27);
        assert_eq!(
            catalog
                .skills
                .iter()
                .filter(|item| item.implementation_status == "ready")
                .count(),
            12
        );
        assert!(catalog.skills.iter().all(|item| {
            !item.name.trim().is_empty()
                && !item.description.trim().is_empty()
                && !item.category.trim().is_empty()
        }));
    }

    #[test]
    fn inventory_never_reports_planned_adapter_as_available() {
        let inventory = local_skill_inventory().expect("inventory");
        assert_eq!(inventory.len(), 27);
        let available_count = inventory
            .iter()
            .filter(|item| item.status == "available")
            .count();
        assert!((11..=12).contains(&available_count));
        let ready_ids = internal_skill_catalog()
            .expect("catalog")
            .skills
            .into_iter()
            .filter(|item| item.implementation_status == "ready")
            .map(|item| item.skill_id)
            .collect::<HashSet<_>>();
        assert!(inventory
            .iter()
            .all(|item| ready_ids.contains(item.skill_id.as_str()) || item.status != "available"));
        assert!(inventory
            .iter()
            .filter(|item| item.status == "available")
            .all(|item| item.dependency_status == "available"));
        assert!(inventory.iter().all(|item| matches!(
            item.dependency_status.as_str(),
            "available" | "missing" | "unsupported" | "error"
        )));
    }

    #[test]
    fn ready_bundle_v2_fingerprint_matches_plugin_management_seed() {
        let catalog = internal_skill_catalog().expect("catalog");
        let rows = catalog
            .skills
            .iter()
            .filter(|item| item.implementation_status == "ready")
            .map(|item| format!("{}:{}", item.skill_id, internal_skill_bundle_hash(item)))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            hex::encode(Sha256::digest(rows.as_bytes())),
            "91dcdc4f36bfa4aa3f7e56d9f9d2c62fe299d2f1175c373fbbe2cdc05168ecee"
        );
    }

    #[test]
    fn all_27_bundled_skill_fingerprints_match_plugin_management_seed() {
        let catalog = internal_skill_catalog().expect("catalog");
        let rows = catalog
            .skills
            .iter()
            .map(|item| format!("{}:{}", item.skill_id, internal_skill_bundle_hash(item)))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            hex::encode(Sha256::digest(rows.as_bytes())),
            "444a397c67701aec2fab0d8ba34bee950f802c84934ce8f2b9718554be7279d2"
        );
    }

    #[test]
    fn ready_skill_prepare_returns_local_instructions() {
        let item = internal_skill_catalog()
            .expect("catalog")
            .skills
            .into_iter()
            .find(|item| item.skill_id == "internal_skill_remotion")
            .expect("remotion");
        let request = json!({
            "type": "skill_prepare_request",
            "request_id": "request-1",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": internal_skill_bundle_hash(&item),
            }
        });
        let response = handle_skill_prepare(request, &LocalState::default());
        assert_eq!(response.get("status").and_then(Value::as_u64), Some(200));
        assert!(response
            .pointer("/body/instructions")
            .and_then(Value::as_str)
            .is_some_and(|value| value.contains("Remotion")));
    }

    #[test]
    fn native_skill_execute_requires_prepared_snapshot_and_writes_locally() {
        let root = std::env::temp_dir().join(format!("chatos-skill-e2e-{}", Uuid::new_v4()));
        fs::create_dir_all(root.as_path()).expect("workspace");
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.clone(),
                alias: "test".to_string(),
                fingerprint: "fp".to_string(),
            }],
            ..LocalState::default()
        };
        let item = internal_skill_catalog()
            .expect("catalog")
            .skills
            .into_iter()
            .find(|item| item.skill_id == "internal_skill_visualize")
            .expect("visualize");
        let bundle_hash = internal_skill_bundle_hash(&item);
        let prepare = handle_skill_prepare(
            json!({
                "type": "skill_prepare_request",
                "request_id": "prepare-1",
                "owner_user_id": "owner-1",
                "device_id": "device-1",
                "workspace_id": "workspace-1",
                "body": {
                    "skill_id": item.skill_id,
                    "bundle_id": item.bundle_id,
                    "version": item.version,
                    "bundle_hash": bundle_hash,
                }
            }),
            &state,
        );
        assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
        let adapter_session_id = prepare
            .pointer("/body/adapter_session_id")
            .and_then(Value::as_str)
            .expect("adapter session");
        let execute = handle_skill_execute(
            json!({
                "type": "skill_execute_request",
                "request_id": "execute-1",
                "owner_user_id": "owner-1",
                "device_id": "device-1",
                "workspace_id": "workspace-1",
                "body": {
                    "skill_id": item.skill_id,
                    "bundle_id": item.bundle_id,
                    "version": item.version,
                    "bundle_hash": bundle_hash,
                    "adapter_session_id": adapter_session_id,
                    "operation": "write_visualization_html",
                    "arguments": {
                        "target_path": "artifacts/e2e.html",
                        "title": "E2E",
                        "body_html": "<main>ready</main>"
                    }
                }
            }),
            &state,
        );
        assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
        assert!(root.join("artifacts/e2e.html").is_file());
        let cancel = handle_skill_cancel(json!({
            "type": "skill_cancel_request",
            "request_id": "cancel-1",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
                "adapter_session_id": adapter_session_id,
            }
        }));
        assert_eq!(cancel.get("status").and_then(Value::as_u64), Some(200));
        assert_eq!(
            cancel.pointer("/body/cancelled").and_then(Value::as_bool),
            Some(true)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn document_skill_prepare_publishes_and_executes_native_tools() {
        let root = std::env::temp_dir().join(format!("chatos-document-e2e-{}", Uuid::new_v4()));
        fs::create_dir_all(root.as_path()).expect("workspace");
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.clone(),
                alias: "test".to_string(),
                fingerprint: "fp".to_string(),
            }],
            ..LocalState::default()
        };
        let item = internal_skill_catalog()
            .expect("catalog")
            .skills
            .into_iter()
            .find(|item| item.skill_id == "internal_skill_documents")
            .expect("documents");
        let bundle_hash = internal_skill_bundle_hash(&item);
        let prepare = handle_skill_prepare(
            json!({
                "type": "skill_prepare_request",
                "request_id": "prepare-documents",
                "owner_user_id": "owner-1",
                "device_id": "device-1",
                "workspace_id": "workspace-1",
                "body": {
                    "skill_id": item.skill_id,
                    "bundle_id": item.bundle_id,
                    "version": item.version,
                    "bundle_hash": bundle_hash,
                }
            }),
            &state,
        );
        assert_eq!(prepare.get("status").and_then(Value::as_u64), Some(200));
        assert!(prepare
            .pointer("/body/tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| tools
                .iter()
                .any(|tool| { tool.get("name").and_then(Value::as_str) == Some("create_docx") })));
        let adapter_session_id = prepare
            .pointer("/body/adapter_session_id")
            .and_then(Value::as_str)
            .expect("adapter session");
        let execute = handle_skill_execute(
            json!({
                "type": "skill_execute_request",
                "request_id": "execute-documents",
                "owner_user_id": "owner-1",
                "device_id": "device-1",
                "workspace_id": "workspace-1",
                "body": {
                    "skill_id": item.skill_id,
                    "bundle_id": item.bundle_id,
                    "version": item.version,
                    "bundle_hash": bundle_hash,
                    "adapter_session_id": adapter_session_id,
                    "operation": "create_docx",
                    "arguments": {
                        "target_path": "artifacts/document.docx",
                        "title": "本机文档",
                        "paragraphs": ["由 Local Connector 创建。"]
                    }
                }
            }),
            &state,
        );
        assert_eq!(execute.get("status").and_then(Value::as_u64), Some(200));
        assert!(root.join("artifacts/document.docx").is_file());
        let cancel = handle_skill_cancel(json!({
            "type": "skill_cancel_request",
            "request_id": "cancel-documents",
            "owner_user_id": "owner-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {
                "skill_id": item.skill_id,
                "bundle_id": item.bundle_id,
                "version": item.version,
                "bundle_hash": bundle_hash,
                "adapter_session_id": adapter_session_id,
            }
        }));
        assert_eq!(
            cancel.pointer("/body/cancelled").and_then(Value::as_bool),
            Some(true)
        );
        let _ = fs::remove_dir_all(root);
    }
}
