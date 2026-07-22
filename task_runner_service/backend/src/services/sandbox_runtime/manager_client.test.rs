// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
mod tests {
    use super::{SandboxLeaseListItem, SandboxManagerAuth, SandboxManagerClient};

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
                client_id: "task-runner".to_string(),
                client_key: "a-long-task-runner-sandbox-secret".to_string(),
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
}
