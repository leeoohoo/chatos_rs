use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
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

#[derive(Debug, FromRow)]
pub struct AgentRow {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    pub plugin_sources: String,
    pub skills: String,
    pub skill_ids: String,
    pub default_skill_ids: String,
    pub mcp_policy: Option<String>,
    pub project_policy: Option<String>,
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl AgentRow {
    pub fn to_agent(self) -> Agent {
        Agent {
            id: self.id,
            user_id: self.user_id,
            name: self.name,
            description: self.description,
            category: self.category,
            role_definition: self.role_definition,
            plugin_sources: serde_json::from_str::<Vec<String>>(&self.plugin_sources)
                .unwrap_or_default(),
            skills: serde_json::from_str::<Vec<AgentSkill>>(&self.skills).unwrap_or_default(),
            skill_ids: serde_json::from_str::<Vec<String>>(&self.skill_ids).unwrap_or_default(),
            default_skill_ids: serde_json::from_str::<Vec<String>>(&self.default_skill_ids)
                .unwrap_or_default(),
            mcp_policy: self
                .mcp_policy
                .and_then(|value| serde_json::from_str::<Value>(&value).ok()),
            project_policy: self
                .project_policy
                .and_then(|value| serde_json::from_str::<Value>(&value).ok()),
            enabled: self.enabled == 1,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
