use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Utc;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use serde::Serialize;
use sqlx::Row;

use crate::core::mongo_cursor::collect_documents;
use crate::core::sql_query::append_limit_offset_clause;
use crate::repositories::db::with_db;
use crate::services::memory_server_client;

#[derive(Debug, Clone, Serialize)]
pub struct ChangeLogItem {
    pub id: String,
    pub server_name: String,
    pub project_id: Option<String>,
    pub path: String,
    pub action: String,
    pub change_kind: String,
    pub bytes: i64,
    pub sha256: Option<String>,
    pub diff: Option<String>,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub confirmed: bool,
    pub confirmed_at: Option<String>,
    pub confirmed_by: Option<String>,
    pub created_at: String,
    pub session_title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectScopedChangeRecord {
    pub id: String,
    pub path: String,
    pub relative_path: String,
    pub kind: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectChangeMark {
    pub path: String,
    pub relative_path: String,
    pub kind: String,
    pub last_change_id: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectChangeCounts {
    pub create: usize,
    pub edit: usize,
    pub delete: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectChangeSummary {
    pub file_marks: Vec<ProjectChangeMark>,
    pub deleted_marks: Vec<ProjectChangeMark>,
    pub counts: ProjectChangeCounts,
}

#[derive(Debug, Clone, Default)]
struct SessionMetaLite {
    title: Option<String>,
    project_id: Option<String>,
}

async fn load_session_meta_map(session_ids: &HashSet<String>) -> HashMap<String, SessionMetaLite> {
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

fn normalize_doc(doc: &Document, session_titles: &HashMap<String, String>) -> ChangeLogItem {
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
                    let record_project_id: Option<String> = row
                        .try_get::<Option<String>, _>("project_id")
                        .unwrap_or(None);
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

pub async fn confirm_change_logs_by_ids(
    change_ids: &[String],
    confirmed_by: Option<&str>,
) -> Result<usize, String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut deduped: Vec<String> = Vec::new();
    for id in change_ids {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            deduped.push(trimmed.to_string());
        }
    }
    if deduped.is_empty() {
        return Ok(0);
    }

    let confirmed_by = confirmed_by
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let now = Utc::now().to_rfc3339();

    with_db(
        |db| {
            let deduped = deduped.clone();
            let confirmed_by = confirmed_by.clone();
            let now = now.clone();
            Box::pin(async move {
                let ids: Vec<Bson> = deduped.into_iter().map(Bson::String).collect();
                let mut set_doc = doc! {
                    "confirmed": true,
                    "confirmed_at": &now,
                };
                if let Some(user_id) = confirmed_by {
                    set_doc.insert("confirmed_by", user_id);
                } else {
                    set_doc.insert("confirmed_by", Bson::Null);
                }
                let filter = doc! {
                    "id": { "$in": ids },
                    "$or": [
                        { "confirmed": { "$exists": false } },
                        { "confirmed": false },
                        { "confirmed": 0 }
                    ]
                };
                let result = db
                    .collection::<Document>("mcp_change_logs")
                    .update_many(filter, doc! { "$set": set_doc }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(result.modified_count as usize)
            })
        },
        |pool| {
            let deduped = deduped.clone();
            let confirmed_by = confirmed_by.clone();
            let now = now.clone();
            Box::pin(async move {
                let placeholders = std::iter::repeat("?")
                    .take(deduped.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "UPDATE mcp_change_logs \
                    SET confirmed = 1, confirmed_at = ?, confirmed_by = ? \
                    WHERE COALESCE(confirmed, 0) = 0 AND id IN ({placeholders})"
                );
                let mut query = sqlx::query(&sql).bind(&now).bind(confirmed_by.as_deref());
                for id in &deduped {
                    query = query.bind(id);
                }
                let result = query.execute(pool).await.map_err(|e| e.to_string())?;
                Ok(result.rows_affected() as usize)
            })
        },
    )
    .await
}

fn increment_kind_count(counts: &mut ProjectChangeCounts, kind: &str) {
    match kind {
        "create" => counts.create += 1,
        "delete" => counts.delete += 1,
        _ => counts.edit += 1,
    }
}

fn is_newer_record(left: &ProjectScopedChangeRecord, right: &ProjectScopedChangeRecord) -> bool {
    match left.created_at.cmp(&right.created_at) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => left.id > right.id,
    }
}

fn normalize_change_kind(kind: Option<&str>, action: &str) -> String {
    let normalized = kind
        .map(|value| value.trim().to_lowercase())
        .unwrap_or_default();
    match normalized.as_str() {
        "create" | "edit" | "delete" => normalized,
        _ => {
            if action.eq_ignore_ascii_case("delete") {
                "delete".to_string()
            } else {
                "edit".to_string()
            }
        }
    }
}

fn parse_doc_bool(value: Option<&Bson>) -> Option<bool> {
    match value {
        Some(Bson::Boolean(v)) => Some(*v),
        Some(Bson::Int32(v)) => Some(*v != 0),
        Some(Bson::Int64(v)) => Some(*v != 0),
        Some(Bson::String(v)) => {
            let lower = v.trim().to_ascii_lowercase();
            match lower.as_str() {
                "1" | "true" | "yes" => Some(true),
                "0" | "false" | "no" => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

fn is_unconfirmed_doc(doc: &Document) -> bool {
    !parse_doc_bool(doc.get("confirmed")).unwrap_or(false)
}

#[derive(Debug, Clone)]
struct ResolvedProjectPath {
    absolute_path: String,
    relative_path: String,
}

fn resolve_project_path_for_project(
    project_root: &str,
    raw_path: &str,
) -> Option<ResolvedProjectPath> {
    let root = normalize_path(project_root);
    if root.is_empty() {
        return None;
    }
    let normalized_path = normalize_path(raw_path);
    if normalized_path.is_empty() {
        return None;
    }

    let relative = if path_looks_absolute(&normalized_path) {
        strip_path_prefix(&normalized_path, &root)?
    } else {
        let mut rel = normalized_path
            .trim_start_matches("./")
            .trim_start_matches('/')
            .to_string();
        if rel.is_empty() {
            return None;
        }
        if let Some(project_dir) = project_dir_name(&root) {
            if rel == project_dir {
                rel.clear();
            } else if let Some(stripped) = strip_path_prefix(&rel, &project_dir) {
                rel = stripped;
            }
        }
        rel
    };

    let relative = normalize_path(&relative);
    let absolute_path = if relative.is_empty() {
        root
    } else {
        join_paths(project_root, &relative)
    };
    Some(ResolvedProjectPath {
        absolute_path: normalize_path(&absolute_path),
        relative_path: relative,
    })
}

fn should_include_record(
    project_id: &str,
    record_project_id: Option<&str>,
    session_project_id: Option<&str>,
    session_id: Option<&str>,
    raw_path: &str,
    kind: &str,
    resolved: Option<&ResolvedProjectPath>,
    project_root: &str,
) -> bool {
    let Some(resolved) = resolved else {
        return false;
    };
    if let Some(pid) = record_project_id {
        let trimmed = pid.trim();
        if !trimmed.is_empty() {
            return trimmed == project_id;
        }
    }
    if let Some(pid) = session_project_id {
        let trimmed = pid.trim();
        if !trimmed.is_empty() {
            return trimmed == project_id;
        }
    }
    if is_path_hint_for_project(raw_path, project_root) {
        return true;
    }
    if kind != "delete" && Path::new(&resolved.absolute_path).exists() {
        return true;
    }
    if kind == "delete" {
        let raw = normalize_path(raw_path);
        if !raw.is_empty() && !path_looks_absolute(&raw) {
            let candidate = join_paths(project_root, &raw);
            if let Some(parent) = Path::new(&candidate).parent() {
                if parent.exists() {
                    return true;
                }
            }
        }
    }
    // session_id is currently unused in this fallback branch, keep it for future refinements
    let _ = session_id;
    false
}

fn is_path_hint_for_project(raw_path: &str, project_root: &str) -> bool {
    let normalized_path = normalize_path(raw_path);
    if normalized_path.is_empty() {
        return false;
    }
    let normalized_root = normalize_path(project_root);
    if normalized_root.is_empty() {
        return false;
    }
    if path_looks_absolute(&normalized_path) {
        return strip_path_prefix(&normalized_path, &normalized_root).is_some();
    }
    let Some(project_dir) = project_dir_name(&normalized_root) else {
        return false;
    };
    normalized_path == project_dir || normalized_path.starts_with(&format!("{project_dir}/"))
}

fn path_looks_absolute(path: &str) -> bool {
    if Path::new(path).is_absolute() {
        return true;
    }
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

fn project_dir_name(path: &str) -> Option<String> {
    normalize_path(path)
        .split('/')
        .filter(|part| !part.is_empty())
        .last()
        .map(|part| part.to_string())
}

fn strip_path_prefix(value: &str, prefix: &str) -> Option<String> {
    let normalized_value = normalize_path(value);
    let normalized_prefix = normalize_path(prefix);
    if normalized_prefix.is_empty() {
        return Some(normalized_value);
    }
    let value_parts: Vec<&str> = normalized_value
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let prefix_parts: Vec<&str> = normalized_prefix
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if prefix_parts.len() > value_parts.len() {
        return None;
    }
    let matched = value_parts
        .iter()
        .zip(prefix_parts.iter())
        .all(|(lhs, rhs)| path_part_eq(lhs, rhs));
    if !matched {
        return None;
    }
    Some(value_parts[prefix_parts.len()..].join("/"))
}

fn path_part_eq(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

fn join_paths(base: &str, tail: &str) -> String {
    let base = normalize_path(base);
    let tail = normalize_path(tail).trim_start_matches('/').to_string();
    if base.is_empty() {
        return tail;
    }
    if tail.is_empty() {
        return base;
    }
    format!("{}/{}", base.trim_end_matches('/'), tail)
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
