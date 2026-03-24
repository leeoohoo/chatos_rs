use mongodb::bson::{doc, Bson, Document};

use super::normalize_optional_text;

pub(crate) fn normalize_project_scope(project_id: Option<String>) -> String {
    normalize_optional_text(project_id.as_deref()).unwrap_or_else(|| "0".to_string())
}

fn metadata_text(metadata: Option<&serde_json::Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalize_optional_text(cursor.as_str())
}

pub(crate) fn contact_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_text(metadata, &["contact", "contact_id"])
        .or_else(|| metadata_text(metadata, &["contact", "contactId"]))
        .or_else(|| metadata_text(metadata, &["ui_contact", "contact_id"]))
        .or_else(|| metadata_text(metadata, &["ui_contact", "contactId"]))
        .or_else(|| metadata_text(metadata, &["chat_runtime", "contact_id"]))
        .or_else(|| metadata_text(metadata, &["chat_runtime", "contactId"]))
}

pub(crate) fn agent_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_text(metadata, &["contact", "agent_id"])
        .or_else(|| metadata_text(metadata, &["contact", "agentId"]))
        .or_else(|| metadata_text(metadata, &["ui_contact", "agent_id"]))
        .or_else(|| metadata_text(metadata, &["ui_contact", "agentId"]))
        .or_else(|| metadata_text(metadata, &["ui_chat_selection", "selected_agent_id"]))
        .or_else(|| metadata_text(metadata, &["ui_chat_selection", "selectedAgentId"]))
        .or_else(|| metadata_text(metadata, &["chat_runtime", "contact_agent_id"]))
        .or_else(|| metadata_text(metadata, &["chat_runtime", "contactAgentId"]))
}

