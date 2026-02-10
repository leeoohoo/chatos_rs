use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::terminal_logs as repo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalLog {
    pub id: String,
    pub terminal_id: String,
    pub log_type: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, FromRow)]
pub struct TerminalLogRow {
    pub id: String,
    pub terminal_id: String,
    pub log_type: String,
    pub content: String,
    pub created_at: String,
}

impl TerminalLogRow {
    pub fn to_log(self) -> TerminalLog {
        TerminalLog {
            id: self.id,
            terminal_id: self.terminal_id,
            log_type: self.log_type,
            content: self.content,
            created_at: self.created_at,
        }
    }
}

impl TerminalLog {
    pub fn new(terminal_id: String, log_type: String, content: String) -> TerminalLog {
        TerminalLog {
            id: Uuid::new_v4().to_string(),
            terminal_id,
            log_type,
            content,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

pub struct TerminalLogService;

impl TerminalLogService {
    pub async fn create(log: TerminalLog) -> Result<String, String> {
        repo::create_terminal_log(&log).await
    }

    pub async fn list(
        terminal_id: &str,
        limit: Option<i64>,
        offset: i64,
    ) -> Result<Vec<TerminalLog>, String> {
        repo::list_terminal_logs(terminal_id, limit, offset).await
    }

    pub async fn delete_by_terminal(terminal_id: &str) -> Result<(), String> {
        repo::delete_terminal_logs(terminal_id).await
    }
}
