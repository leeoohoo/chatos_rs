// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::Path;

use chatos_mcp_management_sdk::SandboxProviderKind;
use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_sandbox_contract::{EffectivePermissionSnapshot, EffectiveSandboxPolicy};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::models::{RunOutputChangesResponse, RunOutputDiffResponse, RunOutputFileChangeCounts};

use super::workspace_mcp::{
    runtime_selected_builtin_kinds, runtime_selected_builtin_kinds_authoritative,
};
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
    CreateSandboxLeaseResponse, SandboxLeaseListItem, SandboxManagerAuth, SandboxManagerClient,
};
pub(super) use output::SandboxOutputReport;
use output::{
    normalize_output_relative_path, read_output_change_manifest_for_run, read_output_diff_file,
};
use workspace::{
    copy_workspace_to_sandbox, is_local_connector_sandbox_manager, sandbox_baseline_workspace,
    sandbox_workspace_root,
};

fn task_owner_user_id(task: &TaskRecord) -> Option<&str> {
    task.owner_user_id
        .as_deref()
        .and_then(normalized_text)
        .or_else(|| task.creator_user_id.as_deref().and_then(normalized_text))
        .or_else(|| normalized_text(task.subject_id.as_str()))
}

fn normalized_text(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

struct SandboxTaskRoute {
    base_url: String,
    auth: Option<SandboxManagerAuth>,
    image_id: Option<String>,
    environment_plan: Option<SandboxEnvironmentPlan>,
    provider: String,
    local_connector_pairing_id: Option<String>,
    policy: chatos_sandbox_contract::SandboxLeasePolicyRequest,
}

#[derive(Debug, Clone, Serialize)]
struct SandboxEnvironmentPlan {
    execution_service_id: String,
    services: Vec<SandboxEnvironmentServicePlan>,
    generated_config_files: Vec<SandboxGeneratedConfigFile>,
}

#[derive(Debug, Clone, Serialize)]
struct SandboxGeneratedConfigFile {
    path: String,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
struct SandboxEnvironmentServicePlan {
    service_id: String,
    environment_key: String,
    display_name: String,
    service_role: String,
    image_id: Option<String>,
    image_ref: Option<String>,
    dockerfile: Option<String>,
    environment: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "SandboxEnvironmentMcpPolicyPlan::is_empty")]
    mcp_policy: SandboxEnvironmentMcpPolicyPlan,
}

#[derive(Debug, Clone, Default, Serialize)]
struct SandboxEnvironmentMcpPolicyPlan {
    managed_by: String,
    attachment: String,
    filesystem: bool,
    terminal: bool,
}

