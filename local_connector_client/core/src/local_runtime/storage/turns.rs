// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::turn_queries::{
    find_turn_by_id, find_turn_by_idempotency, load_turn_snapshot, next_message_sequence,
    refresh_session_message_count,
};
use super::{
    BeginLocalBackgroundTurnInput, BeginLocalTurnInput, BeginLocalTurnResult,
    CompleteLocalTurnInput, LocalDatabase, LocalTurnSnapshot,
};

impl LocalDatabase {
    pub(crate) async fn begin_background_turn(
        &self,
        input: BeginLocalBackgroundTurnInput,
    ) -> Result<BeginLocalTurnResult> {
        let mut transaction = self
            .begin_write()
            .await
            .context("begin local background turn")?;
        if let Some(turn) = find_turn_by_idempotency(
            &mut transaction,
            input.owner_user_id.as_str(),
            input.session_id.as_str(),
            input.idempotency_key.as_str(),
        )
        .await?
        {
            let snapshot = load_turn_snapshot(&mut transaction, turn).await?;
            transaction
                .commit()
                .await
                .context("commit existing local background turn lookup")?;
            return Ok(BeginLocalTurnResult::Existing(snapshot));
        }
        let source_user_message_id = sqlx::query_scalar::<_, String>(
            r#"
            SELECT turns.user_message_id
            FROM turns
            INNER JOIN sessions ON sessions.id = turns.session_id
            WHERE turns.id = ? AND turns.session_id = ? AND sessions.owner_user_id = ?
              AND turns.user_message_id IS NOT NULL
            "#,
        )
        .bind(input.source_turn_id.as_str())
        .bind(input.session_id.as_str())
        .bind(input.owner_user_id.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .context("resolve local background turn source message")?
        .ok_or_else(|| anyhow::anyhow!("local background turn source was not found"))?;
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO turns (
                id, session_id, user_message_id, idempotency_key, status,
                cancel_requested, started_at, created_at, updated_at
            ) VALUES (?, ?, ?, ?, 'running', 0, ?, ?, ?)
            "#,
        )
        .bind(input.turn_id.as_str())
        .bind(input.session_id.as_str())
        .bind(source_user_message_id)
        .bind(input.idempotency_key.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("insert local background turn")?;
        let turn = find_turn_by_id(
            &mut transaction,
            input.owner_user_id.as_str(),
            input.turn_id.as_str(),
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("local background turn was not persisted"))?;
        let snapshot = load_turn_snapshot(&mut transaction, turn).await?;
        transaction
            .commit()
            .await
            .context("commit local background turn")?;
        Ok(BeginLocalTurnResult::Started(snapshot))
    }

    pub(crate) async fn complete_background_turn(
        &self,
        owner_user_id: &str,
        turn_id: &str,
    ) -> Result<()> {
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE turns SET status = 'completed', error_code = NULL, error_message = NULL,
                finished_at = ?, updated_at = ?
            WHERE id = ? AND status = 'running'
              AND EXISTS (
                SELECT 1 FROM sessions
                WHERE sessions.id = turns.session_id AND sessions.owner_user_id = ?
              )
            "#,
        )
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(turn_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("complete local background turn")?;
        Ok(())
    }

    pub(crate) async fn begin_turn(
        &self,
        input: BeginLocalTurnInput,
    ) -> Result<BeginLocalTurnResult> {
        let mut transaction = self.begin_write().await.context("begin local turn")?;
        if let Some(turn) = find_turn_by_idempotency(
            &mut transaction,
            input.owner_user_id.as_str(),
            input.session_id.as_str(),
            input.idempotency_key.as_str(),
        )
        .await?
        {
            let snapshot = load_turn_snapshot(&mut transaction, turn).await?;
            transaction
                .commit()
                .await
                .context("commit existing local turn lookup")?;
            return Ok(BeginLocalTurnResult::Existing(snapshot));
        }

        let session_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sessions WHERE id = ? AND owner_user_id = ? AND status = 'active'",
        )
        .bind(input.session_id.as_str())
        .bind(input.owner_user_id.as_str())
        .fetch_one(&mut *transaction)
        .await
        .context("validate local turn session")?;
        if session_exists == 0 {
            return Err(anyhow::anyhow!("local runtime session is not available"));
        }

        let now = local_now_rfc3339();
        let user_message_id = format!("lc_message_{}", Uuid::new_v4());
        let sequence_no =
            next_message_sequence(&mut transaction, input.session_id.as_str()).await?;
        sqlx::query(
            r#"
            INSERT INTO turns (
                id, session_id, user_message_id, idempotency_key, status,
                cancel_requested, started_at, created_at, updated_at
            ) VALUES (?, ?, ?, ?, 'running', 0, ?, ?, ?)
            "#,
        )
        .bind(input.turn_id.as_str())
        .bind(input.session_id.as_str())
        .bind(user_message_id.as_str())
        .bind(input.idempotency_key.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("insert local turn")?;
        sqlx::query(
            r#"
            INSERT INTO messages (
                id, session_id, turn_id, sequence_no, role, content,
                metadata_json, created_at
            ) VALUES (?, ?, ?, ?, 'user', ?, ?, ?)
            "#,
        )
        .bind(user_message_id.as_str())
        .bind(input.session_id.as_str())
        .bind(input.turn_id.as_str())
        .bind(sequence_no)
        .bind(input.content.as_str())
        .bind(input.metadata_json.as_deref())
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("insert local turn user message")?;
        refresh_session_message_count(&mut transaction, input.session_id.as_str(), now.as_str())
            .await?;

        let turn = find_turn_by_id(
            &mut transaction,
            input.owner_user_id.as_str(),
            input.turn_id.as_str(),
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("local turn was not persisted"))?;
        let snapshot = load_turn_snapshot(&mut transaction, turn).await?;
        transaction.commit().await.context("commit local turn")?;
        Ok(BeginLocalTurnResult::Started(snapshot))
    }

    pub(crate) async fn complete_turn(
        &self,
        input: CompleteLocalTurnInput,
    ) -> Result<LocalTurnSnapshot> {
        let mut transaction = self.begin_write().await.context("complete local turn")?;
        let turn = find_turn_by_id(
            &mut transaction,
            input.owner_user_id.as_str(),
            input.turn_id.as_str(),
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("local runtime turn is not available"))?;
        if turn.status == "completed" {
            let snapshot = load_turn_snapshot(&mut transaction, turn).await?;
            transaction
                .commit()
                .await
                .context("commit completed local turn lookup")?;
            return Ok(snapshot);
        }
        if turn.status != "running" {
            return Err(anyhow::anyhow!(
                "local runtime turn cannot complete from status {}",
                turn.status
            ));
        }

        let now = local_now_rfc3339();
        let assistant_message_id = format!("lc_message_{}", Uuid::new_v4());
        let sequence_no = next_message_sequence(&mut transaction, turn.session_id.as_str()).await?;
        sqlx::query(
            r#"
            INSERT INTO messages (
                id, session_id, turn_id, sequence_no, role, content, reasoning,
                tool_calls_json, metadata_json, created_at
            ) VALUES (?, ?, ?, ?, 'assistant', ?, ?, ?, ?, ?)
            "#,
        )
        .bind(assistant_message_id.as_str())
        .bind(turn.session_id.as_str())
        .bind(turn.id.as_str())
        .bind(sequence_no)
        .bind(input.content.as_str())
        .bind(input.reasoning.as_deref())
        .bind(input.tool_calls_json.as_deref())
        .bind(input.metadata_json.as_deref())
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("insert local assistant message")?;
        sqlx::query(
            r#"
            UPDATE turns
            SET status = 'completed', error_code = NULL, error_message = NULL,
                finished_at = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(turn.id.as_str())
        .execute(&mut *transaction)
        .await
        .context("mark local turn completed")?;
        refresh_session_message_count(&mut transaction, turn.session_id.as_str(), now.as_str())
            .await?;

        let completed = find_turn_by_id(
            &mut transaction,
            input.owner_user_id.as_str(),
            turn.id.as_str(),
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("completed local turn is unavailable"))?;
        let snapshot = load_turn_snapshot(&mut transaction, completed).await?;
        transaction
            .commit()
            .await
            .context("commit local turn completion")?;
        Ok(snapshot)
    }

    pub(crate) async fn fail_turn(
        &self,
        owner_user_id: &str,
        turn_id: &str,
        error_code: &str,
        error_message: &str,
    ) -> Result<()> {
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE turns
            SET status = 'failed', error_code = ?, error_message = ?,
                finished_at = ?, updated_at = ?
            WHERE id = ? AND status = 'running'
              AND EXISTS (
                  SELECT 1 FROM sessions
                  WHERE sessions.id = turns.session_id AND sessions.owner_user_id = ?
              )
            "#,
        )
        .bind(error_code)
        .bind(error_message)
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(turn_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("mark local turn failed")?;
        Ok(())
    }

    pub(crate) async fn request_turn_cancel(
        &self,
        owner_user_id: &str,
        session_id: &str,
        turn_id: Option<&str>,
    ) -> Result<Option<String>> {
        let active_turn_id = sqlx::query_scalar::<_, String>(
            r#"
            SELECT turns.id
            FROM turns
            INNER JOIN sessions ON sessions.id = turns.session_id
            WHERE turns.session_id = ? AND turns.status = 'running'
              AND sessions.owner_user_id = ?
              AND (? IS NULL OR turns.id = ?)
            ORDER BY turns.created_at DESC
            LIMIT 1
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .bind(turn_id)
        .bind(turn_id)
        .fetch_optional(self.pool())
        .await
        .context("find local turn to cancel")?;
        let Some(active_turn_id) = active_turn_id else {
            return Ok(None);
        };
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE turns
            SET cancel_requested = 1, updated_at = ?
            WHERE id = ? AND status = 'running'
            "#,
        )
        .bind(now.as_str())
        .bind(active_turn_id.as_str())
        .execute(self.pool())
        .await
        .context("request local turn cancellation")?;
        Ok(Some(active_turn_id))
    }

    pub(crate) async fn cancel_turn(
        &self,
        owner_user_id: &str,
        turn_id: &str,
        reason: &str,
    ) -> Result<()> {
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE turns
            SET status = 'cancelled', cancel_requested = 1,
                error_code = 'local_runtime_turn_cancelled', error_message = ?,
                finished_at = ?, updated_at = ?
            WHERE id = ? AND status = 'running'
              AND EXISTS (
                  SELECT 1 FROM sessions
                  WHERE sessions.id = turns.session_id AND sessions.owner_user_id = ?
              )
            "#,
        )
        .bind(reason)
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(turn_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("mark local turn cancelled")?;
        Ok(())
    }
}
