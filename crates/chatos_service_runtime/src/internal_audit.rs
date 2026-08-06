// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InternalResourceAccessAudit {
    pub caller_service: String,
    pub audience_service: String,
    pub scope: String,
    pub trace_id: String,
    pub represented_user_id: Option<String>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub resource_type: String,
    pub resource_id: String,
    pub resource_name: Option<String>,
    pub action: String,
    pub outcome: String,
}

impl InternalResourceAccessAudit {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("caller_service", self.caller_service.as_str()),
            ("audience_service", self.audience_service.as_str()),
            ("scope", self.scope.as_str()),
            ("resource_type", self.resource_type.as_str()),
            ("resource_id", self.resource_id.as_str()),
            ("action", self.action.as_str()),
            ("outcome", self.outcome.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 256 {
                return Err(format!("internal audit {name} is invalid"));
            }
        }
        Uuid::parse_str(self.trace_id.trim())
            .map_err(|_| "internal audit trace_id must be a UUID".to_string())?;
        for (name, value) in [
            ("represented_user_id", self.represented_user_id.as_deref()),
            ("tenant_id", self.tenant_id.as_deref()),
            ("project_id", self.project_id.as_deref()),
            ("resource_name", self.resource_name.as_deref()),
        ] {
            if value.is_some_and(|value| value.trim().is_empty() || value.len() > 256) {
                return Err(format!("internal audit {name} is invalid"));
            }
        }
        Ok(())
    }
}

pub fn record_internal_resource_access(event: &InternalResourceAccessAudit) -> Result<(), String> {
    event.validate()?;
    tracing::info!(
        target: "chatos_internal_audit",
        audit_event = "internal_resource_access",
        caller_service = event.caller_service.as_str(),
        audience_service = event.audience_service.as_str(),
        scope = event.scope.as_str(),
        trace_id = event.trace_id.as_str(),
        represented_user_id = event.represented_user_id.as_deref(),
        tenant_id = event.tenant_id.as_deref(),
        project_id = event.project_id.as_deref(),
        resource_type = event.resource_type.as_str(),
        resource_id = event.resource_id.as_str(),
        resource_name = event.resource_name.as_deref(),
        action = event.action.as_str(),
        outcome = event.outcome.as_str(),
        "internal service resource access audit"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> InternalResourceAccessAudit {
        InternalResourceAccessAudit {
            caller_service: "task-runner".to_string(),
            audience_service: "mcp-management-service".to_string(),
            scope: "runtime.tools.call".to_string(),
            trace_id: Uuid::new_v4().to_string(),
            represented_user_id: Some("user-1".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            project_id: Some("project-1".to_string()),
            resource_type: "mcp_tool".to_string(),
            resource_id: "resource-1/tool-1".to_string(),
            resource_name: Some("tool-1".to_string()),
            action: "call".to_string(),
            outcome: "accepted".to_string(),
        }
    }

    #[test]
    fn audit_event_requires_trace_and_resource_identity() {
        assert!(event().validate().is_ok());

        let mut invalid_trace = event();
        invalid_trace.trace_id = "not-a-trace".to_string();
        assert!(invalid_trace.validate().is_err());

        let mut missing_resource = event();
        missing_resource.resource_id.clear();
        assert!(missing_resource.validate().is_err());
    }
}
