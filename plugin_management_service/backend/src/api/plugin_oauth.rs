// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use super::*;

pub(super) async fn list_plugin_oauth_connections(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(plugin_id): Path<String>,
    Query(query): Query<PluginOAuthQuery>,
) -> Result<Json<ListResponse<PluginOAuthConnectionRecord>>, ApiError> {
    let device_id = required_text(query.device_id.as_deref(), "device_id")?;
    let plugin = state
        .store
        .get_plugin_catalog_entry(plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin not found"))?;
    ensure_catalog_visible(&user, &plugin)?;
    let items = state
        .store
        .list_plugin_oauth_connections(
            user.effective_owner_user_id(),
            device_id.as_str(),
            plugin.id.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(ListResponse {
        total: items.len() as u64,
        items,
    }))
}

pub(super) async fn sync_plugin_oauth_status_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut payload): Json<PluginOAuthStatusSyncPayload>,
) -> Result<Json<PluginOAuthConnectionRecord>, ApiError> {
    require_local_connector_internal_request(&state, &headers, PLUGIN_OAUTH_MANAGE_SCOPE)?;
    normalize_oauth_payload(&mut payload)?;
    let plugin = state
        .store
        .get_plugin_catalog_entry(payload.plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("Plugin not found"))?;
    let release = state
        .store
        .get_plugin_release(payload.release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("Plugin release not found"))?;
    validate_oauth_component(&payload, &plugin, &release)?;
    let existing = state
        .store
        .get_plugin_oauth_connection(
            payload.owner_user_id.as_str(),
            payload.device_id.as_str(),
            payload.plugin_id.as_str(),
            payload.component_key.as_str(),
            payload.provider.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    let record = PluginOAuthConnectionRecord {
        id: existing
            .as_ref()
            .map(|record| record.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        owner_user_id: payload.owner_user_id.clone(),
        device_id: payload.device_id.clone(),
        plugin_id: payload.plugin_id.clone(),
        release_id: payload.release_id,
        component_key: payload.component_key,
        provider: payload.provider,
        scopes: payload.scopes,
        connected: payload.connected,
        expires_at: payload.expires_at,
        account_display: payload.account_display,
        updated_at: now_rfc3339(),
    };
    state
        .store
        .replace_plugin_oauth_connection(&record)
        .await
        .map_err(ApiError::internal)?;
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_SYNC_OAUTH,
        record.owner_user_id.as_str(),
        Some(record.device_id.as_str()),
        record.plugin_id.as_str(),
        Some(record.release_id.as_str()),
        "success",
        BTreeMap::from([
            ("provider".to_string(), json!(record.provider)),
            ("connected".to_string(), json!(record.connected)),
        ]),
    );
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

fn normalize_oauth_payload(payload: &mut PluginOAuthStatusSyncPayload) -> Result<(), ApiError> {
    payload.owner_user_id = required_text(Some(payload.owner_user_id.as_str()), "owner_user_id")?;
    payload.device_id = required_text(Some(payload.device_id.as_str()), "device_id")?;
    payload.plugin_id = required_text(Some(payload.plugin_id.as_str()), "plugin_id")?;
    payload.release_id = required_text(Some(payload.release_id.as_str()), "release_id")?;
    payload.component_key = required_text(Some(payload.component_key.as_str()), "component_key")?;
    payload.provider = normalize_provider(payload.provider.as_str())?;
    payload.scopes = normalize_string_list(std::mem::take(&mut payload.scopes));
    payload.expires_at = payload
        .expires_at
        .as_deref()
        .and_then(|value| normalized(Some(value)));
    payload.account_display = payload
        .account_display
        .as_deref()
        .and_then(|value| normalized(Some(value)))
        .map(|value| truncate_text(value.as_str(), 200));
    if !payload.connected {
        payload.expires_at = None;
        payload.account_display = None;
    }
    Ok(())
}

fn normalize_provider(value: &str) -> Result<String, ApiError> {
    let provider = required_text(Some(value), "provider")?.to_ascii_lowercase();
    let valid = provider.len() <= 96
        && provider.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        });
    if valid {
        Ok(provider)
    } else {
        Err(ApiError::bad_request(
            "provider contains unsupported characters",
        ))
    }
}

fn validate_oauth_component(
    payload: &PluginOAuthStatusSyncPayload,
    plugin: &PluginCatalogRecord,
    release: &PluginReleaseRecord,
) -> Result<(), ApiError> {
    if payload.plugin_id != plugin.id || release.plugin_id != plugin.id {
        return Err(ApiError::bad_request(
            "OAuth connection identity does not match Plugin release",
        ));
    }
    if payload.connected && release.revoked_at.is_some() {
        return Err(ApiError::conflict(
            "revoked Plugin release cannot establish OAuth connections",
        ));
    }
    let component = release
        .components
        .iter()
        .find(|component| component.component_key == payload.component_key)
        .ok_or_else(|| ApiError::bad_request("OAuth component not found in Plugin release"))?;
    if !matches!(
        component.kind,
        PluginComponentKind::ConnectedApp | PluginComponentKind::McpServer
    ) {
        return Err(ApiError::bad_request(
            "OAuth status is only valid for App or MCP components",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnected_oauth_status_drops_account_metadata() {
        let mut payload = PluginOAuthStatusSyncPayload {
            owner_user_id: "user".to_string(),
            device_id: "device".to_string(),
            plugin_id: "plugin".to_string(),
            release_id: "release".to_string(),
            component_key: "app".to_string(),
            provider: "Figma".to_string(),
            scopes: vec!["files:read".to_string()],
            connected: false,
            expires_at: Some("later".to_string()),
            account_display: Some("Secret Account".to_string()),
        };
        normalize_oauth_payload(&mut payload).expect("valid status");
        assert_eq!(payload.provider, "figma");
        assert!(payload.expires_at.is_none());
        assert!(payload.account_display.is_none());
    }
}
