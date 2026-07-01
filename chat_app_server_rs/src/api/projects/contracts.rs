// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub(super) struct ProjectRunCustomToolchainRequest {
    pub(super) kind: Option<String>,
    pub(super) label: Option<String>,
    pub(super) path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectQuery {
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateProjectRequest {
    pub(super) name: Option<String>,
    pub(super) root_path: Option<String>,
    pub(super) git_url: Option<String>,
    pub(super) description: Option<String>,
    pub(super) user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateProjectRequest {
    pub(super) name: Option<String>,
    pub(super) root_path: Option<String>,
    pub(super) git_url: Option<String>,
    pub(super) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectContactsQuery {
    pub(super) limit: Option<i64>,
    pub(super) offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AddProjectContactRequest {
    pub(super) contact_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectRunExecuteRequest {
    pub(super) target_id: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) command: Option<String>,
    pub(super) create_if_missing: Option<bool>,
    pub(super) terminal_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectRunDefaultRequest {
    pub(super) target_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectRunEnvironmentUpdateRequest {
    pub(super) selected_toolchains: Option<HashMap<String, String>>,
    pub(super) custom_toolchains: Option<HashMap<String, ProjectRunCustomToolchainRequest>>,
    pub(super) env_vars: Option<HashMap<String, String>>,
    pub(super) terminal_ui_enabled: Option<bool>,
}
