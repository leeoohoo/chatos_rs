// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AppConfig;
use crate::models::{RunOutputChangeManifest, TaskRecord, TaskRunRecord};

use super::SandboxRuntimeContext;
#[derive(Debug, Serialize)]
struct CreateSandboxLeaseRequest {
    tenant_id: String,
    user_id: String,
    project_id: String,
    run_id: String,
    workspace_root: String,
    tools: Vec<String>,
    ttl_seconds: u64,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateSandboxLeaseResponse {
    pub(super) lease_id: String,
    pub(super) sandbox_id: String,
    pub(super) backend_id: Option<String>,
    #[serde(default)]
    pub(super) status: Option<String>,
    pub(super) agent_endpoint: Option<String>,
    pub(super) agent_token: Option<String>,
    pub(super) run_workspace: String,
    pub(super) expires_at: String,
    #[serde(default)]
    pub(super) last_error: Option<String>,
}

impl CreateSandboxLeaseResponse {
    pub(super) fn status_label(&self) -> &str {
        self.status.as_deref().unwrap_or("unknown")
    }

    fn is_ready(&self) -> bool {
        matches!(
            self.status.as_deref().unwrap_or("ready"),
            "ready" | "running"
        ) && self
            .agent_endpoint
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    }

    pub(super) fn is_waiting(&self) -> bool {
        if self.status.is_none() {
            return self
                .agent_endpoint
                .as_deref()
                .map(str::trim)
                .map_or(true, str::is_empty);
        }
        matches!(
            self.status.as_deref().unwrap_or("leasing"),
            "pending" | "leasing" | "starting"
        )
    }

    fn is_terminal_failure(&self) -> bool {
        matches!(
            self.status.as_deref(),
            Some("failed" | "expired" | "destroyed")
        )
    }

    fn apply_record(&mut self, record: SandboxLeaseRecordResponse) {
        self.backend_id = record.backend_id;
        self.status = Some(record.status);
        self.agent_endpoint = record.agent_endpoint;
        self.run_workspace = record.run_workspace;
        self.expires_at = record.expires_at;
        self.last_error = record.last_error;
    }
}

#[derive(Debug, Deserialize)]
struct SandboxLeaseRecordResponse {
    pub(super) backend_id: Option<String>,
    pub(super) status: String,
    pub(super) agent_endpoint: Option<String>,
    pub(super) run_workspace: String,
    pub(super) expires_at: String,
    pub(super) last_error: Option<String>,
}

fn sandbox_wait_deadline(expires_at: &str) -> tokio::time::Instant {
    let fallback = tokio::time::Instant::now() + Duration::from_secs(7_200);
    let Ok(expires_at) = DateTime::parse_from_rfc3339(expires_at) else {
        return fallback;
    };
    let remaining = expires_at
        .with_timezone(&Utc)
        .signed_duration_since(Utc::now());
    if remaining <= chrono::Duration::zero() {
        return tokio::time::Instant::now();
    }
    tokio::time::Instant::now()
        + remaining.to_std().unwrap_or(Duration::from_secs(7_200))
        + Duration::from_secs(30)
}

#[derive(Debug, Serialize)]
struct ReleaseSandboxRequest {
    pub(super) lease_id: String,
    export_result: bool,
    destroy: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct ReleaseSandboxResponse {
    pub(super) ok: bool,
    pub(super) status: String,
    pub(super) output_workspace: Option<String>,
    pub(super) diff_summary: Option<String>,
    pub(super) output_error: Option<String>,
    pub(super) change_manifest: Option<RunOutputChangeManifest>,
}

pub(super) struct SandboxHealthResult {
    pub(super) ok: bool,
    pub(super) message: String,
    pub(super) raw: Value,
}

pub(super) struct SandboxManagerClient {
    pub(super) base_url: String,
    client: reqwest::Client,
    pub(super) auth: Option<SandboxManagerAuth>,
}

#[derive(Debug, Clone)]
pub(super) struct SandboxManagerAuth {
    pub(super) client_id: String,
    pub(super) client_key: String,
}

impl SandboxManagerClient {
    pub(super) fn new(base_url: String, auth: Option<SandboxManagerAuth>) -> Result<Self, String> {
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err("sandbox manager base url is empty".to_string());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|err| format!("build sandbox manager http client failed: {err}"))?;
        Ok(Self {
            base_url,
            client,
            auth,
        })
    }

