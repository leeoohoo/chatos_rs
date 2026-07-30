// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
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
              AND NOT EXISTS (
                SELECT 1 FROM local_task_runs
                WHERE local_task_runs.turn_id = messages.turn_id
                  AND local_task_runs.task_kind = 'conversation_task'
              )
            ORDER BY messages.sequence_no ASC
            "#,
        )
        .bind(session_id)
        .bind(owner_user_id)
        .fetch_all(self.pool())
        .await
        .context("list local runtime messages")
    }

    pub(crate) async fn append_turn_result_message(
        &self,
        input: AppendLocalMessageInput,
    ) -> Result<LocalMessageRecord> {
        let mut transaction = self
            .begin_write()
            .await
            .context("append local turn result message")?;
        let turn_exists = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM turns
            INNER JOIN sessions ON sessions.id = turns.session_id
            WHERE turns.id = ? AND turns.session_id = ?
              AND turns.status IN ('completed', 'failed', 'cancelled')
              AND sessions.owner_user_id = ?
            "#,
        )
        .bind(input.turn_id.as_str())
        .bind(input.session_id.as_str())
        .bind(input.owner_user_id.as_str())
        .fetch_one(&mut *transaction)
        .await
        .context("validate local task result source turn")?;
        if turn_exists == 0 {
            return Err(anyhow::anyhow!(
                "local task result source turn is not terminal"
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
        .context("insert local task result message")?;
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
            FROM messages WHERE id = ?
            "#,
        )
        .bind(message_id)
        .fetch_one(&mut *transaction)
        .await
        .context("load local task result message")?;
        transaction
            .commit()
            .await
            .context("commit local task result message")?;
        Ok(message)
    }

    pub(crate) async fn set_turn_task_runner_status(
        &self,
        owner_user_id: &str,
        turn_id: &str,
        overall_status: &str,
        confirmation_status: &str,
    ) -> Result<()> {
        self.set_turn_task_runner_status_inner(
            owner_user_id,
            turn_id,
            overall_status,
            confirmation_status,
            None,
        )
        .await
    }

    pub(crate) async fn set_turn_task_runner_terminal_task_status(
        &self,
        owner_user_id: &str,
        turn_id: &str,
        task_id: &str,
        overall_status: &str,
        confirmation_status: &str,
    ) -> Result<()> {
        self.set_turn_task_runner_status_inner(
            owner_user_id,
            turn_id,
            overall_status,
            confirmation_status,
            Some(task_id),
        )
        .await
    }

    async fn set_turn_task_runner_status_inner(
        &self,
        owner_user_id: &str,
        turn_id: &str,
        overall_status: &str,
        confirmation_status: &str,
        terminal_task_id: Option<&str>,
    ) -> Result<()> {
        let raw_metadata = sqlx::query_scalar::<_, Option<String>>(
            r#"
            SELECT messages.metadata_json
            FROM messages
            INNER JOIN sessions ON sessions.id = messages.session_id
            WHERE messages.turn_id = ? AND messages.role = 'user'
              AND sessions.owner_user_id = ?
            ORDER BY messages.sequence_no ASC
            LIMIT 1
            "#,
        )
        .bind(turn_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("load local turn task runner metadata")?
        .flatten();
        let mut metadata = raw_metadata
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        let task_runner = metadata
            .entry("task_runner_async".to_string())
            .or_insert_with(|| json!({}));
        if !task_runner.is_object() {
            *task_runner = Value::Object(Map::new());
        }
        if let Some(task_runner) = task_runner.as_object_mut() {
            let current_overall_status = task_runner
                .get("overall_status")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if task_runner_status_is_stop_locked(current_overall_status)
                && !task_runner_status_is_stop_locked(overall_status)
            {
                return Ok(());
            }
            task_runner.insert(
                "overall_status".to_string(),
                Value::String(overall_status.to_string()),
            );
            task_runner.insert(
                "confirmation_status".to_string(),
                Value::String(confirmation_status.to_string()),
            );
            if let Some(task_id) = terminal_task_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                task_runner.insert(
                    "last_task_id".to_string(),
                    Value::String(task_id.to_string()),
                );
                task_runner.insert(
                    "status".to_string(),
                    Value::String(overall_status.to_string()),
                );
                ensure_string_array_contains(task_runner, "created_task_ids", task_id);
                ensure_string_array_contains(task_runner, "terminal_task_ids", task_id);
                remove_string_array_value(task_runner, "running_task_ids", task_id);
                remove_string_array_value(task_runner, "queued_task_ids", task_id);
                remove_string_array_value(task_runner, "pending_task_ids", task_id);
                match overall_status.trim().to_ascii_lowercase().as_str() {
                    "completed" | "complete" | "done" | "succeeded" | "success" => {
                        ensure_string_array_contains(task_runner, "succeeded_task_ids", task_id);
                    }
                    "blocked" => {
                        ensure_string_array_contains(task_runner, "blocked_task_ids", task_id);
                    }
                    "cancelled" | "canceled" => {
                        ensure_string_array_contains(task_runner, "cancelled_task_ids", task_id);
                    }
                    "failed" | "error" => {
                        ensure_string_array_contains(task_runner, "failed_task_ids", task_id);
                    }
                    _ => {}
                }
            }
            if task_runner_status_is_stop_locked(overall_status) {
                task_runner.insert("stopped_at".to_string(), Value::String(local_now_rfc3339()));
            }
        }
        sqlx::query(
            r#"
            UPDATE messages
            SET metadata_json = ?
            WHERE turn_id = ? AND role = 'user'
              AND EXISTS (
                SELECT 1 FROM sessions
                WHERE sessions.id = messages.session_id AND sessions.owner_user_id = ?
              )
            "#,
        )
        .bind(Value::Object(metadata).to_string())
        .bind(turn_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("update local turn task runner metadata")?;
        Ok(())
    }

    pub(crate) async fn set_turn_task_runner_execution_paused(
        &self,
        owner_user_id: &str,
        turn_id: &str,
        paused: bool,
    ) -> Result<()> {
        self.set_turn_task_runner_status(
            owner_user_id,
            turn_id,
            if paused { "paused" } else { "processing" },
            "confirmed",
        )
        .await?;
        let raw_metadata = sqlx::query_scalar::<_, Option<String>>(
            r#"
            SELECT messages.metadata_json
            FROM messages
            INNER JOIN sessions ON sessions.id = messages.session_id
            WHERE messages.turn_id = ? AND messages.role = 'user'
              AND sessions.owner_user_id = ?
            ORDER BY messages.sequence_no ASC
            LIMIT 1
            "#,
        )
        .bind(turn_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("load local execution pause metadata")?
        .flatten();
        let mut metadata = raw_metadata
            .as_deref()
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        let task_runner = metadata
            .entry("task_runner_async".to_string())
            .or_insert_with(|| json!({}));
        if !task_runner.is_object() {
            *task_runner = Value::Object(Map::new());
        }
        if let Some(task_runner) = task_runner.as_object_mut() {
            task_runner.insert("execution_paused".to_string(), Value::Bool(paused));
        }
        sqlx::query(
            r#"
            UPDATE messages SET metadata_json = ?
            WHERE turn_id = ? AND role = 'user'
              AND EXISTS (
                SELECT 1 FROM sessions
                WHERE sessions.id = messages.session_id AND sessions.owner_user_id = ?
              )
            "#,
        )
        .bind(Value::Object(metadata).to_string())
        .bind(turn_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("update local execution pause metadata")?;
        Ok(())
    }

    pub(crate) async fn set_turn_messages_hidden(
        &self,
        owner_user_id: &str,
        turn_id: &str,
        hidden: bool,
    ) -> Result<()> {
        let messages = self.list_turn_messages(owner_user_id, turn_id).await?;
        for message in messages {
            let mut metadata = message
                .metadata_json
                .as_deref()
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
            if hidden {
                metadata.insert("hidden".to_string(), Value::Bool(true));
            } else {
                metadata.remove("hidden");
            }
            sqlx::query(
                r#"
                UPDATE messages SET metadata_json = ?
                WHERE id = ? AND turn_id = ?
                  AND EXISTS (
                    SELECT 1 FROM sessions
                    WHERE sessions.id = messages.session_id AND sessions.owner_user_id = ?
                  )
                "#,
            )
            .bind(Value::Object(metadata).to_string())
            .bind(message.id.as_str())
            .bind(turn_id)
            .bind(owner_user_id)
            .execute(self.pool())
            .await
            .context("update local turn message visibility")?;
        }
        Ok(())
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

fn ensure_string_array_contains(metadata: &mut Map<String, Value>, key: &str, value: &str) {
    let mut values = metadata
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !values.iter().any(|item| item == value) {
        values.push(value.to_string());
    }
    metadata.insert(
        key.to_string(),
        Value::Array(values.into_iter().map(Value::String).collect()),
    );
}

fn remove_string_array_value(metadata: &mut Map<String, Value>, key: &str, value: &str) {
    let Some(values) = metadata.get(key).and_then(Value::as_array) else {
        return;
    };
    let values = values
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty() && *item != value)
        .map(|item| Value::String(item.to_string()))
        .collect::<Vec<_>>();
    metadata.insert(key.to_string(), Value::Array(values));
}

fn task_runner_status_is_stop_locked(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "stopping" | "stopped" | "cancelled" | "canceled"
    )
}
