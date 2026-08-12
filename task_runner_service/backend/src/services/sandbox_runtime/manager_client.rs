// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::http_body::{
    read_response_json_limited, read_response_preview_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use std::error::Error as StdError;
use std::path::Path;
use std::time::Duration;

use chatos_sandbox_contract::SandboxLeasePolicyRequest;
use serde_json::Value;
use tracing::warn;

use crate::models::{TaskRecord, TaskRunRecord};
use crate::trace_context::InternalTraceContextExt;

use super::workspace::{
    copy_workspace_to_sandbox, sandbox_baseline_workspace, write_generated_config_files,
};
use super::{SandboxEnvironmentPlan, SandboxRuntimeContext};

mod auth;
pub(super) use self::models::{
    CreateSandboxLeaseResponse, ReleaseSandboxResponse, SandboxHealthResult, SandboxLeaseListItem,
};
mod models;
use self::models::{
    sandbox_wait_deadline, CreateSandboxEnvironmentLeaseRequest, CreateSandboxLeaseRequest,
    ReleaseSandboxRequest, RenewSandboxEnvironmentLeaseRequest, SandboxEnvironmentLeaseResponse,
    SandboxLeaseRecordResponse, StartSandboxEnvironmentRequest,
};

pub(super) struct SandboxManagerClient {
    pub(super) base_url: String,
    client: reqwest::Client,
    pub(super) auth: Option<SandboxManagerAuth>,
}

