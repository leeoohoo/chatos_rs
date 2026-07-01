// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::utils::{generate_id, now_iso, resolve_state_dir};
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone)]
pub struct ChangeLogStore {
    path: PathBuf,
    server_name: String,
    project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeRecord {
    pub id: String,
    pub server_name: String,
    pub project_id: Option<String>,
    pub path: String,
    pub action: String,
    pub change_kind: String,
    pub bytes: i64,
    pub sha256: String,
    pub diff: Option<String>,
    pub conversation_id: String,
    pub run_id: String,
    pub confirmed: bool,
    pub confirmed_at: Option<String>,
    pub confirmed_by: Option<String>,
    pub created_at: String,
}

impl ChangeLogStore {
    pub fn new(
        server_name: &str,
        project_id: Option<String>,
        db_path: Option<String>,
    ) -> Result<Self, String> {
        let project_id = project_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let path = db_path
            .map(PathBuf::from)
            .unwrap_or_else(|| default_jsonl_path(server_name));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        Ok(Self {
            path,
            server_name: server_name.to_string(),
            project_id,
        })
    }

    pub fn log_change(
        &self,
        path: &str,
        action: &str,
        change_kind: &str,
        bytes: i64,
        sha256: &str,
        conversation_id: &str,
        run_id: &str,
        diff: Option<String>,
    ) -> Result<ChangeRecord, String> {
        let record = ChangeRecord {
            id: generate_id("change"),
            server_name: self.server_name.clone(),
            project_id: self.project_id.clone(),
            path: path.to_string(),
            action: action.to_string(),
            change_kind: change_kind.to_string(),
            bytes,
            sha256: sha256.to_string(),
            diff,
            conversation_id: conversation_id.to_string(),
            run_id: run_id.to_string(),
            confirmed: false,
            confirmed_at: None,
            confirmed_by: None,
            created_at: now_iso(),
        };
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|err| err.to_string())?;
        let line = serde_json::to_string(&record).map_err(|err| err.to_string())?;
        file.write_all(line.as_bytes())
            .map_err(|err| err.to_string())?;
        file.write_all(b"\n").map_err(|err| err.to_string())?;
        Ok(record)
    }
}

fn default_jsonl_path(server_name: &str) -> PathBuf {
    let state_dir = resolve_state_dir(server_name);
    state_dir.join(format!("{server_name}.changes.jsonl"))
}
