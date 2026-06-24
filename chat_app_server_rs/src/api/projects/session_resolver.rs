use crate::core::chat_runtime::{contact_id_from_metadata, project_id_from_metadata};
use crate::models::project::PUBLIC_PROJECT_ID;
use crate::models::session::Session;
use crate::services::chatos_sessions;

pub(super) async fn resolve_project_contact_session_id(
    user_id: &str,
    project_id: &str,
    contact_id: &str,
) -> Option<(String, Option<String>)> {
    let normalized_user_id = user_id.trim();
    let normalized_project_id = project_id.trim();
    let normalized_contact_id = contact_id.trim();
    if normalized_user_id.is_empty()
        || normalized_project_id.is_empty()
        || normalized_contact_id.is_empty()
    {
        return None;
    }

    let mut candidates = collect_matching_sessions(
        normalized_user_id,
        Some(normalized_project_id),
        normalized_project_id,
        normalized_contact_id,
    )
    .await?;
    if candidates.is_empty() {
        candidates = collect_matching_sessions(
            normalized_user_id,
            None,
            normalized_project_id,
            normalized_contact_id,
        )
        .await?;
    }

    candidates.sort_by(|left, right| {
        let left_has_messages = left.message_count > 0;
        let right_has_messages = right.message_count > 0;
        right_has_messages
            .cmp(&left_has_messages)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });

    candidates
        .into_iter()
        .next()
        .map(|session| (session.id, Some(session.updated_at)))
}

async fn collect_matching_sessions(
    user_id: &str,
    project_filter: Option<&str>,
    project_id: &str,
    contact_id: &str,
) -> Option<Vec<Session>> {
    let mut candidates = Vec::new();
    let page_size = 500;
    for page in 0..20 {
        let sessions = chatos_sessions::list_sessions(
            Some(user_id),
            project_filter,
            Some(page_size),
            page * page_size,
            false,
            false,
        )
        .await
        .ok()?;
        if sessions.is_empty() {
            break;
        }
        let loaded = sessions.len();
        for session in sessions {
            if session.message_count > 0
                && session_matches_project_contact(&session, project_id, contact_id)
            {
                candidates.push(session);
            }
        }
        if loaded < page_size as usize {
            break;
        }
    }

    Some(candidates)
}

fn session_matches_project_contact(session: &Session, project_id: &str, contact_id: &str) -> bool {
    let metadata = session.metadata.as_ref();
    let session_project_id = session
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| project_id_from_metadata(metadata))
        .map(normalize_project_id)
        .unwrap_or_else(|| PUBLIC_PROJECT_ID.to_string());

    session_project_id == normalize_project_id(project_id.to_string())
        && contact_id_from_metadata(metadata).as_deref() == Some(contact_id)
}

fn normalize_project_id(value: String) -> String {
    if value.trim() == "0" {
        PUBLIC_PROJECT_ID.to_string()
    } else {
        value
    }
}
