// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;

use crate::api::types::{LocalApiError, UpdateLocalRuntimeSettingsRequest};
use crate::config::ClientConfig;
use crate::registration::disconnect_device;
use crate::{tracing_stdout, LocalRuntime};

pub(crate) async fn local_runtime_settings(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<crate::state::LocalRuntimeSettings>, LocalApiError> {
    let state = runtime.state.read().await;
    Ok(Json(state.runtime_settings.clone().normalized()))
}

pub(crate) async fn local_update_runtime_settings(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<UpdateLocalRuntimeSettingsRequest>,
) -> Result<Json<crate::state::LocalRuntimeSettings>, LocalApiError> {
    {
        let state = runtime.state.read().await;
        validate_runtime_settings_update(&req, &state.runtime_settings)?;
    }
    let mode_changed = {
        let state = runtime.state.read().await;
        req.developer_mode
            .is_some_and(|developer_mode| developer_mode != state.runtime_settings.developer_mode)
    };
    if mode_changed {
        let disconnect = {
            let state = runtime.state.read().await;
            ClientConfig::from_state(&state, runtime.state_path.clone())
                .zip(state.device_id.clone())
        };
        runtime.stop_connector().await;
        if let Some((config, device_id)) = disconnect {
            if let Err(err) =
                disconnect_device(&runtime.http_client, &config, device_id.as_str()).await
            {
                tracing_stdout(
                    format!("disconnect previous developer-mode endpoint failed: {err}").as_str(),
                );
            }
        }
    }
    let mut state = runtime.state.write().await;
    if let Some(developer_mode) = req.developer_mode {
        state.runtime_settings.developer_mode = developer_mode;
    }
    if let Some(enabled) = req.browser_full_cdp_access_enabled {
        state.runtime_settings.browser_full_cdp_access_enabled = enabled;
    }
    state.runtime_settings = state.runtime_settings.clone().normalized();
    state.save(runtime.state_path.as_path())?;
    Ok(Json(state.runtime_settings.clone()))
}

fn validate_runtime_settings_update(
    request: &UpdateLocalRuntimeSettingsRequest,
    current: &crate::state::LocalRuntimeSettings,
) -> Result<(), LocalApiError> {
    let enabling_full_cdp = request.browser_full_cdp_access_enabled == Some(true)
        && !current.browser_full_cdp_access_enabled;
    if enabling_full_cdp && !request.acknowledge_browser_full_cdp_risk {
        return Err(LocalApiError::bad_request(
            "enabling full browser CDP access requires explicit risk acknowledgement",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_runtime_settings_update;
    use crate::api::types::UpdateLocalRuntimeSettingsRequest;
    use crate::state::LocalRuntimeSettings;

    #[test]
    fn full_cdp_access_requires_one_explicit_enable_acknowledgement() {
        let current = LocalRuntimeSettings::default();
        let denied = UpdateLocalRuntimeSettingsRequest {
            developer_mode: None,
            browser_full_cdp_access_enabled: Some(true),
            acknowledge_browser_full_cdp_risk: false,
        };
        assert!(validate_runtime_settings_update(&denied, &current).is_err());

        let allowed = UpdateLocalRuntimeSettingsRequest {
            acknowledge_browser_full_cdp_risk: true,
            ..denied
        };
        assert!(validate_runtime_settings_update(&allowed, &current).is_ok());

        let already_enabled = LocalRuntimeSettings {
            browser_full_cdp_access_enabled: true,
            ..LocalRuntimeSettings::default()
        };
        assert!(validate_runtime_settings_update(
            &UpdateLocalRuntimeSettingsRequest {
                developer_mode: None,
                browser_full_cdp_access_enabled: Some(true),
                acknowledge_browser_full_cdp_risk: false,
            },
            &already_enabled,
        )
        .is_ok());
    }
}
