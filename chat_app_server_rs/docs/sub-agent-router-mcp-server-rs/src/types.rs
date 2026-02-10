use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub exec: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub instructions_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub skills: Option<Vec<String>>,
    pub default_skills: Option<Vec<String>>,
    pub commands: Option<Vec<CommandSpec>>,
    pub default_command: Option<String>,
    pub system_prompt_path: Option<String>,
    pub plugin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryData {
    pub agents: Vec<AgentSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSpec {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub path: String,
    pub plugin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub id: String,
    pub status: String,
    pub task: String,
    pub agent_id: Option<String>,
    pub command_id: Option<String>,
    pub payload_json: Option<String>,
    pub result_json: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub session_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub id: String,
    pub job_id: String,
    pub r#type: String,
    pub payload_json: Option<String>,
    pub created_at: String,
    pub session_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub endpoint_url: String,
    pub headers_json: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}
