// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chatos_service_runtime::InternalServiceTokenClaims;

use super::internal_auth::TOKEN_AUDIENCE;

const AUDIT_TEXT_LIMIT_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryInternalRequestAudit {
    identity: InternalServiceTokenClaims,
    resource_type: String,
    resource_id: String,
    resource_name: Option<String>,
    action: String,
}

impl MemoryInternalRequestAudit {
    pub(crate) fn from_request(
        request: &Request<Body>,
        identity: &InternalServiceTokenClaims,
    ) -> Self {
        let resource = classify_resource(request.uri().path());
        Self {
            identity: identity.clone(),
            resource_type: resource.resource_type.to_string(),
            resource_id: bounded_audit_text(resource.resource_id.as_str()),
            resource_name: resource.resource_name.map(bounded_audit_text),
            action: request_action(request.method(), request.uri().path()),
        }
    }

    pub(crate) fn record(&self, status: StatusCode) {
        let event = self.event(status);
        if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
            tracing::error!(
                target: "chatos_internal_audit",
                trace_id = self.identity.trace_id.as_str(),
                error = error.as_str(),
                "Memory Engine internal resource audit validation failed"
            );
        }
    }

    fn event(&self, status: StatusCode) -> chatos_service_runtime::InternalResourceAccessAudit {
        chatos_service_runtime::InternalResourceAccessAudit {
            caller_service: self.identity.caller.clone(),
            audience_service: TOKEN_AUDIENCE.to_string(),
            scope: self.identity.scope.clone(),
            trace_id: self.identity.trace_id.clone(),
            represented_user_id: None,
            tenant_id: None,
            project_id: None,
            resource_type: self.resource_type.clone(),
            resource_id: self.resource_id.clone(),
            resource_name: self.resource_name.clone(),
            action: self.action.clone(),
            outcome: response_outcome(status).to_string(),
        }
    }
}

struct ClassifiedResource<'a> {
    resource_type: &'static str,
    resource_id: String,
    resource_name: Option<&'a str>,
}

fn classify_resource(path: &str) -> ClassifiedResource<'_> {
    if path == "/api/internal/system/stats" {
        return ClassifiedResource {
            resource_type: "memory_system",
            resource_id: "system/stats".to_string(),
            resource_name: Some("stats"),
        };
    }
    let relative = path
        .strip_prefix("/api/memory-engine/v1/")
        .unwrap_or(path)
        .trim_matches('/');
    let segments = relative
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let resource_type = match segments.as_slice() {
        ["admin", "model-profiles", ..] => "memory_model_profile",
        ["admin", "sources", ..] | ["sources", ..] => "memory_source",
        ["admin", "job-policies", ..] => "memory_job_policy",
        ["admin", "job-runs", ..] => "memory_job_run",
        ["admin", "dashboard", ..] => "memory_admin_dashboard",
        ["jobs", ..] => "memory_job",
        ["queue-operations", ..] => "memory_queue_operation",
        ["subjects", _, "memories", ..] => "memory_subject_memory",
        ["subjects", ..] => "memory_subject",
        ["subject-memory-scopes", ..] => "memory_subject_memory_scope",
        ["subject-memories", ..] => "memory_subject_memory",
        ["threads", ..]
            if segments.contains(&"summaries")
                || segments.contains(&"active-summary")
                || segments.contains(&"repair-summaries") =>
        {
            "memory_summary"
        }
        ["threads", ..] if segments.contains(&"records") => "memory_record",
        ["threads", ..] if segments.contains(&"snapshots") => "memory_thread_snapshot",
        ["threads", ..] => "memory_thread",
        ["records", ..] => "memory_record",
        ["summaries", ..] => "memory_summary",
        ["context", ..] => "memory_context",
        _ => "memory_internal_endpoint",
    };
    ClassifiedResource {
        resource_type,
        resource_id: if relative.is_empty() {
            "root".to_string()
        } else {
            relative.to_string()
        },
        resource_name: segments.last().copied(),
    }
}

