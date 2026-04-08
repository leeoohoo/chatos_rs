use std::collections::{HashMap, HashSet};
use std::path::Path;

use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use sqlx::Row;

use crate::core::mongo_cursor::collect_documents;
use crate::repositories::db::with_db;

use super::path_support::{
    increment_kind_count, is_newer_record, is_unconfirmed_doc, normalize_change_kind,
    normalize_path, resolve_project_path_for_project, should_include_record,
};
use super::session_meta::load_session_meta_map;
use super::{
    ProjectChangeCounts, ProjectChangeMark, ProjectChangeSummary, ProjectScopedChangeRecord,
};

pub async fn list_unconfirmed_project_changes(
    project_id: &str,
    project_root: &str,
) -> Result<Vec<ProjectScopedChangeRecord>, String> {
    let project_id = project_id.to_string();
    let project_root = project_root.to_string();
    with_db(
        |db| {
            let project_id = project_id.clone();
            let project_root = project_root.clone();
            Box::pin(async move {
                let filter = doc! {
                    "$or": [
                        { "confirmed": { "$exists": false } },
                        { "confirmed": false },
                        { "confirmed": 0 }
                    ]
                };
                let options = FindOptions::builder().sort(doc! { "created_at": -1 }).build();
                let cursor = db
                    .collection::<Document>("mcp_change_logs")
                    .find(filter, options)
                    .await
                    .map_err(|e| e.to_string())?;
                let docs = collect_documents(cursor).await?;

                let mut session_ids: HashSet<String> = HashSet::new();
                for doc in &docs {
                    if let Ok(session_id) = doc.get_str("session_id") {
                        let trimmed = session_id.trim();
                        if !trimmed.is_empty() {
                            session_ids.insert(trimmed.to_string());
                        }
                    }
                }

                let mut session_projects: HashMap<String, String> = HashMap::new();
                if !session_ids.is_empty() {
                    let session_meta_map = load_session_meta_map(&session_ids).await;
                    for (session_id, meta) in session_meta_map {
                        if let Some(project_id) = meta.project_id {
                            session_projects.insert(session_id, project_id);
                        }
                    }
                }

                let mut out: Vec<ProjectScopedChangeRecord> = Vec::new();
                for doc in docs {
                    if !is_unconfirmed_doc(&doc) {
                        continue;
                    }
                    let id = doc.get_str("id").unwrap_or("").trim().to_string();
                    if id.is_empty() {
                        continue;
                    }
                    let record_project_id = doc
                        .get_str("project_id")
                        .ok()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    let raw_path = doc.get_str("path").unwrap_or("").trim().to_string();
                    if raw_path.is_empty() {
                        continue;
                    }
                    let session_id = doc.get_str("session_id").ok().map(|v| v.trim().to_string());
                    let session_project_id = session_id
                        .as_ref()
                        .and_then(|sid| session_projects.get(sid))
                        .map(String::as_str);
                    let action = doc.get_str("action").unwrap_or("");
                    let kind_raw = doc.get_str("change_kind").ok();
                    let kind = normalize_change_kind(kind_raw, action);
                    let resolved = resolve_project_path_for_project(&project_root, &raw_path);
                    if !should_include_record(
                        &project_id,
                        record_project_id.as_deref(),
                        session_project_id,
                        session_id.as_deref(),
                        &raw_path,
                        &kind,
                        resolved.as_ref(),
                        &project_root,
                    ) {
                        continue;
                    }
                    let Some(resolved) = resolved else {
                        continue;
                    };
                    out.push(ProjectScopedChangeRecord {
                        id,
                        path: resolved.absolute_path,
                        relative_path: resolved.relative_path,
                        kind,
                        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
                    });
                }
                Ok(out)
            })
        },
        |pool| {
            let project_id = project_id.clone();
            let project_root = project_root.clone();
            Box::pin(async move {
                let rows = sqlx::query(
                    r#"SELECT c.id, c.project_id, c.path, c.action, c.change_kind, c.created_at, c.session_id
                    FROM mcp_change_logs c
                    WHERE COALESCE(c.confirmed, 0) = 0
                    ORDER BY c.created_at DESC"#,
                )
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;

                let mut session_ids: HashSet<String> = HashSet::new();
                for row in &rows {
                    let session_id = row
                        .try_get::<Option<String>, _>("session_id")
                        .unwrap_or(None)
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Some(sid) = session_id {
                        session_ids.insert(sid);
                    }
                }
                let session_meta_map = load_session_meta_map(&session_ids).await;

                let mut out: Vec<ProjectScopedChangeRecord> = Vec::new();
                for row in rows {
                    let id: String = row.try_get("id").unwrap_or_default();
                    let id = id.trim().to_string();
                    if id.is_empty() {
                        continue;
                    }
                    let record_project_id: Option<String> =
                        row.try_get::<Option<String>, _>("project_id").unwrap_or(None);
                    let raw_path: String = row.try_get("path").unwrap_or_default();
                    let raw_path = raw_path.trim().to_string();
                    if raw_path.is_empty() {
                        continue;
                    }
                    let session_id: Option<String> = row
                        .try_get::<Option<String>, _>("session_id")
                        .unwrap_or(None)
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    let session_project_id = session_id
                        .as_ref()
                        .and_then(|sid| session_meta_map.get(sid))
                        .and_then(|meta| meta.project_id.as_deref());
                    let action: String = row.try_get("action").unwrap_or_default();
                    let kind_raw: Option<String> = row.try_get("change_kind").ok();
                    let kind = normalize_change_kind(kind_raw.as_deref(), action.as_str());
                    let resolved = resolve_project_path_for_project(&project_root, &raw_path);
                    if !should_include_record(
                        &project_id,
                        record_project_id.as_deref(),
                        session_project_id,
                        session_id.as_deref(),
                        &raw_path,
                        &kind,
                        resolved.as_ref(),
                        &project_root,
                    ) {
                        continue;
                    }
                    let Some(resolved) = resolved else {
                        continue;
                    };
                    let created_at: String = row.try_get("created_at").unwrap_or_default();
                    out.push(ProjectScopedChangeRecord {
                        id,
                        path: resolved.absolute_path,
                        relative_path: resolved.relative_path,
                        kind,
                        created_at,
                    });
                }
                Ok(out)
            })
        },
    )
    .await
}

