use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use super::state::ensure_state_files;
use super::types::SubAgentRouterMcpPermissions;

pub(super) fn load_mcp_permissions() -> Result<Value, String> {
    let paths = ensure_state_files()?;
    let state = read_mcp_permissions_state(paths.mcp_permissions_path.as_path())?;
    Ok(json!({
        "configured": state.configured,
        "enabled_mcp_ids": state.enabled_mcp_ids,
        "enabled_tool_prefixes": state.enabled_tool_prefixes,
        "selected_system_context_id": state.selected_system_context_id,
        "updated_at": state.updated_at,
        "path": paths.mcp_permissions_path.to_string_lossy().to_string()
    }))
}

pub(super) fn save_mcp_permissions(
    enabled_mcp_ids: &[String],
    enabled_tool_prefixes: &[String],
    selected_system_context_id: Option<&str>,
) -> Result<Value, String> {
    let paths = ensure_state_files()?;
    let previous_state = read_mcp_permissions_state(paths.mcp_permissions_path.as_path())
        .unwrap_or_else(|_| SubAgentRouterMcpPermissions::default());

    let mut ids = enabled_mcp_ids
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();

    let mut prefixes = enabled_tool_prefixes
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    prefixes.sort();
    prefixes.dedup();

    let state = SubAgentRouterMcpPermissions {
        configured: true,
        enabled_mcp_ids: ids,
        enabled_tool_prefixes: prefixes,
        selected_system_context_id: match selected_system_context_id {
            Some(value) => normalize_optional_string(Some(value)),
            None => previous_state.selected_system_context_id,
        },
        updated_at: crate::core::time::now_rfc3339(),
    };

    let text = serde_json::to_string_pretty(&state).map_err(|err| err.to_string())?;
    fs::write(paths.mcp_permissions_path.as_path(), text).map_err(|err| err.to_string())?;

    Ok(json!({
        "ok": true,
        "configured": state.configured,
        "enabled_mcp_ids": state.enabled_mcp_ids,
        "enabled_tool_prefixes": state.enabled_tool_prefixes,
        "selected_system_context_id": state.selected_system_context_id,
        "updated_at": state.updated_at,
        "path": paths.mcp_permissions_path.to_string_lossy().to_string()
    }))
}

fn read_mcp_permissions_state(path: &Path) -> Result<SubAgentRouterMcpPermissions, String> {
    let raw = fs::read_to_string(path).unwrap_or_default();
    if raw.trim().is_empty() {
        return Ok(SubAgentRouterMcpPermissions::default());
    }

    serde_json::from_str::<SubAgentRouterMcpPermissions>(raw.as_str()).or_else(|_| {
        let value = serde_json::from_str::<Value>(raw.as_str()).map_err(|err| err.to_string())?;
        let configured = value
            .get("configured")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut enabled_mcp_ids = value
            .get("enabled_mcp_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| entry.as_str())
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        enabled_mcp_ids.sort();
        enabled_mcp_ids.dedup();

        let mut enabled_tool_prefixes = value
            .get("enabled_tool_prefixes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| entry.as_str())
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        enabled_tool_prefixes.sort();
        enabled_tool_prefixes.dedup();

        let selected_system_context_id = normalize_optional_string(
            value
                .get("selected_system_context_id")
                .and_then(|v| v.as_str()),
        );

        let updated_at = value
            .get("updated_at")
            .and_then(|v| v.as_str())
            .map(|v| v.trim().to_string())
            .unwrap_or_default();

        Ok(SubAgentRouterMcpPermissions {
            configured,
            enabled_mcp_ids,
            enabled_tool_prefixes,
            selected_system_context_id,
            updated_at,
        })
    })
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}
