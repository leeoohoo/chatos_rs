// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySkill {
    pub id: String,
    pub user_id: String,
    pub plugin_source: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub source_path: String,
    pub version: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySkillPluginCommand {
    pub name: String,
    pub source_path: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySkillPlugin {
    pub id: String,
    pub user_id: String,
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub cache_path: Option<String>,
    pub content: Option<String>,
    pub commands: Vec<MemorySkillPluginCommand>,
    pub command_count: i64,
    pub installed: bool,
    pub discoverable_skills: i64,
    pub installed_skill_count: i64,
    pub updated_at: String,
}
