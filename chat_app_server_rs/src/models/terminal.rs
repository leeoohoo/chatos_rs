use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::terminals as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Terminal {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub user_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: String,
}

#[derive(Debug, FromRow)]
pub struct TerminalRow {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub user_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: String,
}

impl TerminalRow {
    pub fn to_terminal(self) -> Terminal {
        Terminal {
            id: self.id,
            name: self.name,
            cwd: self.cwd,
            user_id: self.user_id,
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_active_at: self.last_active_at,
        }
    }
}

impl Terminal {
    pub fn new(name: String, cwd: String, user_id: Option<String>) -> Terminal {
        let now = chrono::Utc::now().to_rfc3339();
        Terminal {
            id: Uuid::new_v4().to_string(),
            name,
            cwd,
            user_id,
            status: "running".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            last_active_at: now,
        }
    }
}

pub struct TerminalService;

impl TerminalService {
    pub async fn create(data: Terminal) -> Result<String, String> {
        repo::create_terminal(&data).await
    }

    pub async fn get_by_id(id: &str) -> Result<Option<Terminal>, String> {
        repo::get_terminal_by_id(id).await
    }

    pub async fn list(user_id: Option<String>) -> Result<Vec<Terminal>, String> {
        repo::list_terminals(user_id).await
    }

    pub async fn update_status(
        id: &str,
        status: Option<String>,
        last_active_at: Option<String>,
    ) -> Result<(), String> {
        repo::update_terminal_status(id, status, last_active_at).await
    }

    pub async fn touch(id: &str) -> Result<(), String> {
        repo::touch_terminal(id).await
    }

    pub async fn delete(id: &str) -> Result<(), String> {
        repo::delete_terminal(id).await
    }
}
