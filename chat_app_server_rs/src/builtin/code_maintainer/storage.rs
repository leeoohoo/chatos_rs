use crate::db::{Database, get_db_sync};
use crate::models::project::ProjectService;
use crate::repositories::change_logs::{
    ProjectChangeCounts, ProjectChangeMark, ProjectChangeSummarySnapshot,
};
use crate::services::project_local_cache::write_cache_json;
use crate::services::project_run::classify_project_run_path_change;
use crate::services::realtime::{
    publish_project_change_summary_updated, publish_project_run_catalog_updated,
};
use mongodb::Database as MongoDatabase;
use mongodb::bson::doc;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tracing::warn;

use chatos_builtin_tools::code_maintainer::{generate_id, now_iso, resolve_state_dir};

const PROJECT_CHANGE_SUMMARY_CACHE_PATH: &str = "project_changes/summary.json";

#[derive(Clone)]
pub struct ChangeLogStore {
    backend: ChangeLogBackend,
    server_name: String,
    project_id: Option<String>,
}

#[derive(Clone)]
enum ChangeLogBackend {
    Jsonl { path: PathBuf },
    Sqlite { pool: SqlitePool },
    Mongo { db: MongoDatabase },
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeRecord {
    pub id: String,
    pub server_name: String,
    pub project_id: Option<String>,
    pub path: String,
    pub action: String,
    pub change_kind: String,
    pub bytes: i64,
    pub sha256: String,
    pub diff: Option<String>,
    pub conversation_id: String,
    pub run_id: String,
    pub confirmed: bool,
    pub confirmed_at: Option<String>,
    pub confirmed_by: Option<String>,
    pub created_at: String,
}

impl ChangeLogStore {
    pub fn new(
        server_name: &str,
        project_id: Option<String>,
        db_path: Option<String>,
    ) -> Result<Self, String> {
        let project_id = project_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(path) = db_path {
            let path = PathBuf::from(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            return Ok(Self {
                backend: ChangeLogBackend::Jsonl { path },
                server_name: server_name.to_string(),
                project_id: project_id.clone(),
            });
        }

        match get_db_sync() {
            Ok(adapter) => match &*adapter {
                Database::Sqlite(pool) => {
                    ensure_sqlite_table(pool)?;
                    Ok(Self {
                        backend: ChangeLogBackend::Sqlite { pool: pool.clone() },
                        server_name: server_name.to_string(),
                        project_id: project_id.clone(),
                    })
                }
                Database::Mongo { db, .. } => {
                    ensure_mongo_indexes(db)?;
                    Ok(Self {
                        backend: ChangeLogBackend::Mongo { db: db.clone() },
                        server_name: server_name.to_string(),
                        project_id: project_id.clone(),
                    })
                }
            },
            Err(err) => {
                warn!("[MCP] fallback to JSONL changelog: {err}");
                let path = default_jsonl_path(server_name);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                Ok(Self {
                    backend: ChangeLogBackend::Jsonl { path },
                    server_name: server_name.to_string(),
                    project_id: project_id.clone(),
                })
            }
        }
    }

    pub fn log_change(
        &self,
        path: &str,
        action: &str,
        change_kind: &str,
        bytes: i64,
        sha256: &str,
        conversation_id: &str,
        run_id: &str,
        diff: Option<String>,
    ) -> Result<ChangeRecord, String> {
        let record = ChangeRecord {
            id: generate_id("change"),
            server_name: self.server_name.clone(),
            project_id: self.project_id.clone(),
            path: path.to_string(),
            action: action.to_string(),
            change_kind: change_kind.to_string(),
            bytes,
            sha256: sha256.to_string(),
            diff,
            conversation_id: conversation_id.to_string(),
            run_id: run_id.to_string(),
            confirmed: false,
            confirmed_at: None,
            confirmed_by: None,
            created_at: now_iso(),
        };
        match &self.backend {
            ChangeLogBackend::Jsonl { path } => {
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|err| err.to_string())?;
                let line = serde_json::to_string(&record).map_err(|err| err.to_string())?;
                file.write_all(line.as_bytes())
                    .map_err(|err| err.to_string())?;
                file.write_all(b"\n").map_err(|err| err.to_string())?;
            }
            ChangeLogBackend::Sqlite { pool } => {
                run_async(sqlite_insert(pool.clone(), record.clone()))?;
            }
            ChangeLogBackend::Mongo { db } => {
                run_async(mongo_insert(db.clone(), record.clone()))?;
            }
        }
        if let Some(project_id) = record.project_id.as_deref() {
            publish_project_change_summary_for_record(project_id, &record);
        }
        Ok(record)
    }

