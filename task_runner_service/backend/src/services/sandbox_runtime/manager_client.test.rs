// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
mod tests {
    use super::{
        apply_sandbox_audit_context, sandbox_lease_idempotency_key, SandboxLeaseListItem,
        SandboxManagerAuth, SandboxManagerAuthMode, SandboxManagerClient,
    };
    use crate::models::TaskRunRecord;
    use serde_json::json;

    fn run_with_attempt(attempt: i64) -> TaskRunRecord {
        let mut run = TaskRunRecord::queued(
            "run-1".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "thread-1".to_string(),
            json!({}),
            Vec::new(),
            "2026-08-02T00:00:00Z".to_string(),
        );
        run.attempt = attempt;
        run
    }

    #[test]
    fn recovered_run_uses_a_new_sandbox_idempotency_key() {
        assert_eq!(
            sandbox_lease_idempotency_key("sandbox-lease", &run_with_attempt(1)),
            "sandbox-lease:run-1:attempt:1"
        );
        assert_eq!(
            sandbox_lease_idempotency_key("sandbox-lease", &run_with_attempt(2)),
            "sandbox-lease:run-1:attempt:2"
        );
    }

    #[test]
    fn terminal_sandbox_lease_statuses_do_not_require_cleanup() {
        for status in ["destroyed", "expired", "failed"] {
            let lease = SandboxLeaseListItem {
                id: "lease-1".to_string(),
                sandbox_id: "sandbox-1".to_string(),
                status: status.to_string(),
            };
            assert!(!lease.requires_cleanup(), "status={status}");
        }

        for status in [
            "pending",
            "leasing",
            "starting",
            "ready",
            "running",
            "stopped",
            "releasing",
            "destroying",
        ] {
            let lease = SandboxLeaseListItem {
                id: "lease-1".to_string(),
                sandbox_id: "sandbox-1".to_string(),
                status: status.to_string(),
            };
            assert!(lease.requires_cleanup(), "status={status}");
        }
    }

    #[test]
    fn lease_response_without_effective_policy_stays_unknown() {
        let response =
            serde_json::from_value::<super::CreateSandboxLeaseResponse>(serde_json::json!({
                "lease_id": "lease-1",
                "sandbox_id": "sandbox-1",
                "backend_id": null,
                "agent_endpoint": "http://127.0.0.1:49888",
                "agent_token": null,
                "run_workspace": "/workspace",
                "expires_at": "2026-07-15T00:00:00Z"
            }))
            .expect("legacy response");

        assert!(response.effective_policy.is_none());
    }

    #[test]
    fn manager_request_uses_short_lived_token_without_client_key() {
        let client = SandboxManagerClient::new(
            "http://127.0.0.1:8095".to_string(),
            Some(SandboxManagerAuth {
                client_key: "a-long-task-runner-sandbox-secret".to_string(),
                mode: SandboxManagerAuthMode::Cloud,
                owner_user_id: None,
                cloud_http: Some(reqwest::Client::new()),
            }),
        )
        .expect("client");
        let request = client
            .apply_auth(client.client.get("http://127.0.0.1:8095/api/sandboxes"))
            .expect("apply auth")
            .build()
            .expect("request");
        assert!(!request.headers().contains_key("x-sandbox-client-key"));
        assert_eq!(
            request
                .headers()
                .get("x-sandbox-caller")
                .and_then(|value| value.to_str().ok()),
            Some("task-runner")
        );
        let token = request
            .headers()
            .get("x-sandbox-internal-token")
            .and_then(|value| value.to_str().ok())
            .expect("token");
        chatos_service_runtime::verify_internal_service_token(
            token,
            "a-long-task-runner-sandbox-secret",
            "task-runner",
            "sandbox-manager",
            "sandbox.service",
        )
        .expect("valid token");
    }

    #[test]
    fn local_connector_manager_request_signs_owner_bound_token() {
        let client = SandboxManagerClient::new(
            "http://127.0.0.1:8095".to_string(),
            Some(SandboxManagerAuth {
                client_key: "a-long-task-runner-local-connector-secret".to_string(),
                mode: SandboxManagerAuthMode::LocalConnector,
                owner_user_id: Some("user-1".to_string()),
                cloud_http: Some(reqwest::Client::new()),
            }),
        )
        .expect("client");
        let request = client
            .apply_auth(client.client.get(
                "http://127.0.0.1:8095/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes",
            ))
            .expect("apply auth")
            .build()
            .expect("request");
        assert_eq!(
            request
                .headers()
                .get("x-local-connector-caller")
                .and_then(|value| value.to_str().ok()),
            Some("task-runner")
        );
        assert_eq!(
            request
                .headers()
                .get("x-local-connector-owner-user-id")
                .and_then(|value| value.to_str().ok()),
            Some("user-1")
        );
        let token = request
            .headers()
            .get("x-local-connector-internal-token")
            .and_then(|value| value.to_str().ok())
            .expect("token");
        let claims = chatos_service_runtime::verify_internal_service_token(
            token,
            "a-long-task-runner-local-connector-secret",
            "task-runner",
            "local-connector-service",
            "sandbox.service",
        )
        .expect("valid local connector token");
        assert_eq!(claims.owner_user_id.as_deref(), Some("user-1"));
        assert!(!request.headers().contains_key("x-sandbox-client-key"));
    }

    #[test]
    fn sandbox_lease_request_includes_audit_context_without_signing_secret() {
        let client = SandboxManagerClient::new(
            "http://127.0.0.1:8095".to_string(),
            Some(SandboxManagerAuth {
                client_key: "a-long-task-runner-sandbox-secret".to_string(),
                mode: SandboxManagerAuthMode::Cloud,
                owner_user_id: None,
                cloud_http: Some(reqwest::Client::new()),
            }),
        )
        .expect("client");
        let request = client
            .apply_auth(
                client
                    .client
                    .post("http://127.0.0.1:8095/api/sandboxes/leases"),
            )
            .expect("apply auth");
        let request = apply_sandbox_audit_context(request, "user-1", "tenant-1", "project-1")
            .build()
            .expect("request");

        assert_eq!(
            request
                .headers()
                .get("x-chatos-owner-user-id")
                .and_then(|value| value.to_str().ok()),
            Some("user-1")
        );
        assert_eq!(
            request
                .headers()
                .get("x-chatos-tenant-id")
                .and_then(|value| value.to_str().ok()),
            Some("tenant-1")
        );
        assert_eq!(
            request
                .headers()
                .get("x-chatos-project-id")
                .and_then(|value| value.to_str().ok()),
            Some("project-1")
        );
        assert!(!request.headers().contains_key("x-sandbox-client-key"));
    }
}
