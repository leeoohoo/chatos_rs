// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::repositories::terminals as repo;

pub const TERMINAL_KIND_SHARED: &str = "shared";
pub const TERMINAL_KIND_PROJECT_RUN: &str = "project_run";

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

#[derive(Debug, FromRow)]
pub struct TerminalRow {
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

impl TerminalRow {
    pub fn to_terminal(self) -> Terminal {
        Terminal {
            id: self.id,
            name: self.name,
            cwd: self.cwd,
            kind: normalize_terminal_kind(Some(self.kind)),
            user_id: self.user_id,
            project_id: self.project_id,
            process_id: self.process_id,
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_active_at: self.last_active_at,
        }
    }
}

impl Terminal {
    pub fn new(
        name: String,
        cwd: String,
        kind: String,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Terminal {
        let now = crate::core::time::now_rfc3339();
        Terminal {
            id: Uuid::new_v4().to_string(),
            name,
            cwd,
            kind: normalize_terminal_kind(Some(kind)),
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

pub fn normalize_terminal_kind(value: Option<String>) -> String {
    match value
        .as_deref()
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
    {
        Some(TERMINAL_KIND_PROJECT_RUN) => TERMINAL_KIND_PROJECT_RUN.to_string(),
        _ => TERMINAL_KIND_SHARED.to_string(),
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

    pub async fn list_by_kind(
        user_id: Option<String>,
        kind: &str,
    ) -> Result<Vec<Terminal>, String> {
        repo::list_terminals_by_kind(user_id, kind).await
    }

    pub async fn get_project_run_by_project_id(
        user_id: Option<String>,
        project_id: &str,
    ) -> Result<Option<Terminal>, String> {
        repo::get_project_run_terminal_by_project_id(user_id, project_id).await
    }

    pub async fn list_project_runs_by_project_id(
        user_id: Option<String>,
        project_id: &str,
    ) -> Result<Vec<Terminal>, String> {
        repo::list_project_run_terminals_by_project_id(user_id, project_id).await
    }

    pub async fn touch(id: &str) -> Result<(), String> {
        repo::touch_terminal(id).await
    }

    pub async fn delete(id: &str) -> Result<(), String> {
        repo::delete_terminal(id).await
    }
}
