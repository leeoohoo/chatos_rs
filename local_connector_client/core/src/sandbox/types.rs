// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalSandboxState {
    pub(crate) enabled: bool,
    pub(crate) selected_image_ref: Option<String>,
    #[serde(default)]
    pub(crate) images: Vec<LocalSandboxImageRecord>,
}

impl Default for LocalSandboxState {
    fn default() -> Self {
        Self {
            enabled: false,
            selected_image_ref: None,
            images: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalSandboxImageRecord {
    pub(crate) id: String,
    pub(crate) image_name: String,
    pub(crate) image_ref: String,
    pub(crate) features: Vec<String>,
    pub(crate) backend: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LocalSandboxRuntime {
    pub(crate) jobs: Arc<RwLock<Vec<LocalSandboxImageJob>>>,
    pub(crate) leases: Arc<RwLock<HashMap<String, LocalSandboxLease>>>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalSandboxImageJob {
    pub(crate) id: String,
    pub(crate) image_id: String,
    pub(crate) image_name: String,
    pub(crate) image_ref: String,
    pub(crate) features: Vec<String>,
    pub(crate) backend: String,
    pub(crate) status: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) output: String,
    pub(crate) error: Option<String>,
    #[serde(skip_serializing)]
    pub(crate) custom_build_script: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalSandboxLease {
    pub(crate) id: String,
    pub(crate) sandbox_id: String,
    pub(crate) tenant_id: String,
    pub(crate) user_id: String,
    pub(crate) project_id: String,
    pub(crate) run_id: String,
    pub(crate) workspace_root: String,
    pub(crate) run_workspace: String,
    pub(crate) backend: String,
    pub(crate) backend_id: Option<String>,
    pub(crate) image_id: Option<String>,
    pub(crate) image_ref: Option<String>,
    pub(crate) status: String,
    pub(crate) agent_endpoint: Option<String>,
    pub(crate) agent_token: String,
    pub(crate) resource_limits: LocalSandboxResourceLimits,
    pub(crate) network: LocalSandboxNetworkPolicy,
    pub(crate) tools: Vec<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) expires_at: String,
    pub(crate) destroyed_at: Option<String>,
    pub(crate) last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalSandboxResourceLimits {
    pub(crate) cpu: f32,
    pub(crate) memory_mb: u64,
    pub(crate) disk_mb: u64,
    pub(crate) max_processes: u32,
}

impl Default for LocalSandboxResourceLimits {
    fn default() -> Self {
        Self {
            cpu: 2.0,
            memory_mb: 4096,
            disk_mb: 10240,
            max_processes: 128,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LocalSandboxNetworkPolicy {
    pub(crate) mode: String,
}

impl Default for LocalSandboxNetworkPolicy {
    fn default() -> Self {
        Self {
            mode: "bridge".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateLocalSandboxLeaseRequest {
    pub(crate) tenant_id: String,
    pub(crate) user_id: String,
    pub(crate) project_id: String,
    pub(crate) run_id: String,
    pub(crate) workspace_root: String,
    pub(crate) image_id: Option<String>,
    #[serde(default)]
    pub(crate) tools: Vec<String>,
    pub(crate) ttl_seconds: Option<u64>,
    pub(crate) resource_limits: Option<LocalSandboxResourceLimits>,
    pub(crate) network: Option<LocalSandboxNetworkPolicy>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReleaseLocalSandboxRequest {
    pub(crate) lease_id: String,
    #[serde(default)]
    pub(crate) export_result: bool,
    #[serde(default = "default_true")]
    pub(crate) destroy: bool,
}

fn default_true() -> bool {
    true
}
