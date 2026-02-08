use super::utils::{generate_id, now_iso, resolve_state_dir};
use crate::db::{get_db_sync, Database};
use mongodb::bson::doc;
use mongodb::Database as MongoDatabase;
use serde::Serialize;
use sqlx::SqlitePool;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tracing::warn;

#[derive(Clone)]
pub struct ChangeLogStore {
    backend: ChangeLogBackend,
    server_name: String,
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
    pub path: String,
    pub action: String,
    pub bytes: i64,
    pub sha256: String,
    pub diff: Option<String>,
    pub session_id: String,
    pub run_id: String,
    pub created_at: String,
}

impl ChangeLogStore {
    pub fn new(server_name: &str, db_path: Option<String>) -> Result<Self, String> {
        if let Some(path) = db_path {
            let path = PathBuf::from(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            return Ok(Self {
                backend: ChangeLogBackend::Jsonl { path },
                server_name: server_name.to_string(),
            });
        }

        match get_db_sync() {
            Ok(adapter) => match &*adapter {
                Database::Sqlite(pool) => {
                    ensure_sqlite_table(pool)?;
                    Ok(Self {
                        backend: ChangeLogBackend::Sqlite { pool: pool.clone() },
                        server_name: server_name.to_string(),
                    })
                }
                Database::Mongo { db, .. } => {
                    ensure_mongo_indexes(db)?;
                    Ok(Self {
                        backend: ChangeLogBackend::Mongo { db: db.clone() },
                        server_name: server_name.to_string(),
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
                })
            }
        }
    }

    pub fn log_change(
        &self,
        path: &str,
        action: &str,
        bytes: i64,
        sha256: &str,
        session_id: &str,
        run_id: &str,
        diff: Option<String>,
    ) -> Result<ChangeRecord, String> {
        let record = ChangeRecord {
            id: generate_id("change"),
            server_name: self.server_name.clone(),
            path: path.to_string(),
            action: action.to_string(),
            bytes,
            sha256: sha256.to_string(),
            diff,
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
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
                file.write_all(line.as_bytes()).map_err(|err| err.to_string())?;
                file.write_all(b"\n").map_err(|err| err.to_string())?;
            }
            ChangeLogBackend::Sqlite { pool } => {
                run_async(sqlite_insert(pool.clone(), record.clone()))?;
            }
            ChangeLogBackend::Mongo { db } => {
                run_async(mongo_insert(db.clone(), record.clone()))?;
            }
        }
        Ok(record)
    }

    // list_changes intentionally omitted for the embedded builtin server.
}

fn default_jsonl_path(server_name: &str) -> PathBuf {
    let state_dir = resolve_state_dir(server_name);
    state_dir.join(format!("{server_name}.changes.jsonl"))
}

async fn sqlite_insert(pool: SqlitePool, record: ChangeRecord) -> Result<(), String> {
    sqlx::query(
        r#"INSERT INTO mcp_change_logs
        (id, server_name, path, action, bytes, sha256, diff, session_id, run_id, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&record.id)
    .bind(&record.server_name)
    .bind(&record.path)
    .bind(&record.action)
    .bind(record.bytes)
    .bind(&record.sha256)
    .bind(&record.diff)
    .bind(&record.session_id)
    .bind(&record.run_id)
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
        "path": &record.path,
        "action": &record.action,
        "bytes": record.bytes,
        "sha256": &record.sha256,
        "diff": record.diff.clone(),
        "session_id": &record.session_id,
        "run_id": &record.run_id,
        "created_at": &record.created_at,
    };
    collection.insert_one(doc, None).await.map_err(|err| err.to_string())?;
    Ok(())
}

fn ensure_sqlite_table(pool: &SqlitePool) -> Result<(), String> {
    let pool = pool.clone();
    run_async(async move {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS mcp_change_logs (
                id TEXT PRIMARY KEY,
                server_name TEXT NOT NULL,
                path TEXT NOT NULL,
                action TEXT NOT NULL,
                bytes INTEGER NOT NULL,
                sha256 TEXT,
                diff TEXT,
                session_id TEXT,
                run_id TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"#,
        )
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

fn ensure_mongo_indexes(db: &MongoDatabase) -> Result<(), String> {
    let db = db.clone();
    run_async(async move {
        let collection = db.collection::<mongodb::bson::Document>("mcp_change_logs");
        let _ = collection
            .create_index(mongodb::IndexModel::builder().keys(doc! { "server_name": 1 }).build(), None)
            .await;
        let _ = collection
            .create_index(mongodb::IndexModel::builder().keys(doc! { "session_id": 1 }).build(), None)
            .await;
        let _ = collection
            .create_index(mongodb::IndexModel::builder().keys(doc! { "created_at": 1 }).build(), None)
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
