// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    pub task_runner_agent_account_id: Option<String>,
    pub plugin_sources: Vec<String>,
    pub skills: Vec<AgentSkill>,
    pub skill_ids: Vec<String>,
    pub default_skill_ids: Vec<String>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Agent {
    pub fn new(
        user_id: String,
        name: String,
        description: Option<String>,
        category: Option<String>,
        role_definition: String,
        plugin_sources: Vec<String>,
        skills: Vec<AgentSkill>,
        skill_ids: Vec<String>,
        default_skill_ids: Vec<String>,
        mcp_policy: Option<Value>,
        project_policy: Option<Value>,
        enabled: bool,
    ) -> Self {
        let now = crate::core::time::now_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            name,
            description,
            category,
            role_definition,
            task_runner_agent_account_id: None,
            plugin_sources,
            skills,
            skill_ids,
            default_skill_ids,
            mcp_policy,
            project_policy,
            enabled,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
