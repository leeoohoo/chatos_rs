// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, StatusCode};

use crate::config::AppConfig;

pub(super) const TASK_RUNNER_TOKEN_AUDIENCE: &str = "task-runner";
pub(super) const CHATOS_MESSAGES_READ_SCOPE: &str = "chatos.messages.read";
pub(super) const CHATOS_EXECUTION_START_SCOPE: &str = "chatos.execution.start";
pub(super) const EXECUTION_OPTIONS_READ_SCOPE: &str = "execution-options.read";
pub(super) const SYSTEM_STATS_READ_SCOPE: &str = "system.stats.read";
pub(super) const CHATOS_CALLER: &str = "chatos-backend";
pub(super) const PROJECT_SERVICE_CALLER: &str = "project-service";
pub(super) const MCP_MANAGEMENT_CALLER: &str = "mcp-management-service";
pub(super) const USER_SERVICE_CALLER: &str = "user-service";
pub(super) const MCP_TOOLS_LIST_SCOPE: &str = "mcp.tools.list";
pub(super) const MCP_TOOLS_CALL_SCOPE: &str = "mcp.tools.call";
pub(super) const MODEL_CONFIGS_SYNC_SCOPE: &str = "model-configs.sync";
pub(super) const PROJECTS_SYNC_SCOPE: &str = "projects.sync";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TaskRunnerInternalRequestIdentity {
    pub caller_service: String,
    pub scope: String,
    pub trace_id: String,
}

pub(super) struct TaskRunnerInternalAuditGuard {
    event: chatos_service_runtime::InternalResourceAccessAudit,
    recorded: bool,
}

impl TaskRunnerInternalAuditGuard {
    pub fn new(
        identity: &TaskRunnerInternalRequestIdentity,
        project_id: Option<&str>,
        resource_type: &str,
        resource_id: &str,
        action: &str,
    ) -> Self {
        Self {
            event: chatos_service_runtime::InternalResourceAccessAudit {
                caller_service: identity.caller_service.clone(),
                audience_service: TASK_RUNNER_TOKEN_AUDIENCE.to_string(),
                scope: identity.scope.clone(),
                trace_id: identity.trace_id.clone(),
                represented_user_id: None,
                tenant_id: None,
                project_id: normalized_optional(project_id),
                resource_type: resource_type.to_string(),
                resource_id: resource_id.to_string(),
                resource_name: None,
                action: action.to_string(),
                outcome: "failed".to_string(),
            },
            recorded: false,
        }
    }

    pub fn represented_user_id(&mut self, value: Option<&str>) {
        self.event.represented_user_id = normalized_optional(value);
    }

    pub fn tenant_id(&mut self, value: Option<&str>) {
        self.event.tenant_id = normalized_optional(value);
    }

    pub fn project_id(&mut self, value: Option<&str>) {
        self.event.project_id = normalized_optional(value);
    }

    pub fn resource_name(&mut self, value: Option<&str>) {
        self.event.resource_name = normalized_optional(value);
    }

    pub fn succeeded(mut self) {
        self.event.outcome = "succeeded".to_string();
        self.record();
    }

    fn record(&mut self) {
        if self.recorded {
            return;
        }
        if let Err(error) = chatos_service_runtime::record_internal_resource_access(&self.event) {
            tracing::error!(
                target: "chatos_internal_audit",
                trace_id = self.event.trace_id.as_str(),
                error = error.as_str(),
                "task runner internal resource audit validation failed"
            );
        }
        self.recorded = true;
    }
}

impl Drop for TaskRunnerInternalAuditGuard {
    fn drop(&mut self) {
        self.record();
    }
}

#[derive(Debug)]
pub(super) struct InternalAuthError {
    pub status: StatusCode,
    pub message: String,
}

pub(super) fn require_task_runner_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    allowed_callers: &[&str],
    required_scope: &str,
) -> Result<TaskRunnerInternalRequestIdentity, InternalAuthError> {
    let token = header_text(headers, "x-task-runner-internal-token");
    let caller = match header_text(headers, "x-task-runner-caller") {
        Some(caller) => caller,
        None if token.is_some() => {
            return Err(bad_request(
                "task runner caller is required for signed internal requests",
            ));
        }
        None => {
            return Err(unauthorized(
                "signed task runner internal API token is required",
            ));
        }
    };
    if !allowed_callers.contains(&caller) {
        return Err(forbidden(
            "caller service is not allowed for this task runner operation",
        ));
    }
    let expected = caller_secret(config, caller)
        .ok_or_else(|| unauthorized("task runner internal API is disabled for caller"))?;
    let token =
        token.ok_or_else(|| unauthorized("signed task runner internal API token is required"))?;
    let claims = chatos_service_runtime::verify_internal_service_token(
        token,
        expected,
        caller,
        TASK_RUNNER_TOKEN_AUDIENCE,
        required_scope,
    )
    .map_err(|_| unauthorized("invalid task runner internal API token"))?;
    Ok(TaskRunnerInternalRequestIdentity {
        caller_service: caller.to_string(),
        scope: required_scope.to_string(),
        trace_id: claims.trace_id,
    })
}

fn caller_secret<'a>(config: &'a AppConfig, caller: &str) -> Option<&'a str> {
    let value = match caller {
        CHATOS_CALLER => config.chatos_internal_api_secret.as_deref(),
        PROJECT_SERVICE_CALLER => config.internal_api_secret.as_deref(),
        MCP_MANAGEMENT_CALLER => config.mcp_management_internal_api_secret.as_deref(),
        USER_SERVICE_CALLER => config.user_service_internal_api_secret.as_deref(),
        _ => None,
    };
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn header_text<'a>(headers: &'a HeaderMap, key: &'static str) -> Option<&'a str> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn unauthorized(message: impl Into<String>) -> InternalAuthError {
    InternalAuthError {
        status: StatusCode::UNAUTHORIZED,
        message: message.into(),
    }
}

fn forbidden(message: impl Into<String>) -> InternalAuthError {
    InternalAuthError {
        status: StatusCode::FORBIDDEN,
        message: message.into(),
    }
}

fn bad_request(message: impl Into<String>) -> InternalAuthError {
    InternalAuthError {
        status: StatusCode::BAD_REQUEST,
        message: message.into(),
    }
}
