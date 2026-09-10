// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::terminals as repo;

pub const TERMINAL_KIND_SHARED: &str = "shared";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Terminal {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub kind: String,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub process_id: Option<i64>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: String,
}

impl Terminal {
    pub fn new(
        name: String,
        cwd: String,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Terminal {
        let now = crate::core::time::now_rfc3339();
        Terminal {
            id: Uuid::new_v4().to_string(),
            name,
            cwd,
            kind: TERMINAL_KIND_SHARED.to_string(),
            user_id,
            project_id,
            process_id: None,
            status: "running".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            last_active_at: now,
        }
    }
}

pub struct TerminalService;

impl TerminalService {
    pub async fn get_by_id(id: &str) -> Result<Option<Terminal>, String> {
        repo::get_terminal_by_id(id).await
    }

    pub async fn list(user_id: Option<String>) -> Result<Vec<Terminal>, String> {
        repo::list_terminals_by_kind(user_id, TERMINAL_KIND_SHARED).await
    }

    pub async fn touch(id: &str) -> Result<(), String> {
        repo::touch_terminal(id).await
    }

    pub async fn delete(id: &str) -> Result<(), String> {
        repo::delete_terminal(id).await
    }
}
