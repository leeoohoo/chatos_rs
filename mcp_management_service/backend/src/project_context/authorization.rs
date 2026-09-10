// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{ClientProjectContextSnapshot, ProjectContextAuthorization};
use chatos_service_runtime::http_body::read_response_bytes_limited;

use crate::config::AppConfig;
use crate::trace_context::InternalTraceContextExt;

/// Authorizes a supplied client snapshot, never looks up or reconstructs a project entity.
pub async fn authorize_client_project_context(
    config: &AppConfig,
    owner_user_id: &str,
    snapshot: &ClientProjectContextSnapshot,
) -> Result<ProjectContextAuthorization, String> {
    snapshot.validate_for_owner(owner_user_id)?;
    let secret = config
        .local_connector_internal_api_secret
        .as_deref()
        .ok_or_else(|| {
            "Local Connector project authorization secret is not configured".to_string()
        })?;
    let token = chatos_service_runtime::issue_internal_service_token_for_owner(
        secret,
        "mcp-management-service",
        "local-connector-service",
        "project-context.authorize",
        60,
        owner_user_id,
    )?;
    let url = format!(
        "{}/api/local-connectors/project-context/authorize",
        config
            .local_connector_service_base_url
            .trim_end_matches('/')
    );
    let response = config
        .local_connector_http_client
        .post(url)
        .header("x-local-connector-caller", "mcp-management-service")
        .header("x-local-connector-internal-token", token)
        .header("x-local-connector-owner-user-id", owner_user_id)
        .with_internal_trace_context()
        .timeout(config.downstream_request_timeout)
        .json(snapshot)
        .send()
        .await
        .map_err(|error| format!("Local Connector project authorization failed: {error}"))?;
    let status = response.status();
    let body = read_response_bytes_limited(response, 16 * 1024)
        .await
        .map_err(|error| format!("read Local Connector project authorization failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "Local Connector rejected project authorization: {}",
            status.as_u16()
        ));
    }
    let authorization: ProjectContextAuthorization =
        serde_json::from_slice(&body).map_err(|error| {
            format!("invalid Local Connector project authorization response: {error}")
        })?;
    authorization.validate_expected(owner_user_id, snapshot)?;
    Ok(authorization)
}