    pub(super) async fn create_lease(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_root: &Path,
        ttl_seconds: u64,
    ) -> Result<CreateSandboxLeaseResponse, String> {
        let payload = CreateSandboxLeaseRequest {
            tenant_id: task.tenant_id.clone(),
            user_id: task.subject_id.clone(),
            project_id: task.project_id.clone(),
            run_id: run.id.clone(),
            workspace_root: workspace_root.to_string_lossy().to_string(),
            tools: vec!["filesystem".to_string(), "terminal".to_string()],
            ttl_seconds,
        };
        let idempotency_key = format!("sandbox-lease:{}", run.id);
        let url = format!("{}/api/sandboxes/leases", self.base_url);
        for attempt in 0..6 {
            let response = self
                .apply_auth(self.client.post(url.as_str()))?
                .header("x-idempotency-key", idempotency_key.as_str())
                .json(&payload)
                .send()
                .await
                .map_err(|err| format!("request sandbox lease failed: {err}"))?;
            let status = response.status();
            if status == reqwest::StatusCode::CONFLICT {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|err| format!("read conflict body failed: {err}"));
                if body.contains("sandbox_lease_idempotency_in_progress") && attempt < 5 {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                return Err(format!(
                    "sandbox lease request returned HTTP {status}: {body}"
                ));
            }
            return response
                .error_for_status()
                .map_err(|err| format!("sandbox lease request returned error: {err}"))?
                .json::<CreateSandboxLeaseResponse>()
                .await
                .map_err(|err| format!("decode sandbox lease response failed: {err}"));
        }
        Err("sandbox lease idempotency retry loop exhausted".to_string())
    }

    pub(super) async fn wait_until_ready(
        &self,
        mut response: CreateSandboxLeaseResponse,
    ) -> Result<CreateSandboxLeaseResponse, String> {
        let mut deadline = sandbox_wait_deadline(response.expires_at.as_str());
        loop {
            if response.is_ready() {
                return Ok(response);
            }
            if response.is_terminal_failure() {
                let detail = response
                    .last_error
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("no error detail");
                return Err(format!(
                    "sandbox lease reached terminal status {}: {detail}",
                    response.status_label()
                ));
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "sandbox lease did not become ready before timeout: sandbox_id={}, lease_id={}, status={}",
                    response.sandbox_id,
                    response.lease_id,
                    response.status_label()
                ));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            let record = self.get_sandbox(response.sandbox_id.as_str()).await?;
            response.apply_record(record);
            deadline = sandbox_wait_deadline(response.expires_at.as_str());
        }
    }

    async fn get_sandbox(&self, sandbox_id: &str) -> Result<SandboxLeaseRecordResponse, String> {
        self.apply_auth(
            self.client
                .get(format!("{}/api/sandboxes/{}", self.base_url, sandbox_id)),
        )?
        .send()
        .await
        .map_err(|err| format!("request sandbox detail failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("sandbox detail request returned error: {err}"))?
        .json::<SandboxLeaseRecordResponse>()
        .await
        .map_err(|err| format!("decode sandbox detail response failed: {err}"))
    }

    pub(super) async fn health(
        &self,
        context: &SandboxRuntimeContext,
    ) -> Result<SandboxHealthResult, String> {
        let raw = self
            .apply_auth(self.client.get(format!(
                "{}/api/sandboxes/{}/health",
                self.base_url, context.sandbox_id
            )))?
            .send()
            .await
            .map_err(|err| format!("request sandbox health failed: {err}"))?
            .error_for_status()
            .map_err(|err| format!("sandbox health request returned error: {err}"))?
            .json::<Value>()
            .await
            .map_err(|err| format!("decode sandbox health response failed: {err}"))?;
        let ok = raw.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let message = raw
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or(if ok { "ok" } else { "unknown health failure" })
            .to_string();
        Ok(SandboxHealthResult { ok, message, raw })
    }

    pub(super) async fn release(
        &self,
        context: &SandboxRuntimeContext,
        export_result: bool,
        destroy: bool,
    ) -> Result<ReleaseSandboxResponse, String> {
        let payload = ReleaseSandboxRequest {
            lease_id: context.lease_id.clone(),
            export_result,
            destroy,
        };
        self.apply_auth(self.client.post(format!(
            "{}/api/sandboxes/{}/release",
            self.base_url, context.sandbox_id
        )))?
        .json(&payload)
        .send()
        .await
        .map_err(|err| format!("request sandbox release failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("sandbox release request returned error: {err}"))?
        .json::<ReleaseSandboxResponse>()
        .await
        .map_err(|err| format!("decode sandbox release response failed: {err}"))
    }

    fn apply_auth(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, String> {
        if let Some(auth) = self.auth.as_ref() {
            let token = chatos_service_runtime::issue_internal_service_token(
                auth.client_key.as_str(),
                "task-runner",
                "sandbox-manager",
                "sandbox.service",
                60,
            )?;
            Ok(request
                .header("x-sandbox-caller", "task-runner")
                .header("x-sandbox-internal-token", token))
        } else {
            Ok(request)
        }
    }
}

impl SandboxManagerAuth {
    pub(super) fn from_config(config: &AppConfig) -> Option<Self> {
        match (
            config.sandbox_manager_client_id.clone(),
            config.sandbox_manager_client_key.clone(),
        ) {
            (Some(_client_id), Some(client_key)) => Some(Self {
                client_id: "task-runner".to_string(),
                client_key,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SandboxManagerAuth, SandboxManagerClient};

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
