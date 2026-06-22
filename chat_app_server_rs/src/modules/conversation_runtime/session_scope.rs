use serde_json::Value;

use crate::core::chat_runtime::{
    contact_agent_id_from_metadata as runtime_contact_agent_id_from_metadata,
    contact_id_from_metadata as runtime_contact_id_from_metadata,
    project_id_from_metadata as runtime_project_id_from_metadata,
};

pub fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(ToOwned::to_owned)
}

pub fn normalize_project_scope(project_id: Option<&str>) -> String {
    normalize_optional_text(project_id).unwrap_or_else(|| "0".to_string())
}

pub fn resolve_session_project_scope(project_id: Option<&str>, metadata: Option<&Value>) -> String {
    normalize_optional_text(project_id)
        .or_else(|| runtime_project_id_from_metadata(metadata))
        .unwrap_or_else(|| "0".to_string())
}

pub fn contact_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    runtime_contact_id_from_metadata(metadata)
}

pub fn contact_agent_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    runtime_contact_agent_id_from_metadata(metadata)
}