    // list_changes intentionally omitted for the embedded builtin server.
}

fn publish_project_change_summary_for_record(project_id: &str, record: &ChangeRecord) {
    let Ok(_handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let project_id = project_id.to_string();
    let conversation_id = record.conversation_id.clone();
    let path = record.path.clone();
    let change_kind = record.change_kind.clone();
    let record_id = record.id.clone();
    let created_at = record.created_at.clone();
    tokio::spawn(async move {
        let Some(project) = ProjectService::get_by_id(project_id.as_str())
            .await
            .ok()
            .flatten()
        else {
            return;
        };
        let Some(user_id) = project.user_id.as_deref() else {
            return;
        };
        persist_project_change_summary_snapshot(
            &project,
            &path,
            &change_kind,
            &record_id,
            &created_at,
        );
        publish_project_change_summary_updated(
            user_id,
            project_id.as_str(),
            "change_log_written",
            Some(conversation_id.as_str()),
            Some(path.as_str()),
        );
        if let Some(kind) =
            classify_project_run_path_change(path.as_str(), Some(change_kind.as_str()))
        {
            publish_project_run_catalog_updated(
                user_id,
                project_id.as_str(),
                kind.realtime_reason(),
                Some(path.as_str()),
            );
        }
    });
}

fn persist_project_change_summary_snapshot(
    project: &crate::models::project::Project,
    absolute_path: &str,
    change_kind: &str,
    change_id: &str,
    created_at: &str,
) {
    let normalized_absolute_path = absolute_path.trim().to_string();
    if normalized_absolute_path.is_empty() {
        return;
    }
    let relative_path = normalized_absolute_path
        .strip_prefix(project.root_path.trim_end_matches('/'))
        .unwrap_or(normalized_absolute_path.as_str())
        .trim_start_matches('/')
        .to_string();

    let mut snapshot =
        crate::services::project_local_cache::read_cache_json::<ProjectChangeSummarySnapshot>(
            project.root_path.as_str(),
            PROJECT_CHANGE_SUMMARY_CACHE_PATH,
        )
        .ok()
        .flatten()
        .unwrap_or_default();

    snapshot
        .file_marks
        .retain(|mark| mark.path != normalized_absolute_path);
    snapshot
        .deleted_marks
        .retain(|mark| mark.path != normalized_absolute_path);

    let mark = ProjectChangeMark {
        path: normalized_absolute_path.clone(),
        relative_path,
        kind: change_kind.to_string(),
        last_change_id: change_id.to_string(),
        updated_at: created_at.to_string(),
    };

    if change_kind == "delete" {
        snapshot.deleted_marks.push(mark);
    } else {
        snapshot.file_marks.push(mark);
    }
    snapshot
        .file_marks
        .sort_by(|left, right| left.path.cmp(&right.path));
    snapshot
        .deleted_marks
        .sort_by(|left, right| left.path.cmp(&right.path));
    snapshot.counts = ProjectChangeCounts {
        create: snapshot
            .file_marks
            .iter()
            .chain(snapshot.deleted_marks.iter())
            .filter(|mark| mark.kind == "create")
            .count(),
        edit: snapshot
            .file_marks
            .iter()
            .chain(snapshot.deleted_marks.iter())
            .filter(|mark| mark.kind == "edit")
            .count(),
        delete: snapshot
            .file_marks
            .iter()
            .chain(snapshot.deleted_marks.iter())
            .filter(|mark| mark.kind == "delete")
            .count(),
        total: snapshot.file_marks.len() + snapshot.deleted_marks.len(),
    };
    let _ = write_cache_json(
        project.root_path.as_str(),
        PROJECT_CHANGE_SUMMARY_CACHE_PATH,
        &snapshot,
    );
}

fn default_jsonl_path(server_name: &str) -> PathBuf {
    let state_dir = resolve_state_dir(server_name);
    state_dir.join(format!("{server_name}.changes.jsonl"))
}

async fn sqlite_insert(pool: SqlitePool, record: ChangeRecord) -> Result<(), String> {
    sqlx::query(
        r#"INSERT INTO mcp_change_logs
        (id, server_name, project_id, path, action, change_kind, bytes, sha256, diff, conversation_id, run_id, confirmed, confirmed_at, confirmed_by, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&record.id)
    .bind(&record.server_name)
    .bind(&record.project_id)
    .bind(&record.path)
    .bind(&record.action)
    .bind(&record.change_kind)
    .bind(record.bytes)
    .bind(&record.sha256)
    .bind(&record.diff)
    .bind(&record.conversation_id)
    .bind(&record.run_id)
    .bind(record.confirmed)
    .bind(&record.confirmed_at)
    .bind(&record.confirmed_by)
    .bind(&record.created_at)
    .execute(&pool)
    .await
    .map_err(|err| err.to_string())?;
    Ok(())
}

async fn mongo_insert(db: MongoDatabase, record: ChangeRecord) -> Result<(), String> {
    let collection = db.collection::<mongodb::bson::Document>("mcp_change_logs");
    let doc = doc! {
        "_id": &record.id,
        "id": &record.id,
        "server_name": &record.server_name,
        "project_id": record.project_id.clone(),
        "path": &record.path,
        "action": &record.action,
        "change_kind": &record.change_kind,
        "bytes": record.bytes,
        "sha256": &record.sha256,
        "diff": record.diff.clone(),
        "conversation_id": &record.conversation_id,
        "run_id": &record.run_id,
        "confirmed": record.confirmed,
        "confirmed_at": record.confirmed_at.clone(),
        "confirmed_by": record.confirmed_by.clone(),
        "created_at": &record.created_at,
    };
    collection
        .insert_one(doc, None)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn ensure_sqlite_table(pool: &SqlitePool) -> Result<(), String> {
    let pool = pool.clone();
    run_async(async move {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS mcp_change_logs (
                id TEXT PRIMARY KEY,
                server_name TEXT NOT NULL,
                project_id TEXT,
                path TEXT NOT NULL,
                action TEXT NOT NULL,
                change_kind TEXT,
                bytes INTEGER NOT NULL,
                sha256 TEXT,
                diff TEXT,
                conversation_id TEXT,
                run_id TEXT,
                confirmed INTEGER NOT NULL DEFAULT 0,
                confirmed_at TEXT,
                confirmed_by TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"#,
        )
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;
        rename_column_sqlite_if_needed(&pool, "mcp_change_logs", "session_id", "conversation_id")
            .await?;
        ensure_column_sqlite(&pool, "mcp_change_logs", "project_id", "TEXT").await?;
        ensure_column_sqlite(&pool, "mcp_change_logs", "change_kind", "TEXT").await?;
        ensure_column_sqlite(
            &pool,
            "mcp_change_logs",
            "confirmed",
            "INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        ensure_column_sqlite(&pool, "mcp_change_logs", "confirmed_at", "TEXT").await?;
        ensure_column_sqlite(&pool, "mcp_change_logs", "confirmed_by", "TEXT").await?;
        Ok(())
    })
}

async fn ensure_column_sqlite(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    ddl: &str,
) -> Result<(), String> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .map_err(|err| err.to_string())?;
    let exists = rows.iter().any(|row| {
        let name: String = row.try_get("name").unwrap_or_default();
        name == column
    });
    if !exists {
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}");
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn rename_column_sqlite_if_needed(
    pool: &SqlitePool,
    table: &str,
    from_column: &str,
    to_column: &str,
) -> Result<(), String> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .map_err(|err| err.to_string())?;
    let mut has_from = false;
    let mut has_to = false;
    for row in rows {
        let name: String = row.try_get("name").unwrap_or_default();
        if name == from_column {
            has_from = true;
        }
        if name == to_column {
            has_to = true;
        }
    }
    if has_from && !has_to {
        let sql = format!("ALTER TABLE {table} RENAME COLUMN {from_column} TO {to_column}");
        sqlx::query(&sql)
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn ensure_mongo_indexes(db: &MongoDatabase) -> Result<(), String> {
    let db = db.clone();
    run_async(async move {
        let collection = db.collection::<mongodb::bson::Document>("mcp_change_logs");
        let _ = collection
            .update_many(
                doc! {
                    "conversation_id": { "$exists": false },
                    "session_id": { "$exists": true }
                },
                doc! { "$rename": { "session_id": "conversation_id" } },
                None,
            )
            .await;
        let _ = collection
            .create_index(
                mongodb::IndexModel::builder()
                    .keys(doc! { "server_name": 1 })
                    .build(),
                None,
            )
            .await;
        let _ = collection
            .create_index(
                mongodb::IndexModel::builder()
                    .keys(doc! { "conversation_id": 1 })
                    .build(),
                None,
            )
            .await;
        let _ = collection
            .create_index(
                mongodb::IndexModel::builder()
                    .keys(doc! { "created_at": 1 })
                    .build(),
                None,
            )
            .await;
        Ok(())
    })
}

fn run_async<F>(fut: F) -> Result<(), String>
where
    F: std::future::Future<Output = Result<(), String>> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(fut))
    } else {
        let rt = tokio::runtime::Runtime::new().map_err(|err| err.to_string())?;
        rt.block_on(fut)
    }
}
