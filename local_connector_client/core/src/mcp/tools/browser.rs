// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex as StdMutex, OnceLock};

use anyhow::{anyhow, Result};
use chatos_builtin_tools::{BrowserToolsOptions, BrowserToolsService};

use crate::relay::RelayRequest;
use crate::terminal::controller::local_mcp_terminal_project_id;
use crate::workspace::paths::canonicalize_existing_dir;

pub(crate) fn local_browser_tools_service_for_root(
    root: &Path,
    request: &RelayRequest,
) -> Result<BrowserToolsService> {
    let root = canonicalize_existing_dir(root)?;
    let key = local_browser_tools_registry_key(root.as_path(), request);
    {
        let registry = local_browser_tools_registry()
            .services
            .lock()
            .map_err(|_| anyhow!("local browser tools registry is poisoned"))?;
        if let Some(service) = registry.get(key.as_str()) {
            return Ok(service.clone());
        }
    }
    let service = BrowserToolsService::new(BrowserToolsOptions {
        server_name: "local_connector_browser_tools".to_string(),
        workspace_dir: root,
        command_timeout_seconds: 30,
        max_snapshot_chars: 8_000,
        vision_adapter: None,
    })
    .map_err(|err| anyhow!(err))?;
    let mut registry = local_browser_tools_registry()
        .services
        .lock()
        .map_err(|_| anyhow!("local browser tools registry is poisoned"))?;
    Ok(registry
        .entry(key)
        .or_insert_with(|| service.clone())
        .clone())
}

pub(crate) fn local_browser_conversation_id(request: &RelayRequest) -> String {
    local_mcp_terminal_project_id(request).unwrap_or_else(|| request.workspace_id.clone())
}

#[derive(Default)]
struct LocalBrowserToolsRegistry {
    services: StdMutex<HashMap<String, BrowserToolsService>>,
}

fn local_browser_tools_registry() -> &'static LocalBrowserToolsRegistry {
    static REGISTRY: OnceLock<LocalBrowserToolsRegistry> = OnceLock::new();
    REGISTRY.get_or_init(LocalBrowserToolsRegistry::default)
}

fn local_browser_tools_registry_key(root: &Path, request: &RelayRequest) -> String {
    let user = request
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("anonymous");
    let project =
        local_mcp_terminal_project_id(request).unwrap_or_else(|| request.workspace_id.clone());
    format!(
        "{}|{}|{}",
        user,
        project,
        root.to_string_lossy().replace('\\', "/")
    )
}
