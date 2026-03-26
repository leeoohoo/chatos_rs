use serde_json::Value;

use crate::models::session::Session;

use super::MemorySession;

pub fn map_memory_session(value: MemorySession) -> Session {
    let (selected_model_id, selected_agent_id) =
        extract_selection_from_session_metadata(value.metadata.as_ref());
    Session {
        id: value.id,
        title: value.title.unwrap_or_else(|| "Untitled".to_string()),
        description: None,
        metadata: value.metadata,
        selected_model_id,
        selected_agent_id,
        user_id: Some(value.user_id),
        project_id: value.project_id,
        status: value.status,
        archived_at: value.archived_at,
        created_at: value.created_at,
        updated_at: value.updated_at,
    }
}

fn extract_selection_from_session_metadata(
    metadata: Option<&Value>,
) -> (Option<String>, Option<String>) {
    let Some(Value::Object(metadata_map)) = metadata else {
        return (None, None);
    };
    let selected_model_id = metadata_map
        .get("chat_runtime")
        .and_then(Value::as_object)
        .and_then(|runtime| {
            runtime
                .get("selected_model_id")
                .or_else(|| runtime.get("selectedModelId"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            metadata_map
                .get("ui_chat_selection")
                .and_then(Value::as_object)
                .and_then(|selection| {
                    selection
                        .get("selected_model_id")
                        .or_else(|| selection.get("selectedModelId"))
                })
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });

    let selected_agent_id = metadata_map
        .get("contact")
        .and_then(Value::as_object)
        .and_then(|contact| contact.get("agent_id").or_else(|| contact.get("agentId")))
        .or_else(|| {
            metadata_map
                .get("chat_runtime")
                .and_then(Value::as_object)
                .and_then(|runtime| {
                    runtime
                        .get("contact_agent_id")
                        .or_else(|| runtime.get("contactAgentId"))
                })
        })
        .or_else(|| {
            metadata_map
                .get("ui_contact")
                .and_then(Value::as_object)
                .and_then(|contact| contact.get("agent_id").or_else(|| contact.get("agentId")))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            metadata_map
                .get("ui_chat_selection")
                .and_then(Value::as_object)
                .and_then(|selection| {
                    selection
                        .get("selected_agent_id")
                        .or_else(|| selection.get("selectedAgentId"))
                })
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });

    (selected_model_id, selected_agent_id)
}
