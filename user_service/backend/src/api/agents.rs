use axum::extract::{Path, State};
use axum::{Extension, Json};

use crate::auth::{hash_password, normalize_display_name, normalize_username, CurrentPrincipal};
use crate::models::{
    AgentAccountListItem, AgentAccountRecord, CreateAgentAccountRequest, ResetAgentPasswordRequest,
    UpdateAgentAccountRequest,
};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::{bad_request, forbidden, internal_error, not_found, ApiResult, ApiStatusResult};

pub async fn list_agent_accounts(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
) -> ApiResult<Vec<AgentAccountListItem>> {
    if principal.is_super_admin() {
        return state
            .store
            .list_agent_accounts()
            .await
            .map(Json)
            .map_err(internal_error);
    }

    let Some(user_id) = principal.user_id.as_deref() else {
        return Err(not_found("current user not found"));
    };
    state
        .store
        .list_agent_accounts_by_owner(user_id)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn create_agent_account(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<CreateAgentAccountRequest>,
) -> ApiResult<AgentAccountListItem> {
    let username = normalize_username(input.username.as_str()).map_err(bad_request)?;
    if state
        .store
        .find_agent_by_username(username.as_str())
        .await
        .map_err(internal_error)?
        .is_some()
    {
        return Err(bad_request("agent username already exists"));
    }

    let owner_user_id = match input.owner_user_id.as_deref() {
        Some(owner_user_id) if principal.is_super_admin() => owner_user_id.to_string(),
        Some(owner_user_id) if principal.user_id.as_deref() != Some(owner_user_id) => {
            return Err(forbidden("cannot create agent for another user"));
        }
        Some(owner_user_id) => owner_user_id.to_string(),
        None => principal
            .user_id
            .clone()
            .ok_or_else(|| not_found("current user not found"))?,
    };
    let Some(owner) = state
        .store
        .find_user_by_id(owner_user_id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("owner user not found"));
    };
    if !owner.enabled {
        return Err(bad_request("owner user is disabled"));
    }

    let now = now_rfc3339();
    let agent = AgentAccountRecord {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.clone(),
        display_name: normalize_display_name(input.display_name.as_deref(), &username),
        password_hash: hash_password(input.password.as_str()).map_err(bad_request)?,
        owner_user_id: owner.id,
        enabled: input.enabled.unwrap_or(true),
        created_at: now.clone(),
        updated_at: now,
        last_login_at: None,
    };
    state
        .store
        .insert_agent_record(&agent)
        .await
        .map_err(internal_error)?;

    let created = load_agent_list_item(&state, agent.id.as_str()).await?;
    Ok(Json(created))
}

pub async fn update_agent_account(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<UpdateAgentAccountRequest>,
) -> ApiResult<AgentAccountListItem> {
    let Some(mut agent) = state
        .store
        .find_agent_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("agent account not found"));
    };
    ensure_agent_manager(&principal, agent.owner_user_id.as_str())?;

    if let Some(display_name) = input.display_name.as_deref() {
        agent.display_name = normalize_display_name(Some(display_name), agent.username.as_str());
    }
    if let Some(enabled) = input.enabled {
        agent.enabled = enabled;
    }
    if let Some(owner_user_id) = input.owner_user_id.as_deref() {
        if !principal.is_super_admin() {
            return Err(forbidden("only super_admin can reassign agent owner"));
        }
        let Some(owner) = state
            .store
            .find_user_by_id(owner_user_id)
            .await
            .map_err(internal_error)?
        else {
            return Err(not_found("target owner user not found"));
        };
        if !owner.enabled {
            return Err(bad_request("target owner user is disabled"));
        }
        agent.owner_user_id = owner.id;
    }

    agent.updated_at = now_rfc3339();
    state
        .store
        .update_agent_record(&agent)
        .await
        .map_err(internal_error)?;

    let updated = load_agent_list_item(&state, agent.id.as_str()).await?;
    Ok(Json(updated))
}

pub async fn reset_agent_password(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(input): Json<ResetAgentPasswordRequest>,
) -> ApiStatusResult {
    let Some(mut agent) = state
        .store
        .find_agent_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("agent account not found"));
    };
    ensure_agent_manager(&principal, agent.owner_user_id.as_str())?;

    agent.password_hash = hash_password(input.password.as_str()).map_err(bad_request)?;
    agent.updated_at = now_rfc3339();
    state
        .store
        .update_agent_record(&agent)
        .await
        .map_err(internal_error)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn load_agent_list_item(
    state: &AppState,
    id: &str,
) -> Result<AgentAccountListItem, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let items = state
        .store
        .list_agent_accounts()
        .await
        .map_err(internal_error)?;
    items
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| internal_error("agent account view missing"))
}

fn ensure_agent_manager(
    principal: &CurrentPrincipal,
    owner_user_id: &str,
) -> Result<(), (axum::http::StatusCode, Json<serde_json::Value>)> {
    if principal.is_super_admin() || principal.user_id.as_deref() == Some(owner_user_id) {
        Ok(())
    } else {
        Err(forbidden("cannot manage another user's agent account"))
    }
}
