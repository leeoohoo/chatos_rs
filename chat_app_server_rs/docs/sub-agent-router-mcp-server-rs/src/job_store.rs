use crate::types::{JobEvent, JobRecord};
use crate::utils::generate_id;
use chrono::Utc;
use rusqlite::{params, Connection};

pub struct JobStore {
    conn: Connection,
    default_session_id: String,
    default_run_id: String,
}

impl JobStore {
    pub fn new(db_path: &str, default_session_id: String, default_run_id: String) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|err| err.to_string())?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|err| err.to_string())?;
        conn.execute_batch(
            r#"
      CREATE TABLE IF NOT EXISTS subagent_jobs (
        id TEXT PRIMARY KEY,
        status TEXT NOT NULL,
        task TEXT NOT NULL,
        agent_id TEXT,
        command_id TEXT,
        payload_json TEXT,
        result_json TEXT,
        error TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        session_id TEXT NOT NULL,
        run_id TEXT
      );
      CREATE INDEX IF NOT EXISTS subagent_jobs_session_idx ON subagent_jobs(session_id);
      CREATE INDEX IF NOT EXISTS subagent_jobs_status_idx ON subagent_jobs(status);

      CREATE TABLE IF NOT EXISTS subagent_events (
        id TEXT PRIMARY KEY,
        job_id TEXT NOT NULL,
        type TEXT NOT NULL,
        payload_json TEXT,
        created_at TEXT NOT NULL,
        session_id TEXT NOT NULL,
        run_id TEXT
      );
      CREATE INDEX IF NOT EXISTS subagent_events_job_idx ON subagent_events(job_id);

      CREATE TABLE IF NOT EXISTS model_routes (
        id TEXT PRIMARY KEY,
        model TEXT,
        reason TEXT,
        created_at TEXT NOT NULL,
        session_id TEXT NOT NULL,
        run_id TEXT
      );
      "#,
        )
        .map_err(|err| err.to_string())?;
        Ok(Self {
            conn,
            default_session_id,
            default_run_id,
        })
    }

    pub fn create_job(
        &self,
        task: &str,
        agent_id: Option<String>,
        command_id: Option<String>,
        payload: Option<serde_json::Value>,
    ) -> Result<JobRecord, String> {
        let now = Utc::now().to_rfc3339();
        let record = JobRecord {
            id: generate_id("job"),
            status: "queued".to_string(),
            task: task.to_string(),
            agent_id,
            command_id,
            payload_json: payload.map(|p| p.to_string()),
            result_json: None,
            error: None,
            created_at: now.clone(),
            updated_at: now,
            session_id: self.default_session_id.clone(),
            run_id: self.default_run_id.clone(),
        };
        self.conn
            .execute(
                r#"
        INSERT INTO subagent_jobs (
          id, status, task, agent_id, command_id, payload_json, result_json, error,
          created_at, updated_at, session_id, run_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
                params![
                    record.id,
                    record.status,
                    record.task,
                    record.agent_id,
                    record.command_id,
                    record.payload_json,
                    record.result_json,
                    record.error,
                    record.created_at,
                    record.updated_at,
                    record.session_id,
                    record.run_id,
                ],
            )
            .map_err(|err| err.to_string())?;
        Ok(record)
    }

    pub fn update_job_status(
        &self,
        job_id: &str,
        status: &str,
        result_json: Option<String>,
        error: Option<String>,
    ) -> Result<Option<JobRecord>, String> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                r#"
        UPDATE subagent_jobs
        SET status = ?1,
            result_json = ?2,
            error = ?3,
            updated_at = ?4
        WHERE id = ?5
        "#,
                params![status, result_json, error, now, job_id],
            )
            .map_err(|err| err.to_string())?;
        self.get_job(job_id)
    }

    pub fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM subagent_jobs WHERE id = ?1")
            .map_err(|err| err.to_string())?;
        let mut rows = stmt.query(params![job_id]).map_err(|err| err.to_string())?;
        if let Some(row) = rows.next().map_err(|err| err.to_string())? {
            Ok(Some(from_job_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn append_event(
        &self,
        job_id: &str,
        event_type: &str,
        payload: Option<serde_json::Value>,
    ) -> Result<JobEvent, String> {
        let now = Utc::now().to_rfc3339();
        let record = JobEvent {
            id: generate_id("event"),
            job_id: job_id.to_string(),
            r#type: event_type.to_string(),
            payload_json: payload.map(|p| p.to_string()),
            created_at: now.clone(),
            session_id: self.default_session_id.clone(),
            run_id: self.default_run_id.clone(),
        };
        self.conn
            .execute(
                r#"
        INSERT INTO subagent_events (id, job_id, type, payload_json, created_at, session_id, run_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
                params![
                    record.id,
                    record.job_id,
                    record.r#type,
                    record.payload_json,
                    record.created_at,
                    record.session_id,
                    record.run_id
                ],
            )
            .map_err(|err| err.to_string())?;
        Ok(record)
    }

    pub fn list_jobs(
        &self,
        session_id: Option<&str>,
        status: Option<&str>,
        limit: Option<i64>,
        all_sessions: bool,
    ) -> Result<Vec<JobRecord>, String> {
        let mut conditions = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(status_val) = status {
            let normalized = status_val.trim().to_lowercase();
            if !normalized.is_empty() {
                conditions.push("status = ?".to_string());
                params.push(rusqlite::types::Value::from(normalized));
            }
        }
        if !all_sessions {
            let sid_raw = session_id.unwrap_or("").trim().to_string();
            let sid = if sid_raw.is_empty() {
                self.default_session_id.clone()
            } else {
                sid_raw
            };
            conditions.push("session_id = ?".to_string());
            params.push(rusqlite::types::Value::from(sid));
        }
        let limit = limit.unwrap_or(200).max(1);
        params.push(rusqlite::types::Value::from(limit));

        let where_clause = if conditions.is_empty() {
            "".to_string()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!(
            "SELECT * FROM subagent_jobs {} ORDER BY created_at DESC LIMIT ?",
            where_clause
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|err| err.to_string())?;
        let mut rows = stmt
            .query(rusqlite::params_from_iter(params))
            .map_err(|err| err.to_string())?;
        let mut jobs = Vec::new();
        while let Some(row) = rows.next().map_err(|err| err.to_string())? {
            jobs.push(from_job_row(row)?);
        }
        Ok(jobs)
    }

    pub fn list_sessions(&self, limit: i64) -> Result<Vec<(String, i64, String)>, String> {
        let max = limit.max(1);
        let mut stmt = self
            .conn
            .prepare(
                r#"
      SELECT session_id as session_id, COUNT(*) as count, MAX(created_at) as last_created_at
      FROM subagent_jobs
      GROUP BY session_id
      ORDER BY last_created_at DESC
      LIMIT ?1
      "#,
            )
            .map_err(|err| err.to_string())?;
        let mut rows = stmt.query(params![max]).map_err(|err| err.to_string())?;
        let mut sessions = Vec::new();
        while let Some(row) = rows.next().map_err(|err| err.to_string())? {
            let session_id: String = row.get(0).map_err(|err| err.to_string())?;
            let count: i64 = row.get(1).map_err(|err| err.to_string())?;
            let last_created: String = row.get(2).map_err(|err| err.to_string())?;
            sessions.push((session_id, count, last_created));
        }
        Ok(sessions)
    }

    pub fn list_events(&self, job_id: &str, limit: i64) -> Result<Vec<JobEvent>, String> {
        let max = limit.max(1);
        let mut stmt = self
            .conn
            .prepare(
                r#"
      SELECT * FROM subagent_events
      WHERE job_id = ?1
      ORDER BY created_at ASC
      LIMIT ?2
      "#,
            )
            .map_err(|err| err.to_string())?;
        let mut rows = stmt.query(params![job_id, max]).map_err(|err| err.to_string())?;
        let mut events = Vec::new();
        while let Some(row) = rows.next().map_err(|err| err.to_string())? {
            events.push(from_event_row(row)?);
        }
        Ok(events)
    }
}

fn from_job_row(row: &rusqlite::Row) -> Result<JobRecord, String> {
    Ok(JobRecord {
        id: row.get("id").map_err(|err| err.to_string())?,
        status: row.get("status").map_err(|err| err.to_string())?,
        task: row.get("task").map_err(|err| err.to_string())?,
        agent_id: row.get("agent_id").map_err(|err| err.to_string())?,
        command_id: row.get("command_id").map_err(|err| err.to_string())?,
        payload_json: row.get("payload_json").map_err(|err| err.to_string())?,
        result_json: row.get("result_json").map_err(|err| err.to_string())?,
        error: row.get("error").map_err(|err| err.to_string())?,
        created_at: row.get("created_at").map_err(|err| err.to_string())?,
        updated_at: row.get("updated_at").map_err(|err| err.to_string())?,
        session_id: row.get("session_id").map_err(|err| err.to_string())?,
        run_id: row
            .get::<_, Option<String>>("run_id")
            .map_err(|err| err.to_string())?
            .unwrap_or_default(),
    })
}

fn from_event_row(row: &rusqlite::Row) -> Result<JobEvent, String> {
    Ok(JobEvent {
        id: row.get("id").map_err(|err| err.to_string())?,
        job_id: row.get("job_id").map_err(|err| err.to_string())?,
        r#type: row.get("type").map_err(|err| err.to_string())?,
        payload_json: row.get("payload_json").map_err(|err| err.to_string())?,
        created_at: row.get("created_at").map_err(|err| err.to_string())?,
        session_id: row.get("session_id").map_err(|err| err.to_string())?,
        run_id: row
            .get::<_, Option<String>>("run_id")
            .map_err(|err| err.to_string())?
            .unwrap_or_default(),
    })
}
