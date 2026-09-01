// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::core::chat_runtime::{
    contact_agent_id_from_metadata as runtime_contact_agent_id_from_metadata,
    contact_id_from_metadata as runtime_contact_id_from_metadata, normalize_project_id,
    project_id_from_metadata as runtime_project_id_from_metadata,
};

pub fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(ToOwned::to_owned)
}

pub fn normalize_project_scope(project_id: Option<&str>) -> Option<String> {
    normalize_project_id(project_id)
}

pub fn resolve_session_project_scope(
    project_id: Option<&str>,
    metadata: Option<&Value>,
) -> Option<String> {
    normalize_optional_text(project_id).or_else(|| runtime_project_id_from_metadata(metadata))
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
    fn normalizes_missing_project_scope_to_none() {
        assert_eq!(normalize_project_scope(None), None);
        assert_eq!(normalize_project_scope(Some("")), None);
        assert_eq!(
            normalize_project_scope(Some(" project_1 ")).as_deref(),
            Some("project_1")
        );
    }

    #[test]
    fn rejects_legacy_global_project_sentinels() {
        let metadata = json!({
            "chat_runtime": {
                "project_id": "0"
            }
        });

        assert_eq!(normalize_project_scope(Some("-1")), None);
        assert_eq!(normalize_project_scope(Some("0")), None);
        assert_eq!(resolve_session_project_scope(None, Some(&metadata)), None);
    }
}
