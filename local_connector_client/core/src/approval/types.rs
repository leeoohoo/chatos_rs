// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApprovalState {
    #[serde(default)]
    pub(crate) default_mode: ApprovalMode,
    #[serde(default)]
    pub(crate) projects: Vec<ProjectApprovalState>,
    #[serde(default)]
    pub(crate) whitelist: Vec<CommandWhitelistEntry>,
    #[serde(default)]
    pub(crate) history: Vec<ApprovalHistoryEntry>,
    #[serde(default)]
    pub(crate) ai: ApprovalAiSettings,
    #[serde(default)]
    pub(crate) memory: ApprovalMemorySettings,
}

impl Default for ApprovalState {
    fn default() -> Self {
        Self {
            default_mode: ApprovalMode::FullControl,
            projects: Vec::new(),
            whitelist: Vec::new(),
            history: Vec::new(),
            ai: ApprovalAiSettings::default(),
            memory: ApprovalMemorySettings::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApprovalMode {
    RequestApproval,
    AutoApproval,
    FullControl,
}

impl Default for ApprovalMode {
    fn default() -> Self {
        Self::FullControl
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProjectApprovalState {
    pub(crate) project_key: ApprovalProjectKey,
    pub(crate) mode: Option<ApprovalMode>,
    #[serde(default)]
    pub(crate) ai_enabled: bool,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApprovalAiSettings {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) base_url: Option<String>,
    #[serde(default)]
    pub(crate) api_key: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default = "default_ai_provider")]
    pub(crate) provider: String,
    #[serde(default)]
    pub(crate) supports_responses: bool,
    #[serde(default)]
    pub(crate) temperature: Option<f64>,
    #[serde(default)]
    pub(crate) max_output_tokens: Option<i64>,
    #[serde(default)]
    pub(crate) thinking_level: Option<String>,
    #[serde(default)]
    pub(crate) request_body_limit_bytes: Option<usize>,
}

impl Default for ApprovalAiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: None,
            api_key: None,
            model: None,
            provider: default_ai_provider(),
            supports_responses: false,
            temperature: Some(0.0),
            max_output_tokens: Some(1_200),
            thinking_level: None,
            request_body_limit_bytes: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApprovalMemorySettings {
    #[serde(default = "default_memory_source_id")]
    pub(crate) source_id: String,
    #[serde(default = "default_memory_timeout_ms")]
    pub(crate) timeout_ms: u64,
}

impl Default for ApprovalMemorySettings {
    fn default() -> Self {
        Self {
            source_id: default_memory_source_id(),
            timeout_ms: default_memory_timeout_ms(),
        }
    }
}

fn default_memory_source_id() -> String {
    "local_connector_approval".to_string()
}

fn default_memory_timeout_ms() -> u64 {
    30_000
}

fn default_ai_provider() -> String {
    "openai_compatible".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ApprovalProjectKey {
    pub(crate) owner_user_id: String,
    pub(crate) device_id: String,
    pub(crate) workspace_id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) project_root_relative_path: String,
    pub(crate) project_anchor_relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CommandWhitelistEntry {
    pub(crate) id: String,
    pub(crate) project_key: ApprovalProjectKey,
    pub(crate) command_fingerprint: String,
    pub(crate) command_display: String,
    pub(crate) normalized_command: String,
    pub(crate) cwd_scope: WhitelistCwdScope,
    pub(crate) created_by: ApprovalSource,
    pub(crate) created_at: String,
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WhitelistCwdScope {
    Project,
    Cwd,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApprovalSource {
    Whitelist,
    User,
    Ai,
    FullControl,
    StaticRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApprovalHistoryEntry {
    pub(crate) id: String,
    pub(crate) request_id: String,
    pub(crate) project_key: ApprovalProjectKey,
    pub(crate) command: String,
    pub(crate) normalized_command: String,
    pub(crate) cwd: String,
    pub(crate) source: String,
    pub(crate) mode: ApprovalMode,
    pub(crate) decision: String,
    pub(crate) decision_source: ApprovalSource,
    pub(crate) risk: String,
    pub(crate) reason: Option<String>,
    pub(crate) whitelist_entry_id: Option<String>,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandApprovalRequest {
    pub(crate) request_id: String,
    pub(crate) project_key: ApprovalProjectKey,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: String,
    pub(crate) source: String,
}

#[derive(Debug, Clone)]
pub(crate) enum ApprovalDecision {
    Approved {
        source: ApprovalSource,
        reason: Option<String>,
        whitelist_entry_id: Option<String>,
    },
    Denied {
        source: ApprovalSource,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PendingApprovalItem {
    pub(crate) id: String,
    pub(crate) request_id: String,
    pub(crate) project_key: ApprovalProjectKey,
    pub(crate) command: String,
    pub(crate) cwd: String,
    pub(crate) source: String,
    pub(crate) risk: String,
    pub(crate) reason: Option<String>,
    pub(crate) created_at: String,
}
