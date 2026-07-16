// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::turn_queries::{next_message_sequence, refresh_session_message_count};
use super::{AppendLocalMessageInput, LocalDatabase, LocalMessageRecord};

impl LocalDatabase {
    pub(crate) async fn list_messages(
        &self,
        owner_user_id: &str,
        session_id: &str,
    ) -> Result<Vec<LocalMessageRecord>> {
        sqlx::query_as::<_, LocalMessageRecord>(
            r#"
            SELECT messages.id, messages.session_id, messages.turn_id, messages.sequence_no,
                   messages.role, messages.content, messages.reasoning,
                   messages.tool_calls_json, messages.tool_call_id,
                   messages.metadata_json, messages.created_at
            FROM messages
            INNER JOIN sessions ON sessions.id = messages.session_id
            WHERE messages.session_id = ? AND sessions.owner_user_id = ?
            ORDER BY messages.sequence_no ASC
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .fetch_all(self.pool())
        .await
        .context("list local runtime messages")
    }

    pub(crate) async fn list_turn_messages(
        &self,
        owner_user_id: &str,
        turn_id: &str,
    ) -> Result<Vec<LocalMessageRecord>> {
        sqlx::query_as::<_, LocalMessageRecord>(
            r#"
            SELECT messages.id, messages.session_id, messages.turn_id, messages.sequence_no,
                   messages.role, messages.content, messages.reasoning,
                   messages.tool_calls_json, messages.tool_call_id,
                   messages.metadata_json, messages.created_at
            FROM messages
            INNER JOIN sessions ON sessions.id = messages.session_id
            WHERE messages.turn_id = ? AND sessions.owner_user_id = ?
            ORDER BY messages.sequence_no ASC
            "#,
        )
        .bind(turn_id)
        .bind(owner_user_id)
        .fetch_all(self.pool())
        .await
        .context("list local runtime turn messages")
    }

    pub(crate) async fn append_turn_message(
        &self,
        input: AppendLocalMessageInput,
    ) -> Result<LocalMessageRecord> {
        let mut transaction = self
            .begin_write()
            .await
            .context("append local turn message")?;
        let turn_is_running = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM turns
            INNER JOIN sessions ON sessions.id = turns.session_id
            WHERE turns.id = ? AND turns.session_id = ? AND turns.status = 'running'
              AND sessions.owner_user_id = ?
            "#,
        )
        .bind(input.turn_id.as_str())
        .bind(input.session_id.as_str())
        .bind(input.owner_user_id.as_str())
        .fetch_one(&mut *transaction)
        .await
        .context("validate local runtime turn message")?;
        if turn_is_running == 0 {
            return Err(anyhow::anyhow!(
                "local runtime turn is not available for process messages"
            ));
        }

        let message_id = input
            .message_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("lc_message_{}", Uuid::new_v4()));
        let created_at = input.created_at.unwrap_or_else(local_now_rfc3339);
        let sequence_no =
            next_message_sequence(&mut transaction, input.session_id.as_str()).await?;
        sqlx::query(
            r#"
            INSERT INTO messages (
                id, session_id, turn_id, sequence_no, role, content, reasoning,
                tool_calls_json, tool_call_id, metadata_json, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(message_id.as_str())
        .bind(input.session_id.as_str())
        .bind(input.turn_id.as_str())
        .bind(sequence_no)
        .bind(input.role.as_str())
        .bind(input.content.as_str())
        .bind(input.reasoning.as_deref())
        .bind(input.tool_calls_json.as_deref())
        .bind(input.tool_call_id.as_deref())
        .bind(input.metadata_json.as_deref())
        .bind(created_at.as_str())
        .execute(&mut *transaction)
        .await
        .context("insert local runtime process message")?;
        refresh_session_message_count(
            &mut transaction,
            input.session_id.as_str(),
            created_at.as_str(),
        )
        .await?;

        let message = sqlx::query_as::<_, LocalMessageRecord>(
            r#"
            SELECT id, session_id, turn_id, sequence_no, role, content, reasoning,
                   tool_calls_json, tool_call_id, metadata_json, created_at
            FROM messages
            WHERE id = ?
            "#,
        )
        .bind(message_id.as_str())
        .fetch_one(&mut *transaction)
        .await
        .context("load appended local runtime process message")?;
        transaction
            .commit()
            .await
            .context("commit local runtime process message")?;
        Ok(message)
    }
}
