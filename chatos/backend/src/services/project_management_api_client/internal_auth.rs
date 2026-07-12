// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

const PROJECT_SERVICE_CALLER: &str = "chatos-backend";
const PROJECT_SERVICE_TOKEN_AUDIENCE: &str = "project-service";
pub(super) const PROJECT_READ_SCOPE: &str = "project.read";
pub(super) const PROJECT_SYNC_SCOPE: &str = "project.sync";
pub(crate) const PROJECT_MCP_SCOPE: &str = "project.mcp";
pub(super) const PROJECT_HARNESS_SCOPE: &str = "project.harness";

pub(super) fn signed_project_service_request(
    request: reqwest::RequestBuilder,
    internal_secret: &str,
    scope: &str,
) -> Result<reqwest::RequestBuilder, String> {
    let token = chatos_service_runtime::issue_internal_service_token(
        internal_secret.trim(),
        PROJECT_SERVICE_CALLER,
        PROJECT_SERVICE_TOKEN_AUDIENCE,
        scope,
        60,
    )?;
    Ok(request
        .header("X-Project-Service-Caller", PROJECT_SERVICE_CALLER)
        .header("X-Project-Service-Internal-Token", token))
}

pub(crate) fn insert_project_service_internal_headers(
    headers: &mut HashMap<String, String>,
    internal_secret: &str,
    scope: &str,
) -> Result<(), String> {
    let internal_secret = internal_secret.trim();
    if internal_secret.is_empty() || scope.trim().is_empty() {
        return Err("project service internal secret and scope are required".to_string());
    }
    headers.insert(
        "X-Project-Service-Sync-Secret".to_string(),
        internal_secret.to_string(),
    );
    headers.insert(
        "X-Project-Service-Caller".to_string(),
        PROJECT_SERVICE_CALLER.to_string(),
    );
    headers.insert(
        "X-Project-Service-Internal-Scope".to_string(),
        scope.trim().to_string(),
    );
    Ok(())
}
