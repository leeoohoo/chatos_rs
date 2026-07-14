// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use chatos_plugin_management_sdk::{
    LocalConnectorSkillInventoryItem, LocalConnectorSkillInventoryRequest, SkillInstallationRecord,
    UpdateUserSkillPreferenceRequest, UserSkillCatalogItem, UserSkillCatalogResponse,
};
use serde::Deserialize;
use serde_json::Value;

use crate::models::CurrentUser;
use crate::state::AppState;

use super::{ensure_device_active_lease, load_owned_device, ApiError};

#[derive(Debug, Deserialize)]
pub(super) struct SkillCatalogQuery {
    device_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateSkillPreferenceRequest {
    device_id: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct SyncSkillInventoryRequest {
    device_id: String,
    platform: String,
    #[serde(default)]
    items: Vec<LocalConnectorSkillInventoryItem>,
}

#[derive(Debug, Deserialize)]
struct SocketSkillInventoryMessage {
    #[serde(rename = "type")]
    _message_type: String,
    platform: String,
    #[serde(default)]
    items: Vec<LocalConnectorSkillInventoryItem>,
}

pub(super) async fn list_user_skills(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<SkillCatalogQuery>,
) -> Result<Json<UserSkillCatalogResponse>, ApiError> {
    require_human_user(&user)?;
    load_owned_device(&state, &user, query.device_id.as_str(), true).await?;
    ensure_device_active_lease(
        &state,
        user.effective_owner_user_id(),
        query.device_id.as_str(),
    )
    .await?;
    let catalog = state
        .plugin_management_client
        .list_user_skill_catalog(
            user.effective_owner_user_id(),
            Some(query.device_id.as_str()),
        )
        .await
        .map_err(plugin_management_error)?;
    Ok(Json(catalog_for_device(catalog, query.device_id.as_str())))
}

pub(super) async fn update_user_skill_preference(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(skill_id): Path<String>,
    Json(request): Json<UpdateSkillPreferenceRequest>,
) -> Result<Json<UserSkillCatalogItem>, ApiError> {
    require_human_user(&user)?;
    load_owned_device(&state, &user, request.device_id.as_str(), true).await?;
    ensure_device_active_lease(
        &state,
        user.effective_owner_user_id(),
        request.device_id.as_str(),
    )
    .await?;

    if request.enabled {
        let catalog = state
            .plugin_management_client
            .list_user_skill_catalog(
                user.effective_owner_user_id(),
                Some(request.device_id.as_str()),
            )
            .await
            .map_err(plugin_management_error)?;
        let item = catalog
            .items
            .into_iter()
            .find(|item| item.skill.id == skill_id)
            .ok_or_else(|| ApiError::not_found("Skill not found"))?;
        let item = catalog_item_for_device(item, request.device_id.as_str());
        if !item.available {
            return Err(ApiError::conflict(
                "skill_not_available",
                item.reason.unwrap_or_else(|| {
                    "Skill is not available on this Local Connector".to_string()
                }),
            ));
        }
    }

    let item = state
        .plugin_management_client
        .update_user_skill_preference(
            skill_id.as_str(),
            &UpdateUserSkillPreferenceRequest {
                owner_user_id: user.effective_owner_user_id().to_string(),
                enabled: request.enabled,
            },
        )
        .await
        .map_err(plugin_management_error)?;
    Ok(Json(catalog_item_for_device(
        item,
        request.device_id.as_str(),
    )))
}

pub(super) async fn sync_user_skill_inventory(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(request): Json<SyncSkillInventoryRequest>,
) -> Result<Json<Vec<SkillInstallationRecord>>, ApiError> {
    require_human_user(&user)?;
    load_owned_device(&state, &user, request.device_id.as_str(), true).await?;
    ensure_device_active_lease(
        &state,
        user.effective_owner_user_id(),
        request.device_id.as_str(),
    )
    .await?;
    let records = sync_inventory(
        &state,
        user.effective_owner_user_id(),
        request.device_id.as_str(),
        request.platform,
        request.items,
    )
    .await
    .map_err(plugin_management_error)?;
    Ok(Json(records))
}

pub(super) fn is_skill_inventory_status_message(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .is_some_and(|value| value == "skill_inventory_status")
}

pub(super) async fn sync_socket_skill_inventory(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    text: &str,
) -> Result<usize, String> {
    let message = serde_json::from_str::<SocketSkillInventoryMessage>(text)
        .map_err(|err| format!("decode Skill inventory status failed: {err}"))?;
    let count = message.items.len();
    sync_inventory(
        state,
        owner_user_id,
        device_id,
        message.platform,
        message.items,
    )
    .await
    .map_err(|err| err.to_string())?;
    Ok(count)
}

pub(super) async fn mark_device_skills_offline(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
) -> Result<usize, String> {
    let catalog = state
        .plugin_management_client
        .list_user_skill_catalog(owner_user_id, Some(device_id))
        .await
        .map_err(|err| err.to_string())?;
    let platform = catalog
        .items
        .iter()
        .filter_map(|item| item.installation.as_ref())
        .find(|installation| installation.device_id == device_id)
        .map(|installation| installation.platform.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let items = catalog
        .items
        .into_iter()
        .filter_map(|item| {
            let bundle_id = item.skill.content.bundle_id?;
            let version = item.skill.content.bundle_version?;
            let bundle_hash = item.skill.content.bundle_hash.or_else(|| {
                item.installation
                    .map(|installation| installation.bundle_hash)
            })?;
            Some(LocalConnectorSkillInventoryItem {
                skill_id: item.skill.id,
                bundle_id,
                version,
                bundle_hash,
                status: "unavailable".to_string(),
                dependency_status: "error".to_string(),
                last_error: Some("Local Connector device is offline".to_string()),
            })
        })
        .collect::<Vec<_>>();
    let count = items.len();
    sync_inventory(state, owner_user_id, device_id, platform, items)
        .await
        .map_err(|err| err.to_string())?;
    Ok(count)
}

async fn sync_inventory(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    platform: String,
    items: Vec<LocalConnectorSkillInventoryItem>,
) -> Result<Vec<SkillInstallationRecord>, chatos_plugin_management_sdk::PluginManagementClientError>
{
    state
        .plugin_management_client
        .sync_local_connector_skill_inventory(&LocalConnectorSkillInventoryRequest {
            owner_user_id: owner_user_id.to_string(),
            device_id: device_id.to_string(),
            platform,
            items,
        })
        .await
}

fn catalog_for_device(
    catalog: UserSkillCatalogResponse,
    device_id: &str,
) -> UserSkillCatalogResponse {
    let items = catalog
        .items
        .into_iter()
        .map(|item| catalog_item_for_device(item, device_id))
        .collect::<Vec<_>>();
    UserSkillCatalogResponse {
        total: items.len() as u64,
        items,
    }
}

fn catalog_item_for_device(
    mut item: UserSkillCatalogItem,
    device_id: &str,
) -> UserSkillCatalogItem {
    let reported_by_device = item
        .installation
        .as_ref()
        .is_some_and(|installation| installation.device_id == device_id);
    if !reported_by_device {
        item.available = false;
        item.status = "not_installed".to_string();
        item.reason =
            Some("Skill bundle has not been reported by this Local Connector client".to_string());
        item.installation = None;
    }
    item
}

fn require_human_user(user: &CurrentUser) -> Result<(), ApiError> {
    if user.principal_type == "human_user" {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "Local Connector Skill configuration requires a human user",
        ))
    }
}

fn plugin_management_error(
    error: chatos_plugin_management_sdk::PluginManagementClientError,
) -> ApiError {
    match error {
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 400,
            message,
        } => ApiError::bad_request(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 403,
            message,
        } => ApiError::forbidden(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 404,
            message,
        } => ApiError::not_found(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 409,
            message,
        } => ApiError::conflict("skill_preference_rejected", message),
        other => ApiError::service_unavailable(other.to_string()),
    }
}