pub fn summarize_project_changes(records: &[ProjectScopedChangeRecord]) -> ProjectChangeSummary {
    let mut latest_by_path: HashMap<String, ProjectScopedChangeRecord> = HashMap::new();
    for record in records {
        let key = normalize_path(&record.path);
        if key.is_empty() {
            continue;
        }
        match latest_by_path.get(&key) {
            Some(existing) => {
                if is_newer_record(record, existing) {
                    latest_by_path.insert(key, record.clone());
                }
            }
            None => {
                latest_by_path.insert(key, record.clone());
            }
        }
    }

    let mut file_marks: Vec<ProjectChangeMark> = Vec::new();
    let mut deleted_marks: Vec<ProjectChangeMark> = Vec::new();
    let mut counts = ProjectChangeCounts::default();

    for record in latest_by_path.into_values() {
        let kind = normalize_change_kind(Some(record.kind.as_str()), "");
        let mark = ProjectChangeMark {
            path: record.path.clone(),
            relative_path: record.relative_path.clone(),
            kind: kind.clone(),
            last_change_id: record.id.clone(),
            updated_at: record.created_at.clone(),
        };
        let exists = Path::new(&record.path).exists();
        if kind == "delete" && !exists {
            deleted_marks.push(mark);
        } else {
            file_marks.push(mark);
        }
        increment_kind_count(&mut counts, &kind);
    }

    file_marks.sort_by(|a, b| a.path.cmp(&b.path));
    deleted_marks.sort_by(|a, b| a.path.cmp(&b.path));
    counts.total = counts.create + counts.edit + counts.delete;

    ProjectChangeSummary {
        file_marks,
        deleted_marks,
        counts,
    }
}
