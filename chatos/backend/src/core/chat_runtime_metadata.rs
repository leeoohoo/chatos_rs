// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRuntimeMetadata {
    pub contact_agent_id: Option<String>,
    pub contact_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub workspace_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub auto_create_task: Option<bool>,
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub fn normalize_id(value: Option<String>) -> Option<String> {
    normalize_optional_string(value)
}

/// Project scope is optional. Historical clients encoded the global scope as
/// `-1` (and briefly `0`); treating those sentinels as real project IDs leaks
/// the transport workaround into authorization, memory labels, and task
/// routing. Normalize them at the boundary so the rest of the runtime only
/// sees a concrete project ID or `None`.
pub fn normalize_project_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| !matches!(*value, "-1" | "0"))
        .map(ToOwned::to_owned)
}

pub fn metadata_string(metadata: Option<&Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalize_optional_string(cursor.as_str().map(ToOwned::to_owned))
}

pub fn metadata_bool(metadata: Option<&Value>, path: &[&str]) -> Option<bool> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_bool()
}

fn metadata_string_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Option<String> {
    paths
        .iter()
        .find_map(|path| metadata_string_with_source(metadata, path))
}

fn metadata_project_id_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        metadata_string_with_source(metadata, path)
            .and_then(|value| normalize_project_id(Some(value.as_str())))
    })
}

fn metadata_bool_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Option<bool> {
    paths
        .iter()
        .find_map(|path| metadata_bool_with_source(metadata, path))
}

fn with_source_metadata_prefix<'a>(path: &'a [&'a str]) -> Vec<&'a str> {
    let mut source_path = Vec::with_capacity(path.len() + 1);
    source_path.push("source_metadata");
    source_path.extend_from_slice(path);
    source_path
}

fn metadata_string_with_source(metadata: Option<&Value>, path: &[&str]) -> Option<String> {
    metadata_string(metadata, path).or_else(|| {
        let source_path = with_source_metadata_prefix(path);
        metadata_string(metadata, source_path.as_slice())
    })
}

fn metadata_bool_with_source(metadata: Option<&Value>, path: &[&str]) -> Option<bool> {
    metadata_bool(metadata, path).or_else(|| {
        let source_path = with_source_metadata_prefix(path);
        metadata_bool(metadata, source_path.as_slice())
    })
}

impl ChatRuntimeMetadata {
    pub fn from_metadata(metadata: Option<&Value>) -> Self {
        Self {
            contact_agent_id: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "contact_agent_id"],
                    &["chat_runtime", "contactAgentId"],
                    &["contact", "agent_id"],
                    &["contact", "agentId"],
                    &["ui_contact", "agent_id"],
                    &["ui_contact", "agentId"],
                    &["ui_chat_selection", "selected_agent_id"],
                    &["ui_chat_selection", "selectedAgentId"],
                    &["legacy_session_mapping", "agent_id"],
                    &["legacy_session_mapping", "agentId"],
                ],
            ),
            contact_id: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "contact_id"],
                    &["chat_runtime", "contactId"],
                    &["contact", "contact_id"],
                    &["contact", "contactId"],
                    &["ui_contact", "contact_id"],
                    &["ui_contact", "contactId"],
                    &["legacy_session_mapping", "contact_id"],
                    &["legacy_session_mapping", "contactId"],
                ],
            ),
            project_id: metadata_project_id_aliases(
                metadata,
                &[
                    &["chat_runtime", "project_id"],
                    &["chat_runtime", "projectId"],
                    &["legacy_session_mapping", "project_id"],
                    &["legacy_session_mapping", "projectId"],
                ],
            ),
            project_root: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "project_root"],
                    &["chat_runtime", "projectRoot"],
                ],
            ),
            workspace_root: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "workspace_root"],
                    &["chat_runtime", "workspaceRoot"],
                ],
            ),
            remote_connection_id: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "remote_connection_id"],
                    &["chat_runtime", "remoteConnectionId"],
                ],
            ),
            auto_create_task: metadata_bool_aliases(
                metadata,
                &[
                    &["chat_runtime", "auto_create_task"],
                    &["chat_runtime", "autoCreateTask"],
                ],
            ),
        }
    }
}

pub fn contact_agent_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).contact_agent_id
}

pub fn contact_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).contact_id
}

pub fn project_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    ChatRuntimeMetadata::from_metadata(metadata).project_id
}
