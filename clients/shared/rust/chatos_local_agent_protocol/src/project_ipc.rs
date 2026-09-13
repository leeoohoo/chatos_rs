// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{require_identifier, ProtocolError};

const MAXIMUM_PROJECT_NAME_BYTES: usize = 1_024;
const MAXIMUM_PROJECT_DESCRIPTION_BYTES: usize = 64 * 1_024;
const MAXIMUM_RELATIVE_ROOT_BYTES: usize = 4 * 1_024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectDraft {
    pub name: String,
    pub description: String,
    pub workspace_id: String,
    pub relative_root: String,
}

impl LocalProjectDraft {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_project_text("project_name", &self.name, MAXIMUM_PROJECT_NAME_BYTES, true)?;
        require_project_text(
            "project_description",
            &self.description,
            MAXIMUM_PROJECT_DESCRIPTION_BYTES,
            false,
        )?;
        require_identifier("workspace_id", &self.workspace_id)?;
        if self.workspace_id.contains(['/', '\\'])
            || matches!(self.workspace_id.as_str(), "." | "..")
        {
            return Err(ProtocolError::InvalidState {
                reason: "workspace_id must be an opaque route identifier",
            });
        }
        validate_relative_root(&self.relative_root)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalProjectStatus {
    Active,
    Archived,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectSnapshot {
    pub project_id: String,
    pub owner_user_id: String,
    pub draft: LocalProjectDraft,
    pub revision: u64,
    pub status: LocalProjectStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LocalProjectSnapshot {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("project_id", &self.project_id)?;
        require_identifier("owner_user_id", &self.owner_user_id)?;
        self.draft.validate()?;
        if self.revision == 0 || self.updated_at < self.created_at {
            return Err(ProtocolError::InvalidState {
                reason: "project revision and timestamps are invalid",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ListProjectsCommand {
    pub cursor: Option<String>,
    pub limit: u32,
    pub include_inactive: bool,
}

impl ListProjectsCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(cursor) = &self.cursor {
            require_identifier("cursor", cursor)?;
        }
        if self.limit == 0 || self.limit > 500 {
            return Err(ProtocolError::InvalidState {
                reason: "project page limit must be between 1 and 500",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GetProjectCommand {
    pub project_id: String,
}

impl GetProjectCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("project_id", &self.project_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectCommand {
    pub project_id: String,
    pub draft: LocalProjectDraft,
}

impl CreateProjectCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("project_id", &self.project_id)?;
        self.draft.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpdateProjectCommand {
    pub project_id: String,
    pub expected_revision: u64,
    pub draft: LocalProjectDraft,
    pub status: LocalProjectStatus,
}

impl UpdateProjectCommand {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("project_id", &self.project_id)?;
        if self.expected_revision == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "project expected_revision must be positive",
            });
        }
        self.draft.validate()
    }
}

fn require_project_text(
    field: &'static str,
    value: &str,
    maximum: usize,
    require_nonempty: bool,
) -> Result<(), ProtocolError> {
    if (require_nonempty && value.is_empty())
        || value.trim() != value
        || value.len() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(ProtocolError::InvalidState {
            reason: match field {
                "project_name" => "project name is invalid",
                _ => "project description is invalid",
            },
        });
    }
    Ok(())
}

fn validate_relative_root(value: &str) -> Result<(), ProtocolError> {
    let invalid = value.trim() != value
        || value.len() > MAXIMUM_RELATIVE_ROOT_BYTES
        || value.contains(['\\', ':'])
        || value.chars().any(char::is_control)
        || (!value.is_empty()
            && value
                .split('/')
                .any(|segment| segment.is_empty() || matches!(segment, "." | "..")));
    if invalid {
        Err(ProtocolError::InvalidState {
            reason: "project relative_root must be a canonical portable relative path",
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(relative_root: &str) -> LocalProjectDraft {
        LocalProjectDraft {
            name: "Project".to_string(),
            description: String::new(),
            workspace_id: "workspace-1".to_string(),
            relative_root: relative_root.to_string(),
        }
    }

    #[test]
    fn relative_root_accepts_portable_paths_only() {
        for accepted in ["", "apps/site", "项目/site", "space here"] {
            assert!(draft(accepted).validate().is_ok(), "{accepted}");
        }
        for rejected in [
            "/absolute",
            "../escape",
            "a/../b",
            "a/./b",
            "a//b",
            "a/",
            "C:/repo",
            "a\\b",
        ] {
            assert!(draft(rejected).validate().is_err(), "{rejected}");
        }
    }
}
