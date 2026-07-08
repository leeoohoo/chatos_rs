// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRuntimeSettings {
    pub session_id: String,
    pub user_id: String,
    pub selected_model_id: Option<String>,
    pub selected_model_name: Option<String>,
    pub selected_thinking_level: Option<String>,
    pub remote_connection_id: Option<String>,
    pub workspace_root: Option<String>,
    pub reasoning_enabled: bool,
    pub plan_mode_enabled: bool,
    pub mcp_enabled: bool,
    pub enabled_mcp_ids: Vec<String>,
    pub auto_create_task: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl SessionRuntimeSettings {
    pub fn new(session_id: String, user_id: String) -> Self {
        let now = crate::core::time::now_rfc3339();
        Self {
            session_id,
            user_id,
            selected_model_id: None,
            selected_model_name: None,
            selected_thinking_level: None,
            remote_connection_id: None,
            workspace_root: None,
            reasoning_enabled: false,
            plan_mode_enabled: false,
            mcp_enabled: true,
            enabled_mcp_ids: Vec::new(),
            auto_create_task: false,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
