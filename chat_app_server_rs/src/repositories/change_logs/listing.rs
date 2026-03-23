use std::collections::{HashMap, HashSet};

use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use sqlx::Row;

use crate::core::mongo_cursor::collect_documents;
use crate::core::sql_query::append_limit_offset_clause;
use crate::repositories::db::with_db;

use super::path_support::{
    build_normalized_paths, build_path_regexes, normalize_change_kind, path_to_sql_like,
};
use super::session_meta::{load_session_meta_map, normalize_doc};
use super::ChangeLogItem;

pub async fn list_project_change_logs(
    _project_id: &str,
    paths: Option<Vec<String>>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<ChangeLogItem>, String> {
    with_db(
        |db| {
            let paths = paths.clone();
            let limit = limit;
            Box::pin(async move {
                let list = match paths {
                    Some(v) if !v.is_empty() => v,
                    _ => return Ok(Vec::new()),
                };

                let regex_values = build_path_regexes(&list);

                let mut options = FindOptions::builder().sort(doc! { "created_at": -1 }).build();
                if let Some(l) = limit {
                    options.limit = Some(l);
                }
                if offset > 0 {
                    options.skip = Some(offset as u64);
                }

                let mut regex_filters: Vec<Document> = Vec::new();
                for pattern in &regex_values {
                    regex_filters.push(doc! {
                        "path": {
                            "$regex": pattern,
                            "$options": if cfg!(windows) { "i" } else { "" }
                        }
                    });
                }
                let filter = if regex_filters.is_empty() {
                    doc! { "path": { "$in": list } }
                } else {
                    doc! { "$or": regex_filters }
                };

                let cursor = db
                    .collection::<Document>("mcp_change_logs")
                    .find(filter, options)
                    .await
                    .map_err(|e| e.to_string())?;
                let out_docs = collect_documents(cursor).await?;

                let mut session_ids: HashSet<String> = HashSet::new();
                for doc in &out_docs {
                    if let Ok(sid) = doc.get_str("session_id") {
                        let sid = sid.trim();
                        if !sid.is_empty() {
                            session_ids.insert(sid.to_string());
                        }
                    }
                }

                let mut session_titles: HashMap<String, String> = HashMap::new();
                if !session_ids.is_empty() {
                    let session_meta_map = load_session_meta_map(&session_ids).await;
                    for (session_id, meta) in session_meta_map {
                        if let Some(title) = meta.title {
                            session_titles.insert(session_id, title);
                        }
                    }
                }

                let out = out_docs
                    .iter()
                    .map(|doc| normalize_doc(doc, &session_titles))
                    .collect();
                Ok(out)
            })
        },
        |pool| {
            let paths = paths.clone();
            let limit = limit;
            Box::pin(async move {
                let list = match paths {
                    Some(v) if !v.is_empty() => v,
                    _ => return Ok(Vec::new()),
                };

                let normalized_paths = build_normalized_paths(&list);
                if normalized_paths.is_empty() {
                    return Ok(Vec::new());
                }

                let mut where_parts: Vec<String> = Vec::new();
                for _ in &normalized_paths {
                    if cfg!(windows) {
                        where_parts.push(r"LOWER(REPLACE(c.path, '\', '/')) = LOWER(?)".to_string());
                    } else {
                        where_parts.push(r"REPLACE(c.path, '\', '/') = ?".to_string());
                    }
                }
                for _ in &normalized_paths {
                    if cfg!(windows) {
                        where_parts.push(
                            r"LOWER(REPLACE(c.path, '\', '/')) LIKE LOWER(?) ESCAPE '!'"
                                .to_string(),
                        );
                    } else {
                        where_parts.push(r"REPLACE(c.path, '\', '/') LIKE ? ESCAPE '!'".to_string());
                    }
                }

                let mut query = format!(
                    "SELECT c.id, c.server_name, c.project_id, c.path, c.action, c.change_kind, c.bytes, c.sha256, c.diff, c.session_id, c.run_id, c.confirmed, c.confirmed_at, c.confirmed_by, c.created_at \
                    FROM mcp_change_logs c \
                    WHERE {}",
                    where_parts.join(" OR "),
                );
                query.push_str(" ORDER BY c.created_at DESC");
                append_limit_offset_clause(&mut query, limit, offset);

                let mut q = sqlx::query(&query);
                for path in &normalized_paths {
                    q = q.bind(path);
                }
                for path in &normalized_paths {
                    q = q.bind(path_to_sql_like(path));
                }
                if let Some(l) = limit {
                    q = q.bind(l);
                    if offset > 0 {
                        q = q.bind(offset);
                    }
                }

                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                let mut session_ids: HashSet<String> = HashSet::new();
                for row in rows {
                    let id: String = row.try_get("id").unwrap_or_default();
                    let server_name: String = row.try_get("server_name").unwrap_or_default();
                    let project_id: Option<String> = row.try_get("project_id").ok();
                    let path_val: String = row.try_get("path").unwrap_or_default();
                    let action: String = row.try_get("action").unwrap_or_default();
                    let change_kind_raw: Option<String> = row.try_get("change_kind").ok();
                    let change_kind =
                        normalize_change_kind(change_kind_raw.as_deref(), action.as_str());
                    let bytes: i64 = row.try_get("bytes").unwrap_or(0);
                    let sha256: Option<String> = row.try_get("sha256").ok();
                    let diff: Option<String> = row.try_get("diff").ok();
                    let session_id = row
                        .try_get::<Option<String>, _>("session_id")
                        .unwrap_or(None)
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Some(sid) = session_id.as_ref() {
                        session_ids.insert(sid.clone());
                    }
                    let run_id: Option<String> = row.try_get("run_id").ok();
                    let confirmed_raw: Option<i64> = row.try_get("confirmed").ok();
                    let confirmed_at: Option<String> = row.try_get("confirmed_at").ok();
                    let confirmed_by: Option<String> = row.try_get("confirmed_by").ok();
                    let created_at: String = row.try_get("created_at").unwrap_or_default();
                    out.push(ChangeLogItem {
                        id,
                        server_name,
                        project_id,
                        path: path_val,
                        action,
                        change_kind,
                        bytes,
                        sha256,
                        diff,
                        session_id,
                        run_id,
                        confirmed: confirmed_raw.unwrap_or(0) != 0,
                        confirmed_at,
                        confirmed_by,
                        created_at,
                        session_title: None,
                    });
                }
                if !session_ids.is_empty() {
                    let session_meta_map = load_session_meta_map(&session_ids).await;
                    for item in &mut out {
                        let Some(session_id) = item.session_id.as_ref() else {
                            continue;
                        };
                        if let Some(meta) = session_meta_map.get(session_id) {
                            item.session_title = meta.title.clone();
                        }
                    }
                }
                Ok(out)
            })
        },
    )
    .await
}
