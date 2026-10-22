use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager, State};

use crate::{
    authentication::{runtime_api_base_url, AuthenticationManager},
    connector_state,
    host_lifecycle::HostManager,
};

const MAXIMUM_PRESENTED_TEXT_BYTES: usize = 1_024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceGatewaySnapshot {
    configured: bool,
    connector_running: bool,
    device_name: String,
    device_id: String,
    cloud_base_url: String,
    workspace_count: usize,
    runtime: &'static str,
    developer_mode: bool,
    connection_mode: &'static str,
    device_signed_relay_available: bool,
}

#[tauri::command]
pub async fn load_device_gateway(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    host: State<'_, HostManager>,
    owner_user_id: String,
) -> Result<DeviceGatewaySnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let data_directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法读取本机连接器状态。".to_owned())?;
    let state_owner = owner_user_id.clone();
    let state = tauri::async_runtime::spawn_blocking(move || {
        connector_state::ensure_owner_state(&data_directory, &state_owner)?;
        connector_state::read_owner_state(&data_directory, &state_owner)
    })
    .await
    .map_err(|_| "本机连接器状态任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let connector_running = host.is_running_for(&owner_user_id).await;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    project_snapshot(state, connector_running)
}

fn project_snapshot(
    state: Map<String, Value>,
    connector_running: bool,
) -> Result<DeviceGatewaySnapshot, String> {
    let device_id = state
        .get("deviceID")
        .and_then(Value::as_str)
        .ok_or_else(status_error)?;
    let device_id = required_identifier("device_id", device_id.to_owned())?;
    let workspace_count = state
        .get("workspaces")
        .and_then(Value::as_array)
        .ok_or_else(status_error)?
        .len();
    let cloud_base_url = safe_gateway_url()?;
    Ok(DeviceGatewaySnapshot {
        configured: true,
        connector_running,
        device_name: safe_device_name(),
        device_id,
        cloud_base_url,
        workspace_count,
        runtime: "Rust Local Agent Host",
        developer_mode: cfg!(debug_assertions),
        connection_mode: "authenticated_control_plane",
        // macOS currently owns the device-signed long-lived relay implementation. Windows uses
        // its authenticated control-plane session and never claims that relay capability.
        device_signed_relay_available: false,
    })
}

fn safe_gateway_url() -> Result<String, String> {
    let url = reqwest::Url::parse(&runtime_api_base_url()).map_err(|_| status_error())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(status_error());
    }
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

fn safe_device_name() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .and_then(|value| {
            let value = value.trim();
            (!value.is_empty()
                && value.len() <= MAXIMUM_PRESENTED_TEXT_BYTES
                && !value.chars().any(char::is_control))
            .then(|| value.to_owned())
        })
        .unwrap_or_else(|| "Windows PC".into())
}

fn required_identifier(field: &str, value: String) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAXIMUM_PRESENTED_TEXT_BYTES
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
    {
        return Err(format!("{field} is invalid"));
    }
    Ok(value.to_owned())
}

fn status_error() -> String {
    "无法读取本机连接器状态。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn safe_projection_only_publishes_workspace_count() {
        let mut state = Map::new();
        state.insert("deviceID".into(), json!("windows-device-1"));
        state.insert(
            "workspaces".into(),
            json!([{
                "id": "workspace-1",
                "absoluteRoot": "C:\\private\\project",
                "fingerprint": "secret-fingerprint"
            }]),
        );
        let snapshot = project_snapshot(state, true).unwrap();
        let encoded = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(snapshot.workspace_count, 1);
        assert!(!encoded.contains("private"));
        assert!(!encoded.contains("fingerprint"));
        assert!(snapshot.connector_running);
        assert!(!snapshot.device_signed_relay_available);
    }

    #[test]
    fn malformed_device_identity_fails_closed() {
        let mut state = Map::new();
        state.insert("deviceID".into(), json!("bad device"));
        state.insert("workspaces".into(), json!([]));
        assert!(project_snapshot(state, false).is_err());
    }
}
