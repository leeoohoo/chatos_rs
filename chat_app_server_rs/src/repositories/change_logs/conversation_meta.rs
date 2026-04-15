use std::collections::{HashMap, HashSet};

use mongodb::bson::Document;

use crate::services::memory_server_client;

use super::path_support::{normalize_change_kind, parse_doc_bool};
use super::ChangeLogItem;

#[derive(Debug, Clone, Default)]
pub(super) struct ConversationMetaLite {
    pub(super) title: Option<String>,
    pub(super) project_id: Option<String>,
}

pub(super) async fn load_conversation_meta_map(
    conversation_ids: &HashSet<String>,
) -> HashMap<String, ConversationMetaLite> {
    let mut out: HashMap<String, ConversationMetaLite> = HashMap::new();
    if conversation_ids.is_empty() {
        return out;
    }

    for conversation_id in conversation_ids {
        match memory_server_client::get_session_by_id(conversation_id).await {
            Ok(Some(conversation)) => {
                let title = conversation.title.trim().to_string();
                let project_id = conversation
                    .project_id
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                out.insert(
                    conversation_id.clone(),
                    ConversationMetaLite {
                        title: if title.is_empty() { None } else { Some(title) },
                        project_id,
                    },
                );
            }
            Ok(None) => {}
            Err(_) => {}
        }
    }

    out
}

pub(super) fn normalize_doc(
    doc: &Document,
    conversation_titles: &HashMap<String, String>,
) -> ChangeLogItem {
    let id = doc.get_str("id").unwrap_or("").to_string();
    let server_name = doc.get_str("server_name").unwrap_or("").to_string();
    let project_id = doc.get_str("project_id").ok().map(|s| s.to_string());
    let path = doc.get_str("path").unwrap_or("").to_string();
    let action = doc.get_str("action").unwrap_or("").to_string();
    let change_kind_raw = doc.get_str("change_kind").ok();
    let change_kind = normalize_change_kind(change_kind_raw, action.as_str());
    let bytes = match doc.get_i64("bytes") {
        Ok(v) => v,
        Err(_) => doc.get_i32("bytes").map(|v| v as i64).unwrap_or(0),
    };
    let sha256 = doc.get_str("sha256").ok().map(|s| s.to_string());
    let diff = doc.get_str("diff").ok().map(|s| s.to_string());
    let conversation_id = doc.get_str("conversation_id").ok().map(|s| s.to_string());
    let run_id = doc.get_str("run_id").ok().map(|s| s.to_string());
    let confirmed = parse_doc_bool(doc.get("confirmed")).unwrap_or(false);
    let confirmed_at = doc.get_str("confirmed_at").ok().map(|s| s.to_string());
    let confirmed_by = doc.get_str("confirmed_by").ok().map(|s| s.to_string());
    let created_at = doc.get_str("created_at").unwrap_or("").to_string();
    let conversation_title = conversation_id
        .as_ref()
        .and_then(|id| conversation_titles.get(id))
        .cloned();

    ChangeLogItem {
        id,
        server_name,
        project_id,
        path,
        action,
        change_kind,
        bytes,
        sha256,
        diff,
        conversation_id,
        run_id,
        confirmed,
        confirmed_at,
        confirmed_by,
        created_at,
        conversation_title,
    }
}
