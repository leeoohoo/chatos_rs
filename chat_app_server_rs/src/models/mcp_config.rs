use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    pub r#type: String,
    pub args: Option<Value>,
    pub env: Option<Value>,
    pub cwd: Option<String>,
    pub user_id: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl McpConfig {
    pub fn new(name: String, command: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            command,
            r#type: "stdio".to_string(),
            args: None,
            env: None,
            cwd: None,
            user_id: None,
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, FromRow)]
pub struct McpConfigRow {
    pub id: String,
    pub name: String,
    pub command: String,
    pub r#type: String,
    pub args: Option<String>,
    pub env: Option<String>,
    pub cwd: Option<String>,
    pub user_id: Option<String>,
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl McpConfigRow {
    pub fn to_config(self) -> McpConfig {
        McpConfig {
            id: self.id,
            name: self.name,
            command: self.command,
            r#type: self.r#type,
            args: self
                .args
                .and_then(|v| serde_json::from_str::<Value>(&v).ok()),
            env: self
                .env
                .and_then(|v| serde_json::from_str::<Value>(&v).ok()),
            cwd: self.cwd,
            user_id: self.user_id,
            enabled: self.enabled == 1,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
