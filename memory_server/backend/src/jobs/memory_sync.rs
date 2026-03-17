use serde_json::Value;

use crate::db::Db;
use crate::models::SessionSummary;
use crate::repositories::{memories, sessions};

fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn metadata_string(metadata: Option<&Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalized_text(cursor.as_str())
}

fn contact_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "contact_id"])
        .or_else(|| metadata_string(metadata, &["ui_contact", "contact_id"]))
}

fn agent_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "agent_id"])
        .or_else(|| metadata_string(metadata, &["ui_contact", "agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selected_agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selectedAgentId"]))
}

fn project_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    metadata_string(metadata, &["chat_runtime", "project_id"])
        .or_else(|| metadata_string(metadata, &["chat_runtime", "projectId"]))
}

pub async fn sync_memories_from_summary(
    pool: &Db,
    session_id: &str,
    summary: &SessionSummary,
) -> Result<(), String> {
    let Some(session) = sessions::get_session_by_id(pool, session_id).await? else {
        return Ok(());
    };

    let metadata = session.metadata.as_ref();
    let user_id = session.user_id.trim().to_string();
    if user_id.is_empty() {
        return Ok(());
    }

    let contact_id = contact_id_from_metadata(metadata);
    let agent_id = agent_id_from_metadata(metadata);
    let project_id = normalized_text(session.project_id.as_deref())
        .or_else(|| project_id_from_metadata(metadata));

    if let (Some(contact_id), Some(agent_id), Some(project_id)) =
        (contact_id.clone(), agent_id.clone(), project_id.clone())
    {
        let _ = memories::upsert_project_memory(
            pool,
            memories::UpsertProjectMemoryInput {
                user_id: user_id.clone(),
                contact_id,
                agent_id,
                project_id,
                memory_text: summary.summary_text.clone(),
                last_source_at: Some(summary.created_at.clone()),
            },
        )
        .await?;
    }

    Ok(())
}
