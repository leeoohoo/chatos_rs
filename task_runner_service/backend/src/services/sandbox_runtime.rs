// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::Path;

use chatos_mcp_runtime::{BuiltinMcpKind, McpHttpServer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::models::{RunOutputChangesResponse, RunOutputDiffResponse, RunOutputFileChangeCounts};

use super::workspace_mcp::runtime_selected_builtin_kinds;
use super::*;

pub(super) const SANDBOX_MCP_SERVER_NAME: &str = "sandbox";
mod manager_client;
mod output;
mod routing;
#[path = "sandbox_runtime/run_service_lifecycle.rs"]
mod run_service_lifecycle;
#[path = "sandbox_runtime/run_service_policy.rs"]
mod run_service_policy;
mod workspace;

use manager_client::{
    CreateSandboxLeaseResponse, SandboxManagerAuth, SandboxManagerAuthMode, SandboxManagerClient,
};
pub(super) use output::SandboxOutputReport;
use output::{
    normalize_output_relative_path, read_output_change_manifest_for_run, read_output_diff_file,
};
use workspace::{
    copy_workspace_to_sandbox, is_local_connector_sandbox_manager, sandbox_baseline_workspace,
    sandbox_workspace_root,
};

struct SandboxTaskRoute {
    base_url: String,
    auth: Option<SandboxManagerAuth>,
    image_id: Option<String>,
    provider: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SandboxRuntimeContext {
    pub lease_id: String,
    pub sandbox_id: String,
    pub backend_id: Option<String>,
    pub agent_endpoint: String,
    pub agent_token: String,
    pub mcp_url: String,
    #[serde(default, skip_serializing)]
    pub manager_client_id: Option<String>,
    #[serde(default, skip_serializing)]
    pub manager_client_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub manager_auth_mode: Option<String>,
    #[serde(default, skip_serializing)]
    pub manager_owner_user_id: Option<String>,
    #[serde(default)]
    pub manager_base_url: String,
    pub run_workspace: String,
    pub workspace_root: String,
    pub expires_at: String,
}

impl SandboxRuntimeContext {
    pub(super) fn to_metadata(&self) -> Value {
        json!({
            "lease_id": self.lease_id,
            "sandbox_id": self.sandbox_id,
            "backend_id": self.backend_id,
            "agent_endpoint": self.agent_endpoint,
            "mcp_url": self.mcp_url,
            "manager_base_url": self.manager_base_url,
            "run_workspace": self.run_workspace,
            "workspace_root": self.workspace_root,
            "expires_at": self.expires_at,
        })
    }

    pub(super) fn to_mcp_server(&self, task: &TaskRecord, run: &TaskRunRecord) -> McpHttpServer {
        let mut headers = HashMap::new();
        headers.insert("X-Chatos-Sandbox-Id".to_string(), self.sandbox_id.clone());
        headers.insert(
            "X-Chatos-Sandbox-Lease-Id".to_string(),
            self.lease_id.clone(),
        );
        if let (Some(client_id), Some(client_key)) = (
            self.manager_client_id.as_deref(),
            self.manager_client_key.as_deref(),
        ) {
            if self.manager_auth_mode.as_deref() == Some("local_connector") {
                if let Some(owner_user_id) = self.manager_owner_user_id.as_deref() {
                    if let Ok(token) = chatos_service_runtime::issue_internal_service_token(
                        client_key,
                        "task-runner",
                        "local-connector-service",
                        "sandbox.service",
                        60,
                    ) {
                        headers.insert(
                            "x-local-connector-caller".to_string(),
                            client_id.to_string(),
                        );
                        headers.insert("x-local-connector-internal-token".to_string(), token);
                        headers.insert(
                            "x-local-connector-owner-user-id".to_string(),
                            owner_user_id.to_string(),
                        );
                    }
                }
            } else {
                headers.insert("x-sandbox-caller".to_string(), client_id.to_string());
                headers.insert("x-sandbox-client-key".to_string(), client_key.to_string());
                headers.insert(
                    "x-sandbox-internal-scope".to_string(),
                    "sandbox.service".to_string(),
                );
            }
        }
        headers.insert("X-Task-Runner-Task-Id".to_string(), task.id.clone());
        headers.insert("X-Task-Runner-Run-Id".to_string(), run.id.clone());
        headers.insert(
            "X-Task-Runner-Tenant-Id".to_string(),
            task.tenant_id.clone(),
        );
        headers.insert("X-Task-Runner-User-Id".to_string(), task.subject_id.clone());
        headers.insert(
            "X-Task-Runner-Project-Id".to_string(),
            task.project_id.clone(),
        );
        McpHttpServer::new(SANDBOX_MCP_SERVER_NAME, self.mcp_url.clone()).with_headers(headers)
    }
}

impl SandboxRuntimeContext {
    fn from_response(
        response: CreateSandboxLeaseResponse,
        workspace_root: &Path,
        manager_base_url: &str,
        manager_auth: Option<SandboxManagerAuth>,
    ) -> Result<Self, String> {
        let agent_endpoint = response
            .agent_endpoint
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "sandbox agent endpoint is empty".to_string())?;
        let manager_base_url = manager_base_url.trim().trim_end_matches('/').to_string();
        if manager_base_url.is_empty() {
            return Err("sandbox manager base url is empty".to_string());
        }
        let lease_id = response.lease_id;
        let sandbox_id = response.sandbox_id;
        let agent_token = response
            .agent_token
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| lease_id.clone());
        let (manager_client_id, manager_client_key, manager_auth_mode, manager_owner_user_id) =
            manager_auth
                .map(|auth| {
                    (
                        Some(auth.client_id),
                        Some(auth.client_key),
                        Some(match auth.mode {
                            SandboxManagerAuthMode::Cloud => "cloud".to_string(),
                            SandboxManagerAuthMode::LocalConnector => "local_connector".to_string(),
                        }),
                        auth.owner_user_id,
                    )
                })
                .unwrap_or((None, None, None, None));
        Ok(Self {
            lease_id,
            sandbox_id: sandbox_id.clone(),
            backend_id: response.backend_id,
            agent_token,
            mcp_url: format!("{manager_base_url}/api/sandboxes/{sandbox_id}/mcp"),
            manager_client_id,
            manager_client_key,
            manager_auth_mode,
            manager_owner_user_id,
            manager_base_url,
            agent_endpoint,
            run_workspace: response.run_workspace,
            workspace_root: workspace_root.to_string_lossy().to_string(),
            expires_at: response.expires_at,
        })
    }
}

pub(super) fn task_requires_sandbox(task: &TaskRecord) -> bool {
    if !task.mcp_config.enabled {
        return false;
    }
    runtime_selected_builtin_kinds(task)
        .into_iter()
        .any(|kind| {
            matches!(
                kind,
                BuiltinMcpKind::CodeMaintainerWrite | BuiltinMcpKind::TerminalController
            ) || (!task.mcp_config.requires_execution && kind == BuiltinMcpKind::CodeMaintainerRead)
        })
}

pub(super) fn sandbox_replaces_builtin_kind(kind: BuiltinMcpKind) -> bool {
    matches!(
        kind,
        BuiltinMcpKind::CodeMaintainerRead
            | BuiltinMcpKind::CodeMaintainerWrite
            | BuiltinMcpKind::TerminalController
    )
}

fn attach_sandbox_context_to_run(run: &mut TaskRunRecord, context: &SandboxRuntimeContext) {
    if let Some(object) = run.input_snapshot.as_object_mut() {
        object.insert("sandbox_enabled".to_string(), Value::Bool(true));
        object.insert("sandbox".to_string(), context.to_metadata());
    }
}
