// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::history::CommandHistoryEntry;
use crate::sandbox::types::LocalSandboxState;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct LocalState {
    #[serde(default)]
    pub(crate) auth: Option<AuthState>,
    #[serde(default)]
    pub(crate) paired_cloud_base_url: Option<String>,
    #[serde(default)]
    pub(crate) paired_user_id: Option<String>,
    pub(crate) device_id: Option<String>,
    pub(crate) device_public_key: Option<String>,
    #[serde(default)]
    pub(crate) workspaces: Vec<WorkspaceState>,
    #[serde(default)]
    pub(crate) sandbox: LocalSandboxState,
    #[serde(default)]
    pub(crate) command_history: Vec<CommandHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthState {
    pub(crate) cloud_base_url: String,
    pub(crate) user_service_base_url: String,
    pub(crate) access_token: String,
    pub(crate) device_name: String,
    pub(crate) user: Option<AuthUserState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthUserState {
    pub(crate) id: String,
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkspaceState {
    pub(crate) id: String,
    pub(crate) absolute_root: PathBuf,
    pub(crate) alias: String,
    pub(crate) fingerprint: String,
}

impl LocalState {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("read state file {}", path.display()))?;
        serde_json::from_str(content.as_str())
            .with_context(|| format!("parse state file {}", path.display()))
    }

    pub(crate) fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create state dir {}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).with_context(|| format!("write state file {}", path.display()))
    }

    pub(crate) fn workspace_by_id(&self, workspace_id: &str) -> Option<&WorkspaceState> {
        self.workspaces
            .iter()
            .find(|workspace| workspace.id == workspace_id)
    }

    pub(crate) fn workspace_index_by_fingerprint(&self, fingerprint: &str) -> Option<usize> {
        self.workspaces
            .iter()
            .position(|workspace| workspace.fingerprint == fingerprint)
    }

    pub(crate) fn pairing_context_matches(&self, cloud_base_url: &str, user_id: &str) -> bool {
        let stored_cloud_base_url = self
            .paired_cloud_base_url
            .as_deref()
            .or_else(|| self.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()));
        let stored_user_id = self.paired_user_id.as_deref().or_else(|| {
            self.auth
                .as_ref()
                .and_then(|auth| auth.user.as_ref().map(|user| user.id.as_str()))
        });
        matches!(
            (stored_cloud_base_url, stored_user_id),
            (Some(stored_cloud_base_url), Some(stored_user_id))
                if stored_cloud_base_url == cloud_base_url && stored_user_id == user_id
        )
    }
}
