use std::collections::{HashMap, HashSet};

use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use serde::Serialize;
use sqlx::Row;

use crate::core::mongo_cursor::collect_documents;
use crate::core::sql_query::append_limit_offset_clause;
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
    _project_id: &str,
    paths: Option<Vec<String>>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<ChangeLogItem>, String> {
    with_db(
        |db| {
            let paths = paths.clone();
            let limit = limit.clone();
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
                    let missing_sessions: Vec<Bson> = session_ids
                        .into_iter()
                        .map(Bson::String)
                        .collect();
                    let title_cursor = db
                        .collection::<Document>("sessions")
                        .find(doc! { "id": { "$in": missing_sessions } }, None)
                        .await
                        .map_err(|e| e.to_string())?;
                    let title_docs = collect_documents(title_cursor).await?;
                    for doc in title_docs {
                        let id = doc.get_str("id").unwrap_or("").trim().to_string();
                        if id.is_empty() {
                            continue;
                        }
                        let title = doc.get_str("title").unwrap_or("").to_string();
                        session_titles.insert(id, title);
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
                        where_parts
                            .push(r"LOWER(REPLACE(c.path, '\', '/')) = LOWER(?)".to_string());
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
                    "SELECT c.id, c.server_name, c.path, c.action, c.bytes, c.sha256, c.diff, c.session_id, c.run_id, c.created_at, s.title as session_title \
                    FROM mcp_change_logs c \
                    LEFT JOIN sessions s ON s.id = c.session_id \
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

fn build_path_regexes(paths: &[String]) -> Vec<String> {
    let normalized = build_normalized_paths(paths);
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for value in normalized {
        let pattern = path_to_regex(&value);
        if seen.insert(pattern.clone()) {
            out.push(pattern);
        }
    }
    out
}

fn build_normalized_paths(paths: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for raw in paths {
        let normalized = normalize_path(raw);
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

fn normalize_path(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    if normalized.len() > 1 {
        normalized = normalized.trim_end_matches('/').to_string();
    }
    normalized
}

fn path_to_regex(path: &str) -> String {
    let escaped = regex::escape(path);
    let slash_flexible = escaped.replace('/', r"[\\/]");
    format!(r"(^|[\\/]){}$", slash_flexible)
}

fn path_to_sql_like(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    let mut escaped = String::new();
    for ch in trimmed.chars() {
        match ch {
            '!' | '%' | '_' => {
                escaped.push('!');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    format!("%/{}", escaped)
}
