// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const PROJECT_SERVICE_CALLER: &str = "chatos-backend";
const PROJECT_SERVICE_TOKEN_AUDIENCE: &str = "project-service";
pub(super) const PROJECT_READ_SCOPE: &str = "project.read";
pub(super) const PROJECT_SYNC_SCOPE: &str = "project.sync";
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
