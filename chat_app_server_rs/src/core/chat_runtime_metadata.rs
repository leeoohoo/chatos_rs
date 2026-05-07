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
    pub mcp_enabled: Option<bool>,
    #[serde(default)]
    pub enabled_mcp_ids: Vec<String>,
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub fn normalize_id(value: Option<String>) -> Option<String> {
    normalize_optional_string(value)
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

pub fn metadata_string_list(metadata: Option<&Value>, path: &[&str]) -> Vec<String> {
    let mut cursor = match metadata {
        Some(value) => value,
        None => return Vec::new(),
    };
    for key in path {
        let Some(next) = cursor.get(*key) else {
            return Vec::new();
        };
        cursor = next;
    }
    let Some(items) = cursor.as_array() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for item in items {
        let Some(raw) = item.as_str() else {
            continue;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn metadata_string_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| metadata_string(metadata, path))
}

fn metadata_bool_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Option<bool> {
    paths.iter().find_map(|path| metadata_bool(metadata, path))
}

fn metadata_string_list_aliases(metadata: Option<&Value>, paths: &[&[&str]]) -> Vec<String> {
    paths
        .iter()
        .find_map(|path| {
            let values = metadata_string_list(metadata, path);
            if values.is_empty() {
                None
            } else {
                Some(values)
            }
        })
        .unwrap_or_default()
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
                ],
            ),
            project_id: metadata_string_aliases(
                metadata,
                &[
                    &["chat_runtime", "project_id"],
                    &["chat_runtime", "projectId"],
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
            mcp_enabled: metadata_bool_aliases(
                metadata,
                &[
                    &["chat_runtime", "mcp_enabled"],
                    &["chat_runtime", "mcpEnabled"],
                ],
            ),
            enabled_mcp_ids: metadata_string_list_aliases(
                metadata,
                &[
                    &["chat_runtime", "enabled_mcp_ids"],
                    &["chat_runtime", "enabledMcpIds"],
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