fn request_action(method: &Method, path: &str) -> String {
    let final_segment = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    match final_segment {
        "run" | "run-once" => return "run".to_string(),
        "replay" => return "replay".to_string(),
        "compose" => return "compose".to_string(),
        "query" | "query-by-label" => return "query".to_string(),
        "batch-sync" => return "batch_sync".to_string(),
        value if value.starts_with("mark-") => return value.replace('-', "_"),
        "rotate-key" => return "rotate_key".to_string(),
        "generate-prompt" => return "generate_prompt".to_string(),
        _ => {}
    }
    match *method {
        Method::GET => "read",
        Method::PUT => "upsert",
        Method::POST => "execute",
        Method::DELETE => "delete",
        _ => method.as_str(),
    }
    .to_string()
}

fn response_outcome(status: StatusCode) -> &'static str {
    if status.is_success() {
        "accepted"
    } else if status.is_server_error() {
        "failed"
    } else {
        "rejected"
    }
}

fn bounded_audit_text(value: &str) -> String {
    let value = value.trim();
    if value.len() <= AUDIT_TEXT_LIMIT_BYTES {
        return value.to_string();
    }
    let mut end = AUDIT_TEXT_LIMIT_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn identity() -> InternalServiceTokenClaims {
        InternalServiceTokenClaims {
            iss: "task-runner".to_string(),
            sub: "task-runner".to_string(),
            caller: "task-runner".to_string(),
            aud: TOKEN_AUDIENCE.to_string(),
            scope: "memory.data".to_string(),
            trace_id: Uuid::new_v4().to_string(),
            iat: 1,
            exp: 2,
        }
    }

    #[test]
    fn classifies_data_and_control_plane_resources() {
        let summary = classify_resource(
            "/api/memory-engine/v1/threads/thread-1/summaries/summary-1",
        );
        assert_eq!(summary.resource_type, "memory_summary");
        assert_eq!(summary.resource_name, Some("summary-1"));

        let replay = classify_resource("/api/memory-engine/v1/queue-operations/replay");
        assert_eq!(replay.resource_type, "memory_queue_operation");
        assert_eq!(
            request_action(&Method::POST, "/api/memory-engine/v1/queue-operations/replay"),
            "replay"
        );

        let model = classify_resource("/api/memory-engine/v1/admin/model-profiles/model-1");
        assert_eq!(model.resource_type, "memory_model_profile");

        let subject_memory =
            classify_resource("/api/memory-engine/v1/subjects/user-1/memories/profile");
        assert_eq!(subject_memory.resource_type, "memory_subject_memory");

        let active_summary =
            classify_resource("/api/memory-engine/v1/threads/thread-1/active-summary/run");
        assert_eq!(active_summary.resource_type, "memory_summary");
    }

    #[test]
    fn audit_event_uses_verified_identity_and_real_response_outcome() {
        let request = Request::builder()
            .method(Method::PUT)
            .uri("/api/memory-engine/v1/threads/thread-1")
            .body(Body::empty())
            .expect("request");
        let audit = MemoryInternalRequestAudit::from_request(&request, &identity());
        let accepted = audit.event(StatusCode::OK);
        let rejected = audit.event(StatusCode::BAD_REQUEST);
        let failed = audit.event(StatusCode::INTERNAL_SERVER_ERROR);

        assert!(accepted.validate().is_ok());
        assert_eq!(accepted.resource_type, "memory_thread");
        assert_eq!(accepted.action, "upsert");
        assert_eq!(accepted.outcome, "accepted");
        assert_eq!(rejected.outcome, "rejected");
        assert_eq!(failed.outcome, "failed");
    }

    #[test]
    fn long_utf8_resource_paths_are_bounded_without_breaking_encoding() {
        let value = "记忆".repeat(100);
        let bounded = bounded_audit_text(value.as_str());
        assert!(bounded.len() <= AUDIT_TEXT_LIMIT_BYTES);
        assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
    }
}
