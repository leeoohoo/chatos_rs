// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use chatos_mcp_service::{JsonRpcResponse, MCP_ERROR_INTERNAL};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChatosInternalRequestIdentity {
    pub caller_service: String,
    pub audience_service: String,
    pub scope: String,
    pub trace_id: String,
}

pub(crate) struct ChatosInternalResourceAudit<'a> {
    pub represented_user_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub resource_type: &'a str,
    pub resource_id: &'a str,
    pub resource_name: Option<&'a str>,
    pub action: &'a str,
    pub outcome: &'a str,
}

pub(crate) fn record_chatos_internal_resource_access(
    identity: &ChatosInternalRequestIdentity,
    access: ChatosInternalResourceAudit<'_>,
) {
    let event = build_audit_event(identity, access);
    if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
        tracing::error!(
            target: "chatos_internal_audit",
            trace_id = identity.trace_id.as_str(),
            error = error.as_str(),
            "ChatOS internal resource audit validation failed"
        );
    }
}

pub(crate) fn http_outcome(status: StatusCode) -> &'static str {
    if status.is_success() {
        "accepted"
    } else if status.is_server_error() {
        "failed"
    } else {
        "rejected"
    }
}

pub(crate) fn jsonrpc_outcome(response: &JsonRpcResponse) -> &'static str {
    match response.error.as_ref() {
        None => "accepted",
        Some(error) if error.code == MCP_ERROR_INTERNAL => "failed",
        Some(_) => "rejected",
    }
}

fn build_audit_event(
    identity: &ChatosInternalRequestIdentity,
    access: ChatosInternalResourceAudit<'_>,
) -> chatos_service_runtime::InternalResourceAccessAudit {
    chatos_service_runtime::InternalResourceAccessAudit {
        caller_service: identity.caller_service.clone(),
        audience_service: identity.audience_service.clone(),
        scope: identity.scope.clone(),
        trace_id: identity.trace_id.clone(),
        represented_user_id: normalized_optional(access.represented_user_id),
        tenant_id: None,
        project_id: normalized_optional(access.project_id),
        resource_type: access.resource_type.to_string(),
        resource_id: access.resource_id.to_string(),
        resource_name: normalized_optional(access.resource_name),
        action: access.action.to_string(),
        outcome: access.outcome.to_string(),
    }
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_mcp_service::{jsonrpc_error, jsonrpc_ok, MCP_ERROR_AUTH_REQUIRED};
    use serde_json::{json, Value};
    use uuid::Uuid;

    fn identity() -> ChatosInternalRequestIdentity {
        ChatosInternalRequestIdentity {
            caller_service: "task-runner".to_string(),
            audience_service: "chatos-backend".to_string(),
            scope: "task-runner.callback".to_string(),
            trace_id: Uuid::new_v4().to_string(),
        }
    }

    #[test]
    fn internal_audit_event_preserves_verified_identity_and_resource_scope() {
        let identity = identity();
        let event = build_audit_event(
            &identity,
            ChatosInternalResourceAudit {
                represented_user_id: Some("user-1"),
                project_id: Some("project-1"),
                resource_type: "task_runner_task",
                resource_id: "task-1",
                resource_name: Some("task.completed"),
                action: "callback",
                outcome: "accepted",
            },
        );

        assert!(event.validate().is_ok());
        assert_eq!(event.trace_id, identity.trace_id);
        assert_eq!(event.represented_user_id.as_deref(), Some("user-1"));
        assert_eq!(event.project_id.as_deref(), Some("project-1"));
    }

    #[test]
    fn outcomes_distinguish_business_rejection_from_internal_failure() {
        assert_eq!(http_outcome(StatusCode::OK), "accepted");
        assert_eq!(http_outcome(StatusCode::BAD_REQUEST), "rejected");
        assert_eq!(http_outcome(StatusCode::INTERNAL_SERVER_ERROR), "failed");
        assert_eq!(
            jsonrpc_outcome(&jsonrpc_ok(Value::Null, json!({}))),
            "accepted"
        );
        assert_eq!(
            jsonrpc_outcome(&jsonrpc_error(
                Value::Null,
                MCP_ERROR_AUTH_REQUIRED,
                "denied"
            )),
            "rejected"
        );
        assert_eq!(
            jsonrpc_outcome(&jsonrpc_error(Value::Null, MCP_ERROR_INTERNAL, "failed")),
            "failed"
        );
    }
}
