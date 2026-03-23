use std::collections::{HashMap, HashSet};

use mongodb::bson::Document;

use crate::services::memory_server_client;

use super::path_support::{normalize_change_kind, parse_doc_bool};
use super::ChangeLogItem;

#[derive(Debug, Clone, Default)]
pub(super) struct SessionMetaLite {
    pub(super) title: Option<String>,
    pub(super) project_id: Option<String>,
}

pub(super) async fn load_session_meta_map(
    session_ids: &HashSet<String>,
) -> HashMap<String, SessionMetaLite> {
    let mut out: HashMap<String, SessionMetaLite> = HashMap::new();
    if session_ids.is_empty() {
        return out;
    }

    for session_id in session_ids {
        match memory_server_client::get_session_by_id(session_id).await {
            Ok(Some(session)) => {
                let title = session.title.trim().to_string();
                let project_id = session
                    .project_id
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                out.insert(
                    session_id.clone(),
                    SessionMetaLite {
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
    session_titles: &HashMap<String, String>,
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
    let session_id = doc.get_str("session_id").ok().map(|s| s.to_string());
    let run_id = doc.get_str("run_id").ok().map(|s| s.to_string());
    let confirmed = parse_doc_bool(doc.get("confirmed")).unwrap_or(false);
    let confirmed_at = doc.get_str("confirmed_at").ok().map(|s| s.to_string());
    let confirmed_by = doc.get_str("confirmed_by").ok().map(|s| s.to_string());
    let created_at = doc.get_str("created_at").unwrap_or("").to_string();
    let session_title = session_id
        .as_ref()
        .and_then(|id| session_titles.get(id))
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
        session_id,
        run_id,
        confirmed,
        confirmed_at,
        confirmed_by,
        created_at,
        session_title,
    }
}
