// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;

use super::super::{LocalDatabase, LocalSubjectMemoryRecord};

impl LocalDatabase {
    pub(crate) async fn forget_subject_memory_for_session(
        &self,
        owner_user_id: &str,
        session_id: &str,
        recall_id: &str,
    ) -> Result<i64> {
        let Some(session) = self.get_session(session_id, owner_user_id).await? else {
            return Ok(0);
        };
        let agent_id = session.selected_agent_id.as_deref().unwrap_or("");
        let Some(target) = sqlx::query_as::<_, LocalSubjectMemoryRecord>(
            r#"
            SELECT id, subject_type, subject_id, project_id, recall_key,
                   recall_text, source_session_id, source_summary_id, level,
                   confidence, last_seen_at, created_at, updated_at
            FROM subject_memories
            WHERE id = ? AND owner_user_id = ? AND project_id = ?
              AND source_session_id <> ?
              AND (
                  (subject_type = 'project' AND subject_id = ?)
                  OR (subject_type = 'agent' AND subject_id = ?)
              )
            "#,
        )
        .bind(recall_id)
        .bind(owner_user_id)
        .bind(session.project_id.as_str())
        .bind(session_id)
        .bind(session.project_id.as_str())
        .bind(agent_id)
        .fetch_optional(self.pool())
        .await
        .context("load local subject memory to forget")?
        else {
            return Ok(0);
        };
        if target.recall_key == "rollup" {
            return self.delete_rollup(owner_user_id, target.id.as_str()).await;
        }
        self.forget_source_recalls(owner_user_id, &target).await
    }

    async fn delete_rollup(&self, owner_user_id: &str, recall_id: &str) -> Result<i64> {
        let result = sqlx::query(
            "DELETE FROM subject_memories WHERE id = ? AND owner_user_id = ? AND recall_key = 'rollup'",
        )
        .bind(recall_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("delete local recall rollup")?;
        Ok(result.rows_affected() as i64)
    }

    async fn forget_source_recalls(
        &self,
        owner_user_id: &str,
        target: &LocalSubjectMemoryRecord,
    ) -> Result<i64> {
        let mut transaction = self
            .begin_write()
            .await
            .context("begin local recall forget")?;
        let records = sqlx::query_as::<_, LocalSubjectMemoryRecord>(
            r#"
            SELECT id, subject_type, subject_id, project_id, recall_key,
                   recall_text, source_session_id, source_summary_id, level,
                   confidence, last_seen_at, created_at, updated_at
            FROM subject_memories
            WHERE owner_user_id = ? AND project_id = ? AND source_summary_id = ?
              AND recall_key <> 'rollup'
            "#,
        )
        .bind(owner_user_id)
        .bind(target.project_id.as_str())
        .bind(target.source_summary_id.as_str())
        .fetch_all(&mut *transaction)
        .await
        .context("load duplicate local recalls to forget")?;
        let now = local_now_rfc3339();
        for record in &records {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO subject_memory_forget_markers (
                    owner_user_id, subject_type, subject_id, project_id, recall_key, created_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(owner_user_id)
            .bind(record.subject_type.as_str())
            .bind(record.subject_id.as_str())
            .bind(record.project_id.as_str())
            .bind(record.recall_key.as_str())
            .bind(now.as_str())
            .execute(&mut *transaction)
            .await
            .context("persist local recall forget marker")?;
        }
        let result = sqlx::query(
            r#"
            DELETE FROM subject_memories
            WHERE owner_user_id = ? AND project_id = ? AND source_summary_id = ?
              AND recall_key <> 'rollup'
            "#,
        )
        .bind(owner_user_id)
        .bind(target.project_id.as_str())
        .bind(target.source_summary_id.as_str())
        .execute(&mut *transaction)
        .await
        .context("delete forgotten local recalls")?;
        transaction
            .commit()
            .await
            .context("commit local recall forget")?;
        Ok(result.rows_affected() as i64)
    }
}
