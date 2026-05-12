use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosSessionDto {
    pub id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosAgentSkillDto {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosSkillDto {
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosSkillPluginCommandDto {
    pub name: String,
    pub source_path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub argument_hint: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosSkillPluginDto {
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
    pub commands: Vec<ChatosSkillPluginCommandDto>,
    pub command_count: i64,
    pub installed: bool,
    pub discoverable_skills: i64,
    pub installed_skill_count: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosAgentRuntimePluginSummaryDto {
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub content_summary: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosAgentRuntimeCommandSummaryDto {
    pub command_ref: String,
    pub name: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub plugin_source: String,
    pub source_path: String,
    pub content: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosAgentRuntimeSkillSummaryDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub plugin_source: Option<String>,
    pub source_type: String,
    pub source_path: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosAgentDto {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    #[serde(default)]
    pub plugin_sources: Vec<String>,
    #[serde(default)]
    pub skills: Vec<ChatosAgentSkillDto>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub default_skill_ids: Vec<String>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatosAgentRuntimeContextDto {
    pub agent_id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    #[serde(default)]
    pub plugin_sources: Vec<String>,
    #[serde(default)]
    pub runtime_plugins: Vec<ChatosAgentRuntimePluginSummaryDto>,
    #[serde(default)]
    pub skills: Vec<ChatosAgentSkillDto>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub runtime_skills: Vec<ChatosAgentRuntimeSkillSummaryDto>,
    #[serde(default)]
    pub runtime_commands: Vec<ChatosAgentRuntimeCommandSummaryDto>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChatosAgentRequest {
    pub user_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    pub plugin_sources: Option<Vec<String>>,
    pub skills: Option<Vec<ChatosAgentSkillDto>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChatosAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: Option<String>,
    pub plugin_sources: Option<Vec<String>>,
    pub skills: Option<Vec<ChatosAgentSkillDto>>,
    pub skill_ids: Option<Vec<String>>,
    pub default_skill_ids: Option<Vec<String>>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub enabled: Option<bool>,
}
