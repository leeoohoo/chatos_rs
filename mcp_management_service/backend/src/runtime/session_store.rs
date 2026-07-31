// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use chatos_mcp_management_sdk::{
    ProjectExecutionContext, ResolvedMcpRoute, RuntimeSessionRoutesResponse, RuntimeToolDescriptor,
    SandboxExecutionTarget,
};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct ExternalHttpProviderBinding {
    pub provider_ref: String,
    pub endpoint: reqwest::Url,
    pub headers: reqwest::header::HeaderMap,
    pub http: reqwest::Client,
    pub allow_writes: bool,
    pub allowed_tool_names: HashSet<String>,
    pub blocked_tool_names: HashSet<String>,
}

impl ExternalHttpProviderBinding {
    pub fn allows_tool(&self, tool_name: &str) -> bool {
        let tool_name = tool_name.trim();
        !tool_name.is_empty()
            && (self.allowed_tool_names.is_empty() || self.allowed_tool_names.contains(tool_name))
            && !self.blocked_tool_names.contains(tool_name)
    }
}

impl fmt::Debug for ExternalHttpProviderBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExternalHttpProviderBinding")
            .field("provider_ref", &self.provider_ref)
            .field("endpoint", &"[redacted]")
            .field("headers", &"[redacted]")
            .field("allow_writes", &self.allow_writes)
            .field("allowed_tool_names", &self.allowed_tool_names)
            .field("blocked_tool_names", &self.blocked_tool_names)
            .finish()
    }
}

#[derive(Clone)]
pub struct CloudStdioProviderBinding {
    pub provider_ref: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub allow_writes: bool,
    pub allowed_tool_names: HashSet<String>,
    pub blocked_tool_names: HashSet<String>,
}

impl CloudStdioProviderBinding {
    pub fn allows_tool(&self, tool_name: &str) -> bool {
        let tool_name = tool_name.trim();
        !tool_name.is_empty()
            && (self.allowed_tool_names.is_empty() || self.allowed_tool_names.contains(tool_name))
            && !self.blocked_tool_names.contains(tool_name)
    }
}

impl fmt::Debug for CloudStdioProviderBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CloudStdioProviderBinding")
            .field("provider_ref", &self.provider_ref)
            .field("command", &"[redacted]")
            .field("args", &"[redacted]")
            .field("env", &"[redacted]")
            .field("cwd", &"[redacted]")
            .field("allow_writes", &self.allow_writes)
            .field("allowed_tool_names", &self.allowed_tool_names)
            .field("blocked_tool_names", &self.blocked_tool_names)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeSessionSnapshot {
    pub session_id: String,
    pub caller_service: String,
    pub owner_user_id: String,
    pub agent_key: String,
    pub project_id: String,
    pub run_id: Option<String>,
    pub turn_id: Option<String>,
    pub task_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub default_model_config_id: Option<String>,
    pub expected_project_task_ids: Vec<String>,
    pub sandbox_target: Option<SandboxExecutionTarget>,
    pub project_context: ProjectExecutionContext,
    pub policy_revision: String,
    pub route_revision: String,
    pub routes: Vec<ResolvedMcpRoute>,
    pub tools: Vec<RuntimeToolDescriptor>,
    pub external_http_bindings: HashMap<String, ExternalHttpProviderBinding>,
    pub cloud_stdio_bindings: HashMap<String, CloudStdioProviderBinding>,
    pub expires_at: String,
    pub expires_at_unix: i64,
}

impl RuntimeSessionSnapshot {
    pub fn routes_response(&self) -> RuntimeSessionRoutesResponse {
        RuntimeSessionRoutesResponse {
            session_id: self.session_id.clone(),
            owner_user_id: self.owner_user_id.clone(),
            agent_key: self.agent_key.clone(),
            project_id: self.project_id.clone(),
            policy_revision: self.policy_revision.clone(),
            route_revision: self.route_revision.clone(),
            expires_at: self.expires_at.clone(),
            routes: self.routes.clone(),
            tools: self.tools.clone(),
        }
    }
}

#[derive(Clone, Default)]
pub struct RuntimeSessionStore {
    sessions: Arc<RwLock<HashMap<String, RuntimeSessionSnapshot>>>,
}

impl RuntimeSessionStore {
    pub async fn insert(&self, snapshot: RuntimeSessionSnapshot) {
        let now = chrono::Utc::now().timestamp();
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, value| value.expires_at_unix > now);
        sessions.insert(snapshot.session_id.clone(), snapshot);
    }

    pub async fn get(&self, session_id: &str) -> Option<RuntimeSessionSnapshot> {
        let now = chrono::Utc::now().timestamp();
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, value| value.expires_at_unix > now);
        sessions.get(session_id).cloned()
    }

    pub async fn remove(&self, session_id: &str) -> Option<RuntimeSessionSnapshot> {
        self.sessions.write().await.remove(session_id)
    }
}
