// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryContactDto {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    #[serde(default)]
    pub task_runner_enabled: bool,
    pub task_runner_base_url: Option<String>,
    pub task_runner_agent_account_id: Option<String>,
    pub task_runner_username: Option<String>,
    #[serde(default)]
    pub task_runner_has_password: bool,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryProjectDto {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub name: String,
    pub root_path: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub is_virtual: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncMemoryProjectRequestDto {
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub is_virtual: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryProjectAgentLinkDto {
    pub id: String,
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub first_bound_at: String,
    pub last_bound_at: String,
    pub last_message_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryProjectContactDto {
    pub project_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    pub contact_status: String,
    pub link_status: String,
    pub latest_session_id: Option<String>,
    pub last_bound_at: Option<String>,
    pub last_message_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncProjectAgentLinkRequestDto {
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub contact_id: Option<String>,
    pub session_id: Option<String>,
    pub last_message_at: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryProjectMemoryDto {
    pub id: String,
    pub user_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub memory_text: String,
    pub memory_version: i64,
    pub last_source_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryAgentRecallDto {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub recall_key: String,
    pub recall_text: String,
    #[serde(default)]
    pub level: i64,
    pub confidence: Option<f64>,
    pub last_seen_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateMemoryContactRequestDto {
    pub user_id: Option<String>,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateMemoryContactResponseDto {
    pub created: bool,
    pub contact: MemoryContactDto,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateContactTaskRunnerConfigRequestDto {
    pub enabled: bool,
    pub base_url: Option<String>,
    pub task_runner_agent_account_id: Option<String>,
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub clear_password: Option<bool>,
}
