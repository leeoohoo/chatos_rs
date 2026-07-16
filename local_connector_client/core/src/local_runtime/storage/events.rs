// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::{AppendLocalRuntimeEventInput, LocalDatabase, LocalRuntimeEventRecord};

impl LocalDatabase {
    pub(crate) async fn append_runtime_event(
        &self,
        input: AppendLocalRuntimeEventInput,
    ) -> Result<LocalRuntimeEventRecord> {
        let event_id = format!("lc_event_{}", Uuid::new_v4());
        let created_at = local_now_rfc3339();
        let payload_json = input.payload.to_string();
        let result = sqlx::query(
            r#"
            INSERT INTO runtime_events (
                event_id, owner_user_id, project_id, session_id, turn_id,
                event_name, stream_type, payload_json, created_at
            )
            SELECT ?, ?, sessions.project_id, sessions.id, ?, ?, ?, ?, ?
            FROM sessions
            WHERE sessions.id = ? AND sessions.owner_user_id = ?
              AND EXISTS (
                  SELECT 1 FROM turns
                  WHERE turns.id = ? AND turns.session_id = sessions.id
              )
            "#,
        )
        .bind(event_id.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.turn_id.as_str())
        .bind(input.event_name.as_str())
        .bind(input.stream_type.as_deref())
        .bind(payload_json.as_str())
        .bind(created_at.as_str())
        .bind(input.session_id.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.turn_id.as_str())
        .execute(self.pool())
        .await
        .context("append local runtime event")?;
        if result.rows_affected() != 1 {
            return Err(anyhow::anyhow!(
                "local runtime session or turn is not available for events"
            ));
        }

        sqlx::query_as::<_, LocalRuntimeEventRecord>(
            r#"
            SELECT event_seq, event_id, project_id, session_id, turn_id,
                   event_name, stream_type, payload_json, created_at
            FROM runtime_events
            WHERE event_id = ?
            "#,
        )
        .bind(event_id)
        .fetch_one(self.pool())
        .await
        .context("load appended local runtime event")
    }

    pub(crate) async fn list_runtime_events(
        &self,
        owner_user_id: &str,
        session_id: &str,
        turn_id: Option<&str>,
        after_sequence: i64,
        limit: i64,
    ) -> Result<Vec<LocalRuntimeEventRecord>> {
        sqlx::query_as::<_, LocalRuntimeEventRecord>(
            r#"
            SELECT runtime_events.event_seq, runtime_events.event_id,
                   runtime_events.project_id, runtime_events.session_id,
                   runtime_events.turn_id, runtime_events.event_name,
                   runtime_events.stream_type, runtime_events.payload_json,
                   runtime_events.created_at
            FROM runtime_events
            INNER JOIN sessions ON sessions.id = runtime_events.session_id
            WHERE runtime_events.session_id = ? AND sessions.owner_user_id = ?
              AND runtime_events.event_seq > ?
              AND (? IS NULL OR runtime_events.turn_id = ?)
            ORDER BY runtime_events.event_seq ASC
            LIMIT ?
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .bind(after_sequence.max(0))
        .bind(turn_id)
        .bind(turn_id)
        .bind(limit.clamp(1, 500))
        .fetch_all(self.pool())
        .await
        .context("list local runtime events")
    }
}