impl SandboxEnvironmentMcpPolicyPlan {
    fn is_empty(&self) -> bool {
        self.managed_by.is_empty()
            && self.attachment.is_empty()
            && !self.filesystem
            && !self.terminal
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SandboxRuntimeContext {
    pub lease_id: String,
    pub sandbox_id: String,
    #[serde(default)]
    pub owner_user_id: String,
    #[serde(default)]
    pub is_environment: bool,
    #[serde(default)]
    pub service_id: Option<String>,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub local_connector_pairing_id: Option<String>,
    pub backend_id: Option<String>,
    pub agent_endpoint: Option<String>,
    pub mcp_url: String,
    #[serde(default)]
    pub manager_base_url: String,
    pub run_workspace: String,
    pub workspace_root: String,
    pub expires_at: String,
    pub effective_policy: EffectiveSandboxPolicy,
    pub effective_permissions: EffectivePermissionSnapshot,
}

impl SandboxRuntimeContext {
    pub(super) fn provider_kind(&self) -> Result<SandboxProviderKind, String> {
        match self.provider.trim().to_ascii_lowercase().as_str() {
            "local_connector" => Ok(SandboxProviderKind::LocalConnector),
            "cloud" | "cloud_sandbox_manager" => Ok(SandboxProviderKind::Cloud),
            value => Err(format!(
                "sandbox runtime context has an unsupported provider: {value}"
            )),
        }
    }

    pub(super) fn to_metadata(&self) -> Value {
        json!({
            "lease_id": self.lease_id,
            "sandbox_id": self.sandbox_id,
            "owner_user_id": self.owner_user_id,
            "is_environment": self.is_environment,
            "service_id": self.service_id,
            "provider": self.provider,
            "local_connector_pairing_id": self.local_connector_pairing_id,
            "backend_id": self.backend_id,
            "agent_endpoint": self.agent_endpoint,
            "mcp_url": self.mcp_url,
            "manager_base_url": self.manager_base_url,
            "run_workspace": self.run_workspace,
            "workspace_root": self.workspace_root,
            "expires_at": self.expires_at,
            "effective_policy": self.effective_policy,
            "effective_permissions": self.effective_permissions,
        })
    }

    fn from_response(
        response: CreateSandboxLeaseResponse,
        workspace_root: &Path,
        manager_base_url: &str,
        provider: &str,
        local_connector_pairing_id: Option<String>,
        owner_user_id: String,
    ) -> Result<Self, String> {
        let effective_policy = response
            .effective_policy
            .ok_or_else(|| "sandbox response missing required effective_policy".to_string())?;
        let effective_permissions = response
            .effective_permissions
            .ok_or_else(|| "sandbox response missing required effective_permissions".to_string())?;
        let local_connector = provider.eq_ignore_ascii_case("local_connector");
        let agent_endpoint = (!local_connector)
            .then_some(response.agent_endpoint)
            .flatten()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty());
        let manager_base_url = manager_base_url.trim().trim_end_matches('/').to_string();
        if manager_base_url.is_empty() {
            return Err("sandbox manager base url is empty".to_string());
        }
        let lease_id = response.lease_id;
        let sandbox_id = response.sandbox_id;
        let run_workspace = if local_connector {
            format!("local-sandbox://{sandbox_id}/workspace")
        } else {
            response.run_workspace
        };
        let workspace_root = if local_connector {
            "local-sandbox://workspace".to_string()
        } else {
            workspace_root.to_string_lossy().to_string()
        };
        Ok(Self {
            lease_id,
            sandbox_id: sandbox_id.clone(),
            owner_user_id,
            is_environment: response.is_environment,
            service_id: response.execution_service_id,
            provider: provider.to_string(),
            local_connector_pairing_id,
            backend_id: (!local_connector).then_some(response.backend_id).flatten(),
            mcp_url: if response.is_environment {
                format!("{manager_base_url}/api/sandbox-environments/{sandbox_id}/mcp")
            } else {
                format!("{manager_base_url}/api/sandboxes/{sandbox_id}/mcp")
            },
            manager_base_url,
            agent_endpoint,
            run_workspace,
            workspace_root,
            expires_at: response.expires_at,
            effective_policy,
            effective_permissions,
        })
    }
}

pub(super) fn task_requires_sandbox(task: &TaskRecord, authoritative_policy: bool) -> bool {
    if !task.mcp_config.enabled {
        return false;
    }
    let selected_builtin_kinds = if authoritative_policy {
        runtime_selected_builtin_kinds_authoritative(task)
    } else {
        runtime_selected_builtin_kinds(task)
    };
    selected_builtin_kinds.into_iter().any(|kind| {
        matches!(
            kind,
            BuiltinMcpKind::CodeMaintainerWrite | BuiltinMcpKind::TerminalController
        ) || (!task.mcp_config.requires_execution && kind == BuiltinMcpKind::CodeMaintainerRead)
    })
}

fn attach_sandbox_context_to_run(run: &mut TaskRunRecord, context: &SandboxRuntimeContext) {
    if let Some(object) = run.input_snapshot.as_object_mut() {
        object.insert("sandbox_enabled".to_string(), Value::Bool(true));
        object.insert("sandbox".to_string(), context.to_metadata());
    }
}
