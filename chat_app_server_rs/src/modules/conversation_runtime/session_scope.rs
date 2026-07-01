// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::core::chat_runtime::{
    contact_agent_id_from_metadata as runtime_contact_agent_id_from_metadata,
    contact_id_from_metadata as runtime_contact_id_from_metadata,
    project_id_from_metadata as runtime_project_id_from_metadata,
};
use crate::models::project::{normalize_project_id, PUBLIC_PROJECT_ID};

pub fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(ToOwned::to_owned)
}

pub fn normalize_project_scope(project_id: Option<&str>) -> String {
    normalize_optional_text(project_id)
        .map(|value| normalize_project_id(value.as_str()))
        .unwrap_or_else(|| PUBLIC_PROJECT_ID.to_string())
}

pub fn resolve_session_project_scope(project_id: Option<&str>, metadata: Option<&Value>) -> String {
    normalize_optional_text(project_id)
        .or_else(|| runtime_project_id_from_metadata(metadata))
        .map(|value| normalize_project_id(value.as_str()))
        .unwrap_or_else(|| PUBLIC_PROJECT_ID.to_string())
}

pub fn contact_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    runtime_contact_id_from_metadata(metadata)
}

pub fn contact_agent_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    runtime_contact_agent_id_from_metadata(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_empty_and_legacy_project_scope_to_public() {
        assert_eq!(normalize_project_scope(None), PUBLIC_PROJECT_ID);
        assert_eq!(normalize_project_scope(Some("")), PUBLIC_PROJECT_ID);
        assert_eq!(normalize_project_scope(Some(" 0 ")), PUBLIC_PROJECT_ID);
        assert_eq!(normalize_project_scope(Some(" project_1 ")), "project_1");
    }

    #[test]
    fn resolves_legacy_metadata_project_scope_to_public() {
        let metadata = json!({
            "chat_runtime": {
                "project_id": "0"
            }
        });

        assert_eq!(
            resolve_session_project_scope(None, Some(&metadata)),
            PUBLIC_PROJECT_ID
        );
    }
}
