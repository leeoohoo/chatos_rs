use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_agent_id: Option<String>,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub status: String,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Session {
    pub fn new(
        title: String,
        description: Option<String>,
        metadata: Option<Value>,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Session {
        let now = crate::core::time::now_rfc3339();
        let (selected_model_id, selected_agent_id) =
            extract_selection_from_metadata(metadata.as_ref());
        Session {
            id: Uuid::new_v4().to_string(),
            title,
            description,
            metadata,
            selected_model_id,
            selected_agent_id,
            user_id,
            project_id,
            status: "active".to_string(),
            archived_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

fn extract_selection_from_metadata(metadata: Option<&Value>) -> (Option<String>, Option<String>) {
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