#[derive(Debug, Clone)]
pub(super) struct SandboxManagerAuth {
    pub(super) client_key: String,
    mode: SandboxManagerAuthMode,
    owner_user_id: Option<String>,
    cloud_http: Option<reqwest::Client>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SandboxManagerAuthMode {
    Cloud,
    LocalConnector,
}

impl SandboxManagerClient {
    pub(super) fn new(base_url: String, auth: Option<SandboxManagerAuth>) -> Result<Self, String> {
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err("sandbox manager base url is empty".to_string());
        }
        let client = match auth.as_ref() {
            Some(auth) => auth
                .cloud_http
                .clone()
                .ok_or_else(|| "Sandbox service mTLS client is not configured".to_string())?,
            _ => build_http_client(HttpClientTimeouts::new(Duration::from_secs(1_800)))
                .map_err(|err| format!("build sandbox manager http client failed: {err}"))?,
        };
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
        preserve_platform_git: bool,
        policy: SandboxLeasePolicyRequest,
    ) -> Result<CreateSandboxLeaseResponse, String> {
        if let Some(environment_plan) = environment_plan {
            return self
                .create_environment_lease(
                    task,
                    run,
                    workspace_root,
                    source_workspace,
                    preserve_platform_git,
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
        let idempotency_key = sandbox_lease_idempotency_key("sandbox-lease", run);
        let url = format!("{}{}", self.base_url, self.api_path("/sandboxes/leases"));
        for attempt in 0..6 {
            let request = self
                .apply_auth(self.client.post(url.as_str()))?
                .header("x-idempotency-key", idempotency_key.as_str());
            let response = apply_sandbox_audit_context(
                request,
                task.owner_user_id
                    .as_deref()
                    .unwrap_or(task.subject_id.as_str()),
                task.tenant_id.as_str(),
                task.project_id.as_str(),
            )
            .json(&payload)
            .with_internal_trace_context()
            .send()
            .await;
            let response = match response {
                Ok(response) => response,
                Err(error) if self.is_local_connector() && attempt < 5 => {
                    warn!(
                        attempt = attempt + 1,
                        error = format_reqwest_error(&error),
                        "Local Connector sandbox lease request failed while the client may be reconnecting; retrying"
                    );
                    tokio::time::sleep(local_connector_retry_delay(attempt)).await;
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "request sandbox lease failed: {}",
                        format_reqwest_error(&error)
                    ));
                }
            };
            let status = response.status();
            if !status.is_success() {
                let body = read_error_body(response).await;
                if body.contains("sandbox_lease_idempotency_in_progress") && attempt < 5 {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                if self.is_local_connector()
                    && matches!(status.as_u16(), 502 | 503 | 504)
                    && attempt < 5
                {
                    warn!(
                        attempt = attempt + 1,
                        %status,
                        error = body.as_str(),
                        "Local Connector sandbox lease endpoint is temporarily unavailable; retrying"
                    );
                    tokio::time::sleep(local_connector_retry_delay(attempt)).await;
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

    fn is_local_connector(&self) -> bool {
        self.auth
            .as_ref()
            .is_some_and(|auth| auth.mode == SandboxManagerAuthMode::LocalConnector)
    }

    async fn create_environment_lease(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_root: &Path,
        source_workspace: &str,
        preserve_platform_git: bool,
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
        let request = self
            .apply_auth(self.client.post(format!(
                "{}{}",
                self.base_url,
                self.api_path("/sandbox-environments/leases")
            )))?
            .header(
                "x-idempotency-key",
                sandbox_lease_idempotency_key("sandbox-environment-lease", run),
            );
        let prepared_response = apply_sandbox_audit_context(
            request,
            task.owner_user_id
                .as_deref()
                .unwrap_or(task.subject_id.as_str()),
            task.tenant_id.as_str(),
            task.project_id.as_str(),
        )
        .json(&payload)
        .with_internal_trace_context()
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
            if let Err(error) = copy_workspace_to_sandbox(
                source_workspace,
                baseline_workspace.as_str(),
                preserve_platform_git,
            )
            .and_then(|_| {
                copy_workspace_to_sandbox(
                    source_workspace,
                    prepared.run_workspace.as_str(),
                    preserve_platform_git,
                )
            }) {
                let _ = self
                    .release_environment_response(&prepared, false, true)
                    .await;
                return Err(format!(
                    "synchronize source into prepared sandbox environment failed: {error}"
                ));
            }
            if let Err(error) = write_generated_config_files(
                baseline_workspace.as_str(),
                environment_plan.generated_config_files.as_slice(),
            )
            .and_then(|_| {
                write_generated_config_files(
                    prepared.run_workspace.as_str(),
                    environment_plan.generated_config_files.as_slice(),
                )
            }) {
                let _ = self
                    .release_environment_response(&prepared, false, true)
                    .await;
                return Err(format!(
                    "materialize generated sandbox environment files failed: {error}"
                ));
            }
        }

        let restart_services = Vec::new();
        let start_payload = StartSandboxEnvironmentRequest {
            lease_id: prepared.lease_id.as_str(),
            execution_service_id: environment_plan.execution_service_id.as_str(),
            services: if prepared.status == "stopped" {
                restart_services.as_slice()
            } else {
                environment_plan.services.as_slice()
            },
        };
        let start_response = match self
            .apply_auth(self.client.post(format!(
                "{}{}",
                self.base_url,
                self.api_path(
                    format!("/sandbox-environments/{}/start", prepared.environment_id).as_str()
                )
            )))?
            .json(&start_payload)
            .with_internal_trace_context()
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
            .apply_auth(self.client.get(format!(
                "{}{}",
                self.base_url,
                self.api_path(format!("/sandboxes/{sandbox_id}").as_str())
            )))?
            .with_internal_trace_context()
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
                "{}{}",
                self.base_url,
                self.api_path(format!("/sandbox-environments/{environment_id}").as_str())
            )))?
            .with_internal_trace_context()
            .send()
            .await
            .map_err(|err| format!("request sandbox environment detail failed: {err}"))?;
        decode_success_json(response, "sandbox environment detail request").await
    }

    pub(super) async fn list_run_leases(
        &self,
        run_id: &str,
    ) -> Result<Vec<SandboxLeaseListItem>, String> {
        let response = self
            .apply_auth(self.client.get(format!(
                "{}{}",
                self.base_url,
                self.api_path("/sandboxes")
            )))?
            .query(&[("run_id", run_id), ("limit", "100")])
            .with_internal_trace_context()
            .send()
            .await
            .map_err(|err| format!("request sandbox leases for run failed: {err}"))?;
        decode_success_json(response, "sandbox leases for run request").await
    }

    pub(super) async fn release_list_item(
        &self,
        record: &SandboxLeaseListItem,
        export_result: bool,
        destroy: bool,
    ) -> Result<ReleaseSandboxResponse, String> {
        let payload = ReleaseSandboxRequest {
            lease_id: record.id.clone(),
            export_result,
            destroy,
        };
        let response = self
            .apply_auth(self.client.post(format!(
                "{}{}",
                self.base_url,
                self.api_path(format!("/sandboxes/{}/release", record.sandbox_id).as_str())
            )))?
            .json(&payload)
            .with_internal_trace_context()
            .send()
            .await
            .map_err(|err| format!("request sandbox release for terminal run failed: {err}"))?;
        decode_success_json(response, "sandbox release for terminal run request").await
    }

    pub(super) async fn health(
        &self,
        context: &SandboxRuntimeContext,
    ) -> Result<SandboxHealthResult, String> {
        let response = self
            .apply_auth(self.client.get(format!(
                "{}{}",
                self.base_url,
                self.api_path(format!("/sandboxes/{}/health", context.sandbox_id).as_str())
            )))?
            .with_internal_trace_context()
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

    pub(super) async fn renew_environment_lease(
        &self,
        context: &SandboxRuntimeContext,
        ttl_seconds: u64,
    ) -> Result<String, String> {
        if !context.is_environment {
            return Ok(context.expires_at.clone());
        }
        let payload = RenewSandboxEnvironmentLeaseRequest {
            lease_id: context.lease_id.as_str(),
            ttl_seconds,
        };
        let response = self
            .apply_auth(self.client.post(format!(
                "{}{}",
                self.base_url,
                self.api_path(
                    format!("/sandbox-environments/{}/renew", context.sandbox_id).as_str()
                )
            )))?
            .json(&payload)
            .with_internal_trace_context()
            .send()
            .await
            .map_err(|err| format!("request sandbox environment renewal failed: {err}"))?;
        let renewed: SandboxEnvironmentLeaseResponse =
            decode_success_json(response, "sandbox environment renewal request").await?;
        Ok(renewed.expires_at)
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
                "{}{}",
                self.base_url,
                self.api_path(format!("/sandboxes/{}/release", context.sandbox_id).as_str())
            )))?
            .json(&payload)
            .with_internal_trace_context()
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
                "{}{}",
                self.base_url,
                self.api_path(format!("/sandboxes/{}/release", response.sandbox_id).as_str())
            )))?
            .json(&payload)
            .with_internal_trace_context()
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
                "{}{}",
                self.base_url,
                self.api_path(format!("/sandboxes/{}/release", response.environment_id).as_str())
            )))?
            .json(&payload)
            .with_internal_trace_context()
            .send()
            .await
            .map_err(|err| format!("request sandbox environment release failed: {err}"))?;
        decode_success_json(response, "sandbox environment release request").await
    }

    fn api_path(&self, path: &str) -> String {
        let prefix = match self.auth.as_ref().map(|auth| auth.mode) {
            Some(SandboxManagerAuthMode::Cloud) => "/api/internal",
            _ => "/api",
        };
        format!("{prefix}{path}")
    }

    fn apply_auth(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, String> {
        if let Some(auth) = self.auth.as_ref() {
            if auth.mode == SandboxManagerAuthMode::LocalConnector {
                let owner_user_id = auth.owner_user_id.as_deref().ok_or_else(|| {
                    "Local Connector sandbox auth is missing owner user id".to_string()
                })?;
                let token = chatos_service_runtime::issue_internal_service_token_for_owner(
                    auth.client_key.as_str(),
                    "task-runner",
                    "local-connector-service",
                    "sandbox.service",
                    60,
                    owner_user_id,
                )?;
                return Ok(request
                    .header("x-local-connector-caller", "task-runner")
                    .header("x-local-connector-internal-token", token)
                    .header("x-local-connector-owner-user-id", owner_user_id));
            }
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

pub(super) fn local_connector_retry_delay(attempt: usize) -> Duration {
    const DELAYS_MS: [u64; 5] = [250, 500, 1_000, 2_000, 2_000];
    Duration::from_millis(DELAYS_MS[attempt.min(DELAYS_MS.len() - 1)])
}

pub(super) fn format_reqwest_error(error: &reqwest::Error) -> String {
    let mut detail = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_message = cause.to_string();
        if !cause_message.is_empty() && !detail.contains(cause_message.as_str()) {
            detail.push_str(": ");
            detail.push_str(cause_message.as_str());
        }
        source = cause.source();
    }
    detail
}

fn apply_sandbox_audit_context(
    request: reqwest::RequestBuilder,
    represented_user_id: &str,
    tenant_id: &str,
    project_id: &str,
) -> reqwest::RequestBuilder {
    request
        .header("x-chatos-owner-user-id", represented_user_id)
        .header("x-chatos-tenant-id", tenant_id)
        .header("x-chatos-project-id", project_id)
}

fn sandbox_lease_idempotency_key(prefix: &str, run: &TaskRunRecord) -> String {
    format!("{prefix}:{}:attempt:{}", run.id, run.attempt.max(1))
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

#[cfg(test)]
include!("manager_client.test.rs");
