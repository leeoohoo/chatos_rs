use std::collections::HashMap;

use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use serde::Serialize;
use sqlx::Row;

use crate::repositories::db::with_db;

#[derive(Debug, Clone, Serialize)]
pub struct ChangeLogItem {
    pub id: String,
    pub server_name: String,
    pub path: String,
    pub action: String,
    pub bytes: i64,
    pub sha256: Option<String>,
    pub diff: Option<String>,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub created_at: String,
    pub session_title: Option<String>,
}

pub async fn list_project_change_logs(
    project_id: &str,
    paths: Option<Vec<String>>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<ChangeLogItem>, String> {
    let project_id = project_id.to_string();
    with_db(
        |db| {
            let project_id = project_id.clone();
            let paths = paths.clone();
            let limit = limit.clone();
            Box::pin(async move {
                let mut session_cursor = db
                    .collection::<Document>("sessions")
                    .find(doc! { "project_id": &project_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;

                let mut session_titles: HashMap<String, String> = HashMap::new();
                let mut session_ids: Vec<Bson> = Vec::new();
                while let Some(doc) = session_cursor.try_next().await.map_err(|e| e.to_string())? {
                    let id = doc.get_str("id").unwrap_or("").to_string();
                    if id.is_empty() {
                        continue;
                    }
                    let title = doc.get_str("title").unwrap_or("").to_string();
                    session_titles.insert(id.clone(), title);
                    session_ids.push(Bson::String(id));
                }

                let mut options = FindOptions::builder().sort(doc! { "created_at": -1 }).build();
                if let Some(l) = limit {
                    options.limit = Some(l);
                }
                if offset > 0 {
                    options.skip = Some(offset as u64);
                }

                if session_ids.is_empty() {
                    let list = match paths {
                        Some(v) if !v.is_empty() => v,
                        _ => return Ok(Vec::new()),
                    };
                    let filter = doc! { "path": { "$in": list.clone() } };
                    let mut cursor = db
                        .collection::<Document>("mcp_change_logs")
                        .find(filter, options)
                        .await
                        .map_err(|e| e.to_string())?;
                    let mut out_docs = Vec::new();
                    let mut missing_sessions: Vec<Bson> = Vec::new();
                    while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                        if let Ok(sid) = doc.get_str("session_id") {
                            if !sid.trim().is_empty() {
                                missing_sessions.push(Bson::String(sid.to_string()));
                            }
                        }
                        out_docs.push(doc);
                    }
                    if !missing_sessions.is_empty() {
                        let mut title_cursor = db
                            .collection::<Document>("sessions")
                            .find(doc! { "id": { "$in": missing_sessions } }, None)
                            .await
                            .map_err(|e| e.to_string())?;
                        while let Some(doc) = title_cursor.try_next().await.map_err(|e| e.to_string())? {
                            let id = doc.get_str("id").unwrap_or("").to_string();
                            if id.is_empty() {
                                continue;
                            }
                            let title = doc.get_str("title").unwrap_or("").to_string();
                            session_titles.insert(id, title);
                        }
                    }
                    let mut out = Vec::new();
                    for doc in out_docs {
                        out.push(normalize_doc(&doc, &session_titles));
                    }
                    return Ok(out);
                }

                let mut filter = doc! { "session_id": { "$in": session_ids } };
                if let Some(list) = paths {
                    if !list.is_empty() {
                        filter.insert("path", doc! { "$in": list });
                    }
                }

                let mut cursor = db
                    .collection::<Document>("mcp_change_logs")
                    .find(filter, options)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    out.push(normalize_doc(&doc, &session_titles));
                }
                Ok(out)
            })
        },
        |pool| {
            let project_id = project_id.clone();
            let paths = paths.clone();
            Box::pin(async move {
                let session_rows = sqlx::query("SELECT id, title FROM sessions WHERE project_id = ?")
                    .bind(&project_id)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut has_sessions = false;
                for row in session_rows {
                    let id: String = row.try_get("id").unwrap_or_default();
                    if !id.is_empty() {
                        has_sessions = true;
                        break;
                    }
                }

                let mut query = String::from(
                    "SELECT c.id, c.server_name, c.path, c.action, c.bytes, c.sha256, c.diff, c.session_id, c.run_id, c.created_at, s.title as session_title \
                    FROM mcp_change_logs c \
                    LEFT JOIN sessions s ON s.id = c.session_id \
                    WHERE s.project_id = ?",
                );
                if !has_sessions {
                    query = String::from(
                        "SELECT c.id, c.server_name, c.path, c.action, c.bytes, c.sha256, c.diff, c.session_id, c.run_id, c.created_at, s.title as session_title \
                        FROM mcp_change_logs c \
                        LEFT JOIN sessions s ON s.id = c.session_id \
                        WHERE 1 = 1",
                    );
                }
                if let Some(ref list) = paths {
                    if !list.is_empty() {
                        let placeholders = std::iter::repeat("?").take(list.len()).collect::<Vec<_>>().join(", ");
                        query.push_str(&format!(" AND c.path IN ({})", placeholders));
                    } else if !has_sessions {
                        return Ok(Vec::new());
                    }
                } else if !has_sessions {
                    return Ok(Vec::new());
                }
                query.push_str(" ORDER BY c.created_at DESC");
                if let Some(_l) = limit {
                    query.push_str(" LIMIT ?");
                    if offset > 0 {
                        query.push_str(" OFFSET ?");
                    }
                }

                let mut q = sqlx::query(&query);
                if has_sessions {
                    q = q.bind(&project_id);
                }
                if let Some(ref list) = paths {
                    for p in list {
                        q = q.bind(p);
                    }
                }
                if let Some(l) = limit {
                    q = q.bind(l);
                    if offset > 0 {
                        q = q.bind(offset);
                    }
                }

                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for row in rows {
                    let id: String = row.try_get("id").unwrap_or_default();
                    let server_name: String = row.try_get("server_name").unwrap_or_default();
                    let path_val: String = row.try_get("path").unwrap_or_default();
                    let action: String = row.try_get("action").unwrap_or_default();
                    let bytes: i64 = row.try_get("bytes").unwrap_or(0);
                    let sha256: Option<String> = row.try_get("sha256").ok();
                    let diff: Option<String> = row.try_get("diff").ok();
                    let session_id: Option<String> = row.try_get("session_id").ok();
                    let run_id: Option<String> = row.try_get("run_id").ok();
                    let created_at: String = row.try_get("created_at").unwrap_or_default();
                    let session_title: Option<String> = row.try_get("session_title").ok();
                    out.push(ChangeLogItem {
                        id,
                        server_name,
                        path: path_val,
                        action,
                        bytes,
                        sha256,
                        diff,
                        session_id,
                        run_id,
                        created_at,
                        session_title,
                    });
                }
                Ok(out)
            })
        },
    )
    .await
}

fn normalize_doc(doc: &Document, session_titles: &HashMap<String, String>) -> ChangeLogItem {
    let id = doc.get_str("id").unwrap_or("").to_string();
    let server_name = doc.get_str("server_name").unwrap_or("").to_string();
    let path = doc.get_str("path").unwrap_or("").to_string();
    let action = doc.get_str("action").unwrap_or("").to_string();
    let bytes = match doc.get_i64("bytes") {
        Ok(v) => v,
        Err(_) => doc.get_i32("bytes").map(|v| v as i64).unwrap_or(0),
    };
    let sha256 = doc.get_str("sha256").ok().map(|s| s.to_string());
    let diff = doc.get_str("diff").ok().map(|s| s.to_string());
    let session_id = doc.get_str("session_id").ok().map(|s| s.to_string());
    let run_id = doc.get_str("run_id").ok().map(|s| s.to_string());
    let created_at = doc.get_str("created_at").unwrap_or("").to_string();
    let session_title = session_id
        .as_ref()
        .and_then(|id| session_titles.get(id))
        .cloned();

    ChangeLogItem {
        id,
        server_name,
        path,
        action,
        bytes,
        sha256,
        diff,
        session_id,
        run_id,
        created_at,
        session_title,
    }
}
