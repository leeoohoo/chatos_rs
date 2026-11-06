// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentDirectoryItem {
    pub tenant_id: String,
    pub source_id: String,
    pub agent_subject_id: String,
    pub agent_id: String,
    pub display_name: Option<String>,
    pub thread_count: i64,
    pub project_count: i64,
    pub memory_count: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentProjectItem {
    pub tenant_id: String,
    pub source_id: String,
    pub project_subject_id: String,
    pub project_id: String,
    pub display_name: Option<String>,
    pub thread_count: i64,
    pub memory_count: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentConversationItem {
    pub tenant_id: String,
    pub source_id: String,
    pub agent_subject_id: String,
    pub thread_id: String,
    pub project_subject_id: Option<String>,
    pub project_display_name: Option<String>,
    pub conversation_id: Option<String>,
    pub thread_kind: String,
    pub record_count: i64,
    pub status: String,
    pub summary_status: String,
    pub updated_at: String,
}
