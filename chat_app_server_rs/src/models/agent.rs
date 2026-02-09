use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub ai_model_config_id: String,
    pub system_context_id: Option<String>,
    pub description: Option<String>,
    pub user_id: Option<String>,
    pub mcp_config_ids: Vec<String>,
    pub callable_agent_ids: Vec<String>,
    pub project_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, FromRow)]
pub struct AgentRow {
    pub id: String,
    pub name: String,
    pub ai_model_config_id: String,
    pub system_context_id: Option<String>,
    pub description: Option<String>,
    pub user_id: Option<String>,
    pub mcp_config_ids: Option<String>,
    pub callable_agent_ids: Option<String>,
    pub project_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl AgentRow {
    pub fn to_agent(self) -> Agent {
        Agent {
            id: self.id,
            name: self.name,
            ai_model_config_id: self.ai_model_config_id,
            system_context_id: self.system_context_id,
            description: self.description,
            user_id: self.user_id,
            mcp_config_ids: parse_json_list(&self.mcp_config_ids),
            callable_agent_ids: parse_json_list(&self.callable_agent_ids),
            project_id: self.project_id,
            workspace_dir: self.workspace_dir,
            enabled: self.enabled == 1,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

fn parse_json_list(raw: &Option<String>) -> Vec<String> {
    if let Some(s) = raw {
        if let Ok(v) = serde_json::from_str::<Value>(s) {
            if let Some(arr) = v.as_array() {
                return arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            }
        }
        return Vec::new();
    }
    Vec::new()
}