fn set_metadata_text(metadata: &mut serde_json::Value, scope: &str, key: &str, value: &str) {
    let Some(root) = metadata.as_object_mut() else {
        return;
    };
    let entry = root
        .entry(scope.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !entry.is_object() {
        *entry = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(map) = entry.as_object_mut() {
        map.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

pub(crate) fn normalize_session_metadata(
    metadata: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let contact_id = contact_id_from_metadata(metadata.as_ref());
    let agent_id = agent_id_from_metadata(metadata.as_ref());

    if contact_id.is_none() && agent_id.is_none() {
        return metadata;
    }

    let mut normalized = match metadata {
        Some(serde_json::Value::Object(map)) => serde_json::Value::Object(map),
        Some(_) | None => serde_json::Value::Object(serde_json::Map::new()),
    };

    if let Some(contact_id) = contact_id.as_deref() {
        set_metadata_text(&mut normalized, "contact", "contact_id", contact_id);
        set_metadata_text(&mut normalized, "ui_contact", "contact_id", contact_id);
    }
    if let Some(agent_id) = agent_id.as_deref() {
        set_metadata_text(&mut normalized, "contact", "agent_id", agent_id);
        set_metadata_text(&mut normalized, "ui_contact", "agent_id", agent_id);
        set_metadata_text(
            &mut normalized,
            "ui_chat_selection",
            "selected_agent_id",
            agent_id,
        );
    }

    Some(normalized)
}

pub(crate) fn is_duplicate_key_error(err: &mongodb::error::Error) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("e11000") || text.contains("duplicate key")
}

pub(crate) fn build_contact_or_conditions(
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Vec<Document> {
    let mut out = Vec::new();
    if let Some(contact_id) = normalize_optional_text(contact_id) {
        out.push(doc! {"metadata.contact.contact_id": contact_id.clone()});
        out.push(doc! {"metadata.contact.contactId": contact_id.clone()});
        out.push(doc! {"metadata.ui_contact.contact_id": contact_id.clone()});
        out.push(doc! {"metadata.ui_contact.contactId": contact_id.clone()});
        out.push(doc! {"metadata.chat_runtime.contact_id": contact_id.clone()});
        out.push(doc! {"metadata.chat_runtime.contactId": contact_id.clone()});
        out.push(doc! {"contact_id": contact_id.clone()});
        out.push(doc! {"contactId": contact_id});
    }
    if let Some(agent_id) = normalize_optional_text(agent_id) {
        out.push(doc! {"metadata.contact.agent_id": agent_id.clone()});
        out.push(doc! {"metadata.contact.agentId": agent_id.clone()});
        out.push(doc! {"metadata.ui_contact.agent_id": agent_id.clone()});
        out.push(doc! {"metadata.ui_contact.agentId": agent_id.clone()});
        out.push(doc! {"metadata.ui_chat_selection.selected_agent_id": agent_id.clone()});
        out.push(doc! {"metadata.ui_chat_selection.selectedAgentId": agent_id.clone()});
        out.push(doc! {"metadata.chat_runtime.contact_agent_id": agent_id.clone()});
        out.push(doc! {"metadata.chat_runtime.contactAgentId": agent_id.clone()});
        out.push(doc! {"selected_agent_id": agent_id.clone()});
        out.push(doc! {"selectedAgentId": agent_id});
    }
    out
}

pub(crate) fn project_scope_condition(project_id: &str) -> Document {
    if project_id == "0" {
        doc! {
            "$or": [
                {"project_id": "0"},
                {"project_id": Bson::Null},
                {"project_id": ""},
                {"project_id": {"$exists": false}}
            ]
        }
    } else {
        doc! {"project_id": project_id}
    }
}

pub(crate) fn insert_project_scope_filter(filter: &mut Document, project_id: &str) {
    if project_id == "0" {
        filter.insert("$and", vec![project_scope_condition(project_id)]);
    } else {
        filter.insert("project_id", project_id);
    }
}

pub(crate) fn agent_lookup_conditions(agent_id: &str) -> Vec<Document> {
    vec![
        doc! {"metadata.contact.agent_id": agent_id},
        doc! {"metadata.contact.agentId": agent_id},
        doc! {"metadata.ui_contact.agent_id": agent_id},
        doc! {"metadata.ui_contact.agentId": agent_id},
        doc! {"metadata.ui_chat_selection.selected_agent_id": agent_id},
        doc! {"metadata.ui_chat_selection.selectedAgentId": agent_id},
        doc! {"metadata.chat_runtime.contact_agent_id": agent_id},
        doc! {"metadata.chat_runtime.contactAgentId": agent_id},
        doc! {"selected_agent_id": agent_id},
        doc! {"selectedAgentId": agent_id},
    ]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{agent_id_from_metadata, agent_lookup_conditions, normalize_session_metadata};

    #[test]
    fn reads_agent_id_from_legacy_metadata_keys() {
        let metadata = json!({
            "ui_chat_selection": {
                "selectedAgentId": "agent_legacy"
            }
        });
        let parsed = agent_id_from_metadata(Some(&metadata));
        assert_eq!(parsed.as_deref(), Some("agent_legacy"));
    }

    #[test]
    fn normalize_session_metadata_promotes_legacy_agent_key() {
        let metadata = json!({
            "ui_chat_selection": {
                "selectedAgentId": "agent_001"
            }
        });
        let normalized = normalize_session_metadata(Some(metadata))
            .expect("normalized metadata should exist");
        assert_eq!(
            normalized
                .get("contact")
                .and_then(|value| value.get("agent_id"))
                .and_then(serde_json::Value::as_str),
            Some("agent_001")
        );
        assert_eq!(
            normalized
                .get("ui_contact")
                .and_then(|value| value.get("agent_id"))
                .and_then(serde_json::Value::as_str),
            Some("agent_001")
        );
    }

    #[test]
    fn agent_lookup_conditions_cover_legacy_and_runtime_paths() {
        let rows = agent_lookup_conditions("agent_look");
        assert!(rows.iter().any(|row| {
            row.get_str("metadata.ui_chat_selection.selectedAgentId")
                .map(|value| value == "agent_look")
                .unwrap_or(false)
        }));
        assert!(rows.iter().any(|row| {
            row.get_str("metadata.chat_runtime.contactAgentId")
                .map(|value| value == "agent_look")
                .unwrap_or(false)
        }));
        assert!(rows.iter().any(|row| {
            row.get_str("selected_agent_id")
                .map(|value| value == "agent_look")
                .unwrap_or(false)
        }));
    }
}
