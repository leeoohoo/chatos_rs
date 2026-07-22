// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};
use chatos_plugin_management_sdk::{
    ResolveAgentCapabilitiesRequest, ResolvedAgentCapabilities, SystemAgentKey,
};

use crate::models::CurrentUser;
use crate::state::AppState;

use super::ApiError;

pub(super) async fn resolve_local_runtime_capabilities(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(agent_key): Path<String>,
) -> Result<Json<ResolvedAgentCapabilities>, ApiError> {
    let agent_key = local_runtime_agent_key(agent_key.as_str())
        .ok_or_else(|| ApiError::not_found("Local runtime agent capability was not found"))?;
    let owner_user_id = user.effective_owner_user_id();
    let request = ResolveAgentCapabilitiesRequest::new(agent_key, owner_user_id)
        .with_runtime_context(None, None, Some("local_connector".to_string()), None);
    let capabilities = state
        .plugin_management_client
        .resolve_for_service(&request)
        .await
        .map_err(|err| ApiError::service_unavailable(err.to_string()))?;
    if capabilities.owner_user_id != owner_user_id || capabilities.agent_key != agent_key.as_str() {
        return Err(ApiError::service_unavailable(
            "Plugin Management returned a mismatched capability identity",
        ));
    }
    Ok(Json(capabilities))
}

fn local_runtime_agent_key(value: &str) -> Option<SystemAgentKey> {
    let value = value.trim();
    SystemAgentKey::ALL
        .into_iter()
        .find(|agent_key| agent_key.as_str() == value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_every_system_agent_configuration_resource() {
        for agent_key in SystemAgentKey::ALL {
            assert_eq!(local_runtime_agent_key(agent_key.as_str()), Some(agent_key));
        }
        assert_eq!(local_runtime_agent_key("unknown"), None);
    }
}
