// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatosContact {
    pub id: String,
    pub user_id: String,
    pub agent_id: String,
    pub agent_name_snapshot: Option<String>,
    #[serde(default)]
    pub task_runner_enabled: bool,
    #[serde(default)]
    pub task_runner_base_url: Option<String>,
    #[serde(default)]
    pub task_runner_agent_account_id: Option<String>,
    #[serde(default)]
    pub task_runner_username: Option<String>,
    #[serde(default, skip_serializing)]
    pub task_runner_password: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ChatosContact {
    pub fn new(
        user_id: String,
        agent_id: String,
        agent_name_snapshot: Option<String>,
        status: String,
    ) -> Self {
        let now = crate::core::time::now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            agent_id,
            agent_name_snapshot,
            task_runner_enabled: false,
            task_runner_base_url: None,
            task_runner_agent_account_id: None,
            task_runner_username: None,
            task_runner_password: None,
            status,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatosMemoryProject {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatosProjectAgentLink {
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
