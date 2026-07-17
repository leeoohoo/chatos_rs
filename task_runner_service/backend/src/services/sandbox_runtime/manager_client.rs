// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::http_body::{
    read_response_json_limited, read_response_preview_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use std::path::Path;
use std::time::Duration;

use chatos_sandbox_contract::{
    EffectivePermissionSnapshot, EffectiveSandboxPolicy, SandboxLeasePolicyRequest,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AppConfig;
use crate::models::{RunOutputChangeManifest, TaskRecord, TaskRunRecord};

use super::workspace::{copy_workspace_to_sandbox, sandbox_baseline_workspace};
use super::{SandboxEnvironmentPlan, SandboxRuntimeContext};
#[derive(Debug, Serialize)]
struct CreateSandboxLeaseRequest {
    tenant_id: String,
    user_id: String,
    project_id: String,
    run_id: String,
    workspace_root: String,
    image_id: Option<String>,
    tools: Vec<String>,
    ttl_seconds: u64,
    #[serde(flatten)]
    policy: SandboxLeasePolicyRequest,
}

#[derive(Debug, Serialize)]
struct CreateSandboxEnvironmentLeaseRequest {
    tenant_id: String,
    user_id: String,
    project_id: String,
    run_id: String,
    workspace_root: String,
    ttl_seconds: u64,
    #[serde(flatten)]
    policy: SandboxLeasePolicyRequest,
}

#[derive(Debug, Serialize)]
struct StartSandboxEnvironmentRequest<'a> {
    lease_id: &'a str,
    primary_service_id: &'a str,
    services: &'a [super::SandboxEnvironmentServicePlan],
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateSandboxLeaseResponse {
    pub(super) lease_id: String,
    pub(super) sandbox_id: String,
    #[serde(default)]
    pub(super) is_environment: bool,
    #[serde(default)]
    pub(super) primary_service_id: Option<String>,
    pub(super) backend_id: Option<String>,
    #[serde(default)]
    pub(super) status: Option<String>,
    pub(super) agent_endpoint: Option<String>,
    pub(super) agent_token: Option<String>,
    pub(super) run_workspace: String,
    pub(super) expires_at: String,
    #[serde(default)]
    pub(super) last_error: Option<String>,
    pub(super) effective_policy: Option<EffectiveSandboxPolicy>,
    pub(super) effective_permissions: Option<EffectivePermissionSnapshot>,
}

#[derive(Debug, Deserialize)]
struct SandboxEnvironmentServiceResponse {
    service_id: String,
    #[serde(default)]
    backend_id: Option<String>,
    #[serde(default)]
    agent_endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SandboxEnvironmentLeaseResponse {
    lease_id: String,
    environment_id: String,
    #[serde(default)]
    backend_id: Option<String>,
    status: String,
    run_workspace: String,
    expires_at: String,
    #[serde(default)]
    primary_service_id: Option<String>,
    #[serde(default)]
    services: Vec<SandboxEnvironmentServiceResponse>,
    #[serde(default)]
    agent_token: Option<String>,
    #[serde(default)]
    effective_policy: Option<EffectiveSandboxPolicy>,
    #[serde(default)]
    effective_permissions: Option<EffectivePermissionSnapshot>,
}

impl SandboxEnvironmentLeaseResponse {
    fn into_runtime_response(self) -> CreateSandboxLeaseResponse {
        let primary_service_id = self.primary_service_id.clone();
        let primary = primary_service_id.as_deref().and_then(|service_id| {
            self.services
                .iter()
                .find(|service| service.service_id == service_id)
        });
        CreateSandboxLeaseResponse {
            lease_id: self.lease_id,
            sandbox_id: self.environment_id,
            is_environment: true,
            primary_service_id,
            backend_id: self
                .backend_id
                .or_else(|| primary.and_then(|service| service.backend_id.clone())),
            status: Some(self.status),
            agent_endpoint: primary.and_then(|service| service.agent_endpoint.clone()),
            agent_token: self.agent_token,
            run_workspace: self.run_workspace,
            expires_at: self.expires_at,
            last_error: None,
            effective_policy: self.effective_policy,
            effective_permissions: self.effective_permissions,
        }
    }
}

impl CreateSandboxLeaseResponse {
    pub(super) fn status_label(&self) -> &str {
        self.status.as_deref().unwrap_or("unknown")
    }

    fn is_ready(&self) -> bool {
        match self.status.as_deref() {
            Some("ready" | "running") => true,
            Some(_) => false,
            None => self
                .agent_endpoint
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
        }
    }

    pub(super) fn is_waiting(&self) -> bool {
        if self.status.is_none() {
            return self
                .agent_endpoint
                .as_deref()
                .map(str::trim)
                .is_none_or(str::is_empty);
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
        self.effective_policy = record.effective_policy;
        self.effective_permissions = record.effective_permissions;
    }

    fn apply_environment_record(&mut self, record: SandboxEnvironmentLeaseResponse) {
        let record = record.into_runtime_response();
        self.backend_id = record.backend_id;
        self.status = record.status;
        self.agent_endpoint = record.agent_endpoint;
        self.primary_service_id = record.primary_service_id;
        self.run_workspace = record.run_workspace;
        self.expires_at = record.expires_at;
        self.last_error = record.last_error;
        self.effective_policy = record.effective_policy;
        self.effective_permissions = record.effective_permissions;
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
    pub(super) effective_policy: Option<EffectiveSandboxPolicy>,
    pub(super) effective_permissions: Option<EffectivePermissionSnapshot>,
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
        let client = build_http_client(HttpClientTimeouts::new(Duration::from_secs(1_800)))
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
        image_id: Option<&str>,
        environment_plan: Option<&SandboxEnvironmentPlan>,
        source_workspace: &str,
        policy: SandboxLeasePolicyRequest,
    ) -> Result<CreateSandboxLeaseResponse, String> {
        if let Some(environment_plan) = environment_plan {
            return self
                .create_environment_lease(
                    task,
                    run,
                    workspace_root,
                    source_workspace,
                    ttl_seconds,
                    environment_plan,
                    policy,
                )
                .await;
        }
        let payload = CreateSandboxLeaseRequest {
            tenant_id: task.tenant_id.clone(),
            user_id: task.subject_id.clone(),
            project_id: task.project_id.clone(),
            run_id: run.id.clone(),
            workspace_root: workspace_root.to_string_lossy().to_string(),
            image_id: image_id.map(ToOwned::to_owned),
            tools: vec!["filesystem".to_string(), "terminal".to_string()],
            ttl_seconds,
            policy,
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
            if !status.is_success() {
                let body = read_error_body(response).await;
                if body.contains("sandbox_lease_idempotency_in_progress") && attempt < 5 {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                return Err(format!(
                    "sandbox lease request returned HTTP {status}: {body}"
                ));
            }
            return read_response_json_limited::<CreateSandboxLeaseResponse>(
                response,
                JSON_BODY_LIMIT_BYTES,
            )
            .await
            .map_err(|err| format!("decode sandbox lease response failed: {err}"));
        }
        Err("sandbox lease idempotency retry loop exhausted".to_string())
    }

    async fn create_environment_lease(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_root: &Path,
        source_workspace: &str,
        ttl_seconds: u64,
        environment_plan: &SandboxEnvironmentPlan,
        policy: SandboxLeasePolicyRequest,
    ) -> Result<CreateSandboxLeaseResponse, String> {
        let payload = CreateSandboxEnvironmentLeaseRequest {
            tenant_id: task.tenant_id.clone(),
            user_id: task.subject_id.clone(),
            project_id: task.project_id.clone(),
            run_id: run.id.clone(),
            workspace_root: workspace_root.to_string_lossy().to_string(),
            ttl_seconds,
            policy,
        };
        let prepared_response = self
            .apply_auth(
                self.client
                    .post(format!("{}/api/sandbox-environments/leases", self.base_url)),
            )?
            .header(
                "x-idempotency-key",
                format!("sandbox-environment-lease:{}", run.id),
            )
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("request sandbox environment lease failed: {err}"))?;
        let prepared: SandboxEnvironmentLeaseResponse =
            decode_success_json(prepared_response, "sandbox environment lease request").await?;

        match prepared.status.as_str() {
            "ready" | "running" | "starting" => {
                return Ok(prepared.into_runtime_response());
            }
            "failed" | "expired" | "destroyed" => {
                return Err(format!(
                    "sandbox environment lease is not reusable: environment_id={}, status={}",
                    prepared.environment_id, prepared.status
                ));
            }
            _ => {}
        }

        if prepared.status != "stopped" {
            let baseline_workspace =
                match sandbox_baseline_workspace(prepared.run_workspace.as_str()) {
                    Ok(path) => path,
                    Err(error) => {
                        let _ = self
                            .release_environment_response(&prepared, false, true)
                            .await;
                        return Err(error);
                    }
                };
            if let Err(error) =
                copy_workspace_to_sandbox(source_workspace, baseline_workspace.as_str()).and_then(
                    |_| {
                        copy_workspace_to_sandbox(source_workspace, prepared.run_workspace.as_str())
                    },
                )
            {
                let _ = self
                    .release_environment_response(&prepared, false, true)
                    .await;
                return Err(format!(
                    "synchronize source into prepared sandbox environment failed: {error}"
                ));
            }
        }

        let restart_services = Vec::new();
        let start_payload = StartSandboxEnvironmentRequest {
            lease_id: prepared.lease_id.as_str(),
            primary_service_id: environment_plan.primary_service_id.as_str(),
            services: if prepared.status == "stopped" {
                restart_services.as_slice()
            } else {
                environment_plan.services.as_slice()
            },
        };
        let start_response = match self
            .apply_auth(self.client.post(format!(
                "{}/api/sandbox-environments/{}/start",
                self.base_url, prepared.environment_id
            )))?
            .json(&start_payload)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let _ = self
                    .release_environment_response(&prepared, false, true)
                    .await;
                return Err(format!("start sandbox environment failed: {error}"));
            }
        };
        let started = match decode_success_json::<SandboxEnvironmentLeaseResponse>(
            start_response,
            "sandbox environment start request",
        )
        .await
        {
            Ok(started) => started,
            Err(error) => {
                let _ = self
                    .release_environment_response(&prepared, false, true)
                    .await;
                return Err(error);
            }
        };
        Ok(started.into_runtime_response())
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
            if response.is_environment {
                let record = self.get_environment(response.sandbox_id.as_str()).await?;
                response.apply_environment_record(record);
            } else {
                let record = self.get_sandbox(response.sandbox_id.as_str()).await?;
                response.apply_record(record);
            }
            deadline = sandbox_wait_deadline(response.expires_at.as_str());
        }
    }

    async fn get_sandbox(&self, sandbox_id: &str) -> Result<SandboxLeaseRecordResponse, String> {
        let response = self
            .apply_auth(
                self.client
                    .get(format!("{}/api/sandboxes/{}", self.base_url, sandbox_id)),
            )?
            .send()
            .await
            .map_err(|err| format!("request sandbox detail failed: {err}"))?;
        decode_success_json(response, "sandbox detail request").await
    }

    async fn get_environment(
        &self,
        environment_id: &str,
    ) -> Result<SandboxEnvironmentLeaseResponse, String> {
        let response = self
            .apply_auth(self.client.get(format!(
                "{}/api/sandbox-environments/{}",
                self.base_url, environment_id
            )))?
            .send()
            .await
            .map_err(|err| format!("request sandbox environment detail failed: {err}"))?;
        decode_success_json(response, "sandbox environment detail request").await
    }

    pub(super) async fn health(
        &self,
        context: &SandboxRuntimeContext,
    ) -> Result<SandboxHealthResult, String> {
        let response = self
            .apply_auth(self.client.get(format!(
                "{}/api/sandboxes/{}/health",
                self.base_url, context.sandbox_id
            )))?
            .send()
            .await
            .map_err(|err| format!("request sandbox health failed: {err}"))?;
        let raw: Value = decode_success_json(response, "sandbox health request").await?;
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
        let response = self
            .apply_auth(self.client.post(format!(
                "{}/api/sandboxes/{}/release",
                self.base_url, context.sandbox_id
            )))?
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("request sandbox release failed: {err}"))?;
        decode_success_json(response, "sandbox release request").await
    }

    pub(super) async fn release_response(
        &self,
        response: &CreateSandboxLeaseResponse,
        export_result: bool,
        destroy: bool,
    ) -> Result<ReleaseSandboxResponse, String> {
        let payload = ReleaseSandboxRequest {
            lease_id: response.lease_id.clone(),
            export_result,
            destroy,
        };
        let response = self
            .apply_auth(self.client.post(format!(
                "{}/api/sandboxes/{}/release",
                self.base_url, response.sandbox_id
            )))?
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("request sandbox release failed: {err}"))?;
        decode_success_json(response, "sandbox release request").await
    }

    async fn release_environment_response(
        &self,
        response: &SandboxEnvironmentLeaseResponse,
        export_result: bool,
        destroy: bool,
    ) -> Result<ReleaseSandboxResponse, String> {
        let payload = ReleaseSandboxRequest {
            lease_id: response.lease_id.clone(),
            export_result,
            destroy,
        };
        let response = self
            .apply_auth(self.client.post(format!(
                "{}/api/sandboxes/{}/release",
                self.base_url, response.environment_id
            )))?
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("request sandbox environment release failed: {err}"))?;
        decode_success_json(response, "sandbox environment release request").await
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

async fn read_error_body(response: reqwest::Response) -> String {
    read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await
}

async fn decode_success_json<T>(response: reqwest::Response, label: &str) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    if !status.is_success() {
        let body = read_error_body(response).await;
        return Err(format!("{label} returned HTTP {status}: {body}"));
    }
    read_response_json_limited::<T>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("decode {label} response failed: {err}"))
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
