// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_sandbox_contract::{
    EffectivePermissionSnapshot, EffectiveSandboxPolicy, SandboxLeasePolicyRequest,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::RunOutputChangeManifest;

#[derive(Debug, Serialize)]
pub(crate) struct CreateSandboxLeaseRequest {
    pub(crate) tenant_id: String,
    pub(crate) user_id: String,
    pub(crate) project_id: String,
    pub(crate) run_id: String,
    pub(crate) workspace_root: String,
    pub(crate) image_id: Option<String>,
    pub(crate) tools: Vec<String>,
    pub(crate) ttl_seconds: u64,
    #[serde(flatten)]
    pub(crate) policy: SandboxLeasePolicyRequest,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateSandboxEnvironmentLeaseRequest {
    pub(crate) tenant_id: String,
    pub(crate) user_id: String,
    pub(crate) project_id: String,
    pub(crate) run_id: String,
    pub(crate) workspace_root: String,
    pub(crate) ttl_seconds: u64,
    #[serde(flatten)]
    pub(crate) policy: SandboxLeasePolicyRequest,
}

#[derive(Debug, Serialize)]
pub(crate) struct StartSandboxEnvironmentRequest<'a> {
    pub(crate) lease_id: &'a str,
    pub(crate) execution_service_id: &'a str,
    pub(crate) services: &'a [super::super::SandboxEnvironmentServicePlan],
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateSandboxLeaseResponse {
    pub(crate) lease_id: String,
    pub(crate) sandbox_id: String,
    #[serde(default)]
    pub(crate) is_environment: bool,
    #[serde(default)]
    #[serde(alias = "primary_service_id")]
    pub(crate) execution_service_id: Option<String>,
    pub(crate) backend_id: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<String>,
    pub(crate) agent_endpoint: Option<String>,
    pub(crate) run_workspace: String,
    pub(crate) expires_at: String,
    #[serde(default)]
    pub(crate) last_error: Option<String>,
    pub(crate) effective_policy: Option<EffectiveSandboxPolicy>,
    pub(crate) effective_permissions: Option<EffectivePermissionSnapshot>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SandboxEnvironmentServiceResponse {
    pub(crate) service_id: String,
    #[serde(default)]
    pub(crate) backend_id: Option<String>,
    #[serde(default)]
    pub(crate) agent_endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SandboxEnvironmentLeaseResponse {
    pub(crate) lease_id: String,
    pub(crate) environment_id: String,
    #[serde(default)]
    pub(crate) backend_id: Option<String>,
    pub(crate) status: String,
    pub(crate) run_workspace: String,
    pub(crate) expires_at: String,
    #[serde(default)]
    #[serde(alias = "primary_service_id")]
    pub(crate) execution_service_id: Option<String>,
    #[serde(default)]
    pub(crate) services: Vec<SandboxEnvironmentServiceResponse>,
    #[serde(default)]
    pub(crate) effective_policy: Option<EffectiveSandboxPolicy>,
    #[serde(default)]
    pub(crate) effective_permissions: Option<EffectivePermissionSnapshot>,
}

impl SandboxEnvironmentLeaseResponse {
    pub(crate) fn into_runtime_response(self) -> CreateSandboxLeaseResponse {
        let execution_service_id = self.execution_service_id.clone();
        let execution = execution_service_id.as_deref().and_then(|service_id| {
            self.services
                .iter()
                .find(|service| service.service_id == service_id)
        });
        CreateSandboxLeaseResponse {
            lease_id: self.lease_id,
            sandbox_id: self.environment_id,
            is_environment: true,
            execution_service_id,
            backend_id: self
                .backend_id
                .or_else(|| execution.and_then(|service| service.backend_id.clone())),
            status: Some(self.status),
            agent_endpoint: execution.and_then(|service| service.agent_endpoint.clone()),
            run_workspace: self.run_workspace,
            expires_at: self.expires_at,
            last_error: None,
            effective_policy: self.effective_policy,
            effective_permissions: self.effective_permissions,
        }
    }
}

impl CreateSandboxLeaseResponse {
    pub(crate) fn status_label(&self) -> &str {
        self.status.as_deref().unwrap_or("unknown")
    }

    pub(crate) fn is_ready(&self) -> bool {
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

    pub(crate) fn is_waiting(&self) -> bool {
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

    pub(crate) fn is_terminal_failure(&self) -> bool {
        matches!(
            self.status.as_deref(),
            Some("failed" | "expired" | "destroyed")
        )
    }

    pub(crate) fn apply_record(&mut self, record: SandboxLeaseRecordResponse) {
        self.backend_id = record.backend_id;
        self.status = Some(record.status);
        self.agent_endpoint = record.agent_endpoint;
        self.run_workspace = record.run_workspace;
        self.expires_at = record.expires_at;
        self.last_error = record.last_error;
        self.effective_policy = record.effective_policy;
        self.effective_permissions = record.effective_permissions;
    }

    pub(crate) fn apply_environment_record(&mut self, record: SandboxEnvironmentLeaseResponse) {
        let record = record.into_runtime_response();
        self.backend_id = record.backend_id;
        self.status = record.status;
        self.agent_endpoint = record.agent_endpoint;
        self.execution_service_id = record.execution_service_id;
        self.run_workspace = record.run_workspace;
        self.expires_at = record.expires_at;
        self.last_error = record.last_error;
        self.effective_policy = record.effective_policy;
        self.effective_permissions = record.effective_permissions;
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct SandboxLeaseRecordResponse {
    pub(crate) backend_id: Option<String>,
    pub(crate) status: String,
    pub(crate) agent_endpoint: Option<String>,
    pub(crate) run_workspace: String,
    pub(crate) expires_at: String,
    pub(crate) last_error: Option<String>,
    pub(crate) effective_policy: Option<EffectiveSandboxPolicy>,
    pub(crate) effective_permissions: Option<EffectivePermissionSnapshot>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SandboxLeaseListItem {
    pub(crate) id: String,
    pub(crate) sandbox_id: String,
    pub(crate) status: String,
}

impl SandboxLeaseListItem {
    pub(crate) fn requires_cleanup(&self) -> bool {
        !matches!(self.status.as_str(), "destroyed" | "expired" | "failed")
    }
}

pub(crate) fn sandbox_wait_deadline(expires_at: &str) -> tokio::time::Instant {
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
pub(crate) struct ReleaseSandboxRequest {
    pub(crate) lease_id: String,
    pub(crate) export_result: bool,
    pub(crate) destroy: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ReleaseSandboxResponse {
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) output_workspace: Option<String>,
    pub(crate) diff_summary: Option<String>,
    pub(crate) output_error: Option<String>,
    pub(crate) change_manifest: Option<RunOutputChangeManifest>,
}

impl ReleaseSandboxResponse {
    pub(crate) fn redact_local_paths(&mut self) {
        self.output_workspace = None;
        if self.output_error.is_some() {
            self.output_error = Some("local sandbox output export failed".to_string());
        }
        if let Some(manifest) = self.change_manifest.as_mut() {
            manifest.output_workspace = None;
            manifest.manifest_path = None;
            for file in &mut manifest.files {
                file.diff_available = false;
                file.diff_ref = None;
            }
            manifest.counts.diff_available = 0;
        }
    }
}

pub(crate) struct SandboxHealthResult {
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) raw: Value,
}
