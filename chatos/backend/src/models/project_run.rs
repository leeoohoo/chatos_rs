// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRunTarget {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub language: Option<String>,
    pub cwd: String,
    pub command: String,
    pub source: String,
    pub confidence: f64,
    pub is_default: bool,
    pub entrypoint: Option<String>,
    pub manifest_path: Option<String>,
    pub required_toolchains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRunCatalog {
    pub project_id: String,
    pub user_id: Option<String>,
    pub status: String,
    pub default_target_id: Option<String>,
    pub targets: Vec<ProjectRunTarget>,
    pub error_message: Option<String>,
    pub analyzed_at: Option<String>,
    pub updated_at: String,
}
