use crate::utils::{generate_id, now_iso};
use rusqlite::{params, Connection, Row};
use rusqlite::types::Value as SqlValue;

pub struct ChangeLogStore {
    conn: Connection,
}

#[derive(Debug, serde::Serialize)]
pub struct ChangeRecord {
    pub id: String,
    pub path: String,
    pub action: String,
    pub bytes: i64,
    pub sha256: String,
    pub diff: Option<String>,
    pub session_id: String,
    pub run_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ChangeQuery {
    pub path: Option<String>,
    pub path_prefix: Option<String>,
    pub action: Option<String>,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

impl ChangeLogStore {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|err| err.to_string())?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|err| err.to_string())?;
        conn.execute_batch(
            r#"
      CREATE TABLE IF NOT EXISTS file_changes (
        id TEXT PRIMARY KEY,
        path TEXT NOT NULL,
        action TEXT NOT NULL,
        bytes INTEGER NOT NULL,
        sha256 TEXT NOT NULL,
        diff TEXT,
        session_id TEXT NOT NULL,
        run_id TEXT NOT NULL,
        created_at TEXT NOT NULL
      );
      CREATE INDEX IF NOT EXISTS file_changes_path_idx ON file_changes(path);
      CREATE INDEX IF NOT EXISTS file_changes_session_idx ON file_changes(session_id);
      CREATE INDEX IF NOT EXISTS file_changes_created_idx ON file_changes(created_at);
      "#,
        )
        .map_err(|err| err.to_string())?;
        if let Err(err) = conn.execute("ALTER TABLE file_changes ADD COLUMN diff TEXT", []) {
            let message = err.to_string();
            let is_duplicate = message.contains("duplicate column") || message.contains("already exists");
            if !is_duplicate {
                return Err(message);
            }
        }
        Ok(Self { conn })
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
            path: path.to_string(),
            action: action.to_string(),
            bytes,
            sha256: sha256.to_string(),
            diff,
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            created_at: now_iso(),
        };
        self.conn
            .execute(
                r#"
        INSERT INTO file_changes (id, path, action, bytes, sha256, diff, session_id, run_id, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
                params![
                    record.id,
                    record.path,
                    record.action,
                    record.bytes,
                    record.sha256,
                    record.diff,
                    record.session_id,
                    record.run_id,
                    record.created_at
                ],
            )
            .map_err(|err| err.to_string())?;
        Ok(record)
    }

    pub fn list_changes(&self, query: ChangeQuery, include_diff: bool) -> Result<Vec<ChangeRecord>, String> {
        let mut conditions = Vec::new();
        let mut params: Vec<SqlValue> = Vec::new();

        if let Some(path) = query.path {
            conditions.push("path = ?".to_string());
            params.push(SqlValue::from(path));
        }
        if let Some(prefix) = query.path_prefix {
            conditions.push("path LIKE ?".to_string());
            params.push(SqlValue::from(format!("{}%", prefix)));
        }
        if let Some(action) = query.action {
            conditions.push("action = ?".to_string());
            params.push(SqlValue::from(action));
        }
        if let Some(session_id) = query.session_id {
            conditions.push("session_id = ?".to_string());
            params.push(SqlValue::from(session_id));
        }
        if let Some(run_id) = query.run_id {
            conditions.push("run_id = ?".to_string());
            params.push(SqlValue::from(run_id));
        }

        let where_clause = if conditions.is_empty() {
            "".to_string()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let select_columns = if include_diff {
            "id, path, action, bytes, sha256, diff, session_id, run_id, created_at"
        } else {
            "id, path, action, bytes, sha256, session_id, run_id, created_at"
        };
        let sql = format!(
            "SELECT {} FROM file_changes {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
            select_columns, where_clause
        );
        params.push(SqlValue::from(query.limit.max(1)));
        params.push(SqlValue::from(query.offset.max(0)));

        let mut stmt = self.conn.prepare(&sql).map_err(|err| err.to_string())?;
        let mut rows = stmt
            .query(rusqlite::params_from_iter(params))
            .map_err(|err| err.to_string())?;
        let mut records = Vec::new();
        while let Some(row) = rows.next().map_err(|err| err.to_string())? {
            records.push(from_row(row, include_diff)?);
        }
        Ok(records)
    }
}

fn from_row(row: &Row, include_diff: bool) -> Result<ChangeRecord, String> {
    Ok(ChangeRecord {
        id: row.get("id").map_err(|err| err.to_string())?,
        path: row.get("path").map_err(|err| err.to_string())?,
        action: row.get("action").map_err(|err| err.to_string())?,
        bytes: row.get("bytes").map_err(|err| err.to_string())?,
        sha256: row.get("sha256").map_err(|err| err.to_string())?,
        diff: if include_diff {
            row.get("diff").map_err(|err| err.to_string())?
        } else {
            None
        },
        session_id: row.get("session_id").map_err(|err| err.to_string())?,
        run_id: row.get("run_id").map_err(|err| err.to_string())?,
        created_at: row.get("created_at").map_err(|err| err.to_string())?,
    })
}
