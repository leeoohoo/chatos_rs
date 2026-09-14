// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{require_identifier, ProtocolError};

const MAXIMUM_PROJECT_NAME_BYTES: usize = 1_024;
const MAXIMUM_PROJECT_DESCRIPTION_BYTES: usize = 64 * 1_024;
const MAXIMUM_ROOT_PATH_BYTES: usize = 16 * 1_024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectDraft {
    pub name: String,
    pub description: String,
    pub root_path: String,
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
        validate_root_path(&self.root_path)
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

fn validate_root_path(value: &str) -> Result<(), ProtocolError> {
    let invalid = value.trim() != value
        || value.is_empty()
        || value.len() > MAXIMUM_ROOT_PATH_BYTES
        || !value.starts_with('/')
        || value.contains("//")
        || (value.len() > 1 && value.ends_with('/'))
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."));
    if invalid {
        Err(ProtocolError::InvalidState {
            reason: "project root_path must be an absolute normalized path",
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(root_path: &str) -> LocalProjectDraft {
        LocalProjectDraft {
            name: "Project".to_string(),
            description: String::new(),
            root_path: root_path.to_string(),
        }
    }

    #[test]
    fn root_path_accepts_absolute_normalized_paths_only() {
        for accepted in ["/", "/apps/site", "/项目/site", "/space here"] {
            assert!(draft(accepted).validate().is_ok(), "{accepted}");
        }
        for rejected in [
            "",
            "relative",
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
