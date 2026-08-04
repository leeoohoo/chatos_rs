// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp::{system_mcp_catalog, SystemMcpBackend};
use chatos_mcp_management_sdk::{McpCatalogItem, McpCatalogResponse};

use crate::auth::require_internal_request;
use crate::error::ApiError;
use crate::state::AppState;

pub(super) async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<McpCatalogResponse>, ApiError> {
    require_internal_request(&state.config, &headers, "catalog.read")?;
    let items = system_mcp_catalog()
        .iter()
        .map(|descriptor| McpCatalogItem {
            resource_id: descriptor.resource_id.to_string(),
            system_key: descriptor.key.as_str().to_string(),
            server_name: descriptor.server_name.to_string(),
            display_name: descriptor.display_name.to_string(),
            description: descriptor.description.to_string(),
            owner_service: descriptor.owner_service.to_string(),
            backend: backend_name(descriptor.backend).to_string(),
            allow_writes: descriptor.allow_writes,
            tags: descriptor
                .tags
                .iter()
                .map(|value| value.to_string())
                .collect(),
            category: descriptor.category.map(ToOwned::to_owned),
        })
        .collect::<Vec<_>>();
    let total = items.len();
    Ok(Json(McpCatalogResponse {
        service: "mcp-management-service".to_string(),
        items,
        total,
    }))
}

fn backend_name(backend: SystemMcpBackend) -> &'static str {
    match backend {
        SystemMcpBackend::Embedded => "embedded",
        SystemMcpBackend::RunScopedBuiltin => "run_scoped_builtin",
        SystemMcpBackend::ServiceHttp => "service_http",
        SystemMcpBackend::ServiceDynamic => "service_dynamic",
        SystemMcpBackend::HostAdapter => "host_adapter",
    }
}
