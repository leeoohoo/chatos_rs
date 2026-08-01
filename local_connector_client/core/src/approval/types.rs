// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_sandbox_contract::{
    CommandExecutionApprovalDecision, GrantedPermissionProfile, PermissionGrantScope,
    RequestPermissionProfile,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApprovalState {
    #[serde(default)]
    pub(crate) default_mode: ApprovalMode,
    #[serde(default)]
    pub(crate) settings_revision: Option<String>,
    #[serde(default)]
    pub(crate) projects: Vec<ProjectApprovalState>,
    #[serde(default)]
    pub(crate) whitelist: Vec<CommandWhitelistEntry>,
    #[serde(default)]
    pub(crate) history: Vec<ApprovalHistoryEntry>,
}

impl Default for ApprovalState {
    fn default() -> Self {
        Self {
            default_mode: ApprovalMode::RequestApproval,
            settings_revision: None,
            projects: Vec::new(),
            whitelist: Vec::new(),
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApprovalMode {
    #[default]
    RequestApproval,
    AutoApproval,
    FullControl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ProjectApprovalState {
    pub(crate) project_key: ApprovalProjectKey,
    pub(crate) mode: Option<ApprovalMode>,
    pub(crate) updated_at: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ApprovalActionAuditDetail {
    pub(crate) key: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ApprovalActionAudit {
    pub(crate) kind: String,
    pub(crate) operation: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) details: Vec<ApprovalActionAuditDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) privacy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) safety: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recovery: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ApprovalConfirmationRequirement {
    pub(crate) kind: String,
    pub(crate) risk: String,
    pub(crate) challenge: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) permission_scope: Option<PermissionGrantScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) action_audit: Option<ApprovalActionAudit>,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandApprovalRequest {
    pub(crate) request_id: String,
    pub(crate) project_key: ApprovalProjectKey,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) redact_arguments_in_history: bool,
    pub(crate) cwd: String,
    pub(crate) source: String,
    pub(crate) requested_permissions: Option<RequestPermissionProfile>,
    pub(crate) session_id: Option<String>,
    pub(crate) action_audit: Option<ApprovalActionAudit>,
}

#[derive(Debug, Clone)]
pub(crate) enum ApprovalDecision {
    Approved {
        source: ApprovalSource,
        reason: Option<String>,
        whitelist_entry_id: Option<String>,
        granted_permissions: Option<GrantedPermissionProfile>,
        permission_scope: PermissionGrantScope,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) requested_permissions: Option<RequestPermissionProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) action_audit: Option<ApprovalActionAudit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) confirmation: Option<ApprovalConfirmationRequirement>,
    pub(crate) available_decisions: Vec<CommandExecutionApprovalDecision>,
}
