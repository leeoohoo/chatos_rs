use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{default_false, default_i64_0, default_true};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAgentSkill {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAgent {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    #[serde(default)]
    pub plugin_sources: Vec<String>,
    #[serde(default)]
    pub skills: Vec<MemoryAgentSkill>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub default_skill_ids: Vec<String>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryAgentRequest {
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    pub plugin_sources: Option<Vec<String>>,
    pub skills: Option<Vec<MemoryAgentSkill>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: Option<String>,
    pub plugin_sources: Option<Vec<String>>,
    pub skills: Option<Vec<MemoryAgentSkill>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAgentRuntimePluginSummary {
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub content_summary: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAgentRuntimeSkillSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub plugin_source: Option<String>,
    pub source_type: String,
    pub source_path: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAgentRuntimeContext {
    pub agent_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    #[serde(default)]
    pub plugin_sources: Vec<String>,
    #[serde(default)]
    pub runtime_plugins: Vec<MemoryAgentRuntimePluginSummary>,
    pub skills: Vec<MemoryAgentSkill>,
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub runtime_skills: Vec<MemoryAgentRuntimeSkillSummary>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub updated_at: String,
}

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
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub commands: Vec<MemorySkillPluginCommand>,
    #[serde(default = "default_i64_0")]
    pub command_count: i64,
    #[serde(default = "default_false")]
    pub installed: bool,
    #[serde(default = "default_i64_0")]
    pub discoverable_skills: i64,
    #[serde(default = "default_i64_0")]
    pub installed_skill_count: i64,
    pub updated_at: String,
}
