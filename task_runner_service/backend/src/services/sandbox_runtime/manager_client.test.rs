// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
mod tests {
    use super::{
        apply_sandbox_audit_context, local_connector_retry_delay,
        sandbox_lease_idempotency_key, SandboxLeaseListItem, SandboxManagerAuth,
        SandboxManagerAuthMode, SandboxManagerClient,
    };
    use crate::models::TaskRunRecord;
    use crate::models::{TaskRecord, TaskStatus};
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::json;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

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

    fn task() -> TaskRecord {
        TaskRecord {
            id: "task-1".to_string(),
            title: "Task".to_string(),
            description: None,
            objective: "Test local sandbox lease retry".to_string(),
            input_payload: None,
            status: TaskStatus::Ready,
            priority: 0,
            tags: Vec::new(),
            default_model_config_id: None,
            memory_thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            subject_id: "user-1".to_string(),
            project_id: "project-1".to_string(),
            task_profile: crate::models::default_task_profile(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: None,
            owner_display_name: None,
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule: crate::models::TaskScheduleConfig::default(),
            parent_task_id: None,
            source_run_id: None,
            source_session_id: None,
            source_turn_id: None,
            source_user_message_id: None,
            prerequisite_task_ids: Vec::new(),
            task_tool_state: crate::models::TaskToolState::default(),
            plugin_config: Default::default(),
            mcp_config: crate::models::TaskMcpConfig::default(),
            created_at: "2026-08-12T00:00:00Z".to_string(),
            updated_at: "2026-08-12T00:00:00Z".to_string(),
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn local_connector_lease_retries_transient_service_unavailable() {
        async fn create_lease(
            State(attempts): State<Arc<AtomicUsize>>,
        ) -> (StatusCode, Json<serde_json::Value>) {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({"error": "Local Connector is reconnecting"})),
                );
            }
            (
                StatusCode::OK,
                Json(json!({
                    "lease_id": "lease-1",
                    "sandbox_id": "sandbox-1",
                    "backend_id": "local",
                    "agent_endpoint": "http://127.0.0.1:49888",
                    "agent_token": null,
                    "run_workspace": "/workspace",
                    "expires_at": "2026-08-12T01:00:00Z"
                })),
            )
        }

        let attempts = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route("/api/sandboxes/leases", post(create_lease))
            .with_state(attempts.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock Local Connector");
        let address = listener.local_addr().expect("mock address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve mock Local Connector");
        });
        let client = SandboxManagerClient::new(
            format!("http://{address}"),
            Some(SandboxManagerAuth {
                client_key: "a-long-task-runner-local-connector-secret".to_string(),
                mode: SandboxManagerAuthMode::LocalConnector,
                owner_user_id: Some("user-1".to_string()),
                cloud_http: Some(reqwest::Client::new()),
            }),
        )
        .expect("client");

        let response = client
            .create_lease(
                &task(),
                &run_with_attempt(1),
                Path::new("/workspace"),
                60,
                None,
                None,
                "/workspace",
                false,
                Default::default(),
            )
            .await
            .expect("transient 503 should be retried");

        assert_eq!(response.lease_id, "lease-1");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        server.abort();
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
    fn local_connector_retry_delay_is_bounded() {
        assert_eq!(local_connector_retry_delay(0).as_millis(), 250);
        assert_eq!(local_connector_retry_delay(1).as_millis(), 500);
        assert_eq!(local_connector_retry_delay(2).as_millis(), 1_000);
        assert_eq!(local_connector_retry_delay(3).as_millis(), 2_000);
        assert_eq!(local_connector_retry_delay(99).as_millis(), 2_000);
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
        assert!(client.is_local_connector());
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
