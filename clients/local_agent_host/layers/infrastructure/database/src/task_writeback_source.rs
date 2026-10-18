// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{task_callback_display::contains_cjk, ClientStorageError, SqliteResultExt};
use chatos_local_agent_protocol::LocalTaskGraphNotificationPolicy;
use serde_json::Value;
use sqlx::{Row, SqliteConnection};

pub(super) struct CallbackSource {
    pub(super) graph_id: String,
    pub(super) task_id: String,
    pub(super) task_title: String,
    pub(super) task_objective: String,
    pub(super) task_status: String,
    pub(super) task_version: u64,
    pub(super) task_updated_at_unix_ms: i64,
    pub(super) conversation_id: String,
    pub(super) turn_id: String,
    pub(super) source_user_message_id: String,
    pub(super) source_run_id: String,
    pub(super) prefers_english: bool,
    pub(super) cancel_reason: Option<String>,
    pub(super) replacement_task_ids: Vec<String>,
    pub(super) cancelled_because_task_id: Option<String>,
    pub(super) schedule_mode: String,
    pub(super) notification_policy: LocalTaskGraphNotificationPolicy,
    pub(super) notification_milestone: bool,
}

pub(super) async fn callback_source(
    connection: &mut SqliteConnection,
    task_id: &str,
) -> Result<Option<CallbackSource>, ClientStorageError> {
    let row = sqlx::query(
        "SELECT g.graph_id, t.task_id, t.title, t.input_json, t.status, t.version, \
         t.updated_at_unix_ms AS task_updated_at_unix_ms, \
         t.cancel_reason, t.replacement_task_ids_json, t.cancelled_because_task_id, \
         turn.conversation_id, turn.turn_id, turn.user_message_id, turn.run_id AS source_run_id, \
         message.content_json AS source_message_content, \
         (SELECT COUNT(*) FROM local_tasks graph_task WHERE graph_task.graph_id = g.graph_id) \
           AS graph_task_count, \
         (SELECT json_extract(policy_task.input_json, '$.graph_notification_policy') \
          FROM local_tasks policy_task WHERE policy_task.graph_id = g.graph_id \
            AND json_extract(policy_task.input_json, '$.graph_notification_policy') IS NOT NULL \
          ORDER BY policy_task.task_id LIMIT 1) AS graph_notification_policy \
         FROM local_tasks t \
         JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         JOIN local_conversation_turns turn \
           ON g.source_entity_type = 'conversation_turn' \
          AND turn.turn_id = g.source_entity_id \
         JOIN local_conversations conversation \
           ON conversation.conversation_id = turn.conversation_id \
          AND conversation.owner_user_id = g.owner_user_id \
         JOIN local_conversation_messages message \
           ON message.message_id = turn.user_message_id \
         WHERE t.task_id = ?",
    )
    .bind(task_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    row.map(|row| {
        let source_message_content: String = row.try_get("source_message_content").db()?;
        let source_message_content: Value = serde_json::from_str(&source_message_content)?;
        let task_input_json: String = row.try_get("input_json").db()?;
        let task_input: Value = serde_json::from_str(&task_input_json)?;
        let task_title: String = row.try_get("title").db()?;
        let task_objective = task_input
            .get("objective")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let replacement_task_ids_json: String = row.try_get("replacement_task_ids_json").db()?;
        let graph_task_count = usize::try_from(row.try_get::<i64, _>("graph_task_count").db()?)
            .map_err(|_| {
                ClientStorageError::InvalidState("invalid graph task count".to_string())
            })?;
        let notification_policy = row
            .try_get::<Option<String>, _>("graph_notification_policy")
            .db()?
            .map(|value| value.parse::<LocalTaskGraphNotificationPolicy>())
            .transpose()
            .map_err(ClientStorageError::InvalidState)?
            .unwrap_or_else(|| {
                LocalTaskGraphNotificationPolicy::default_for_task_count(graph_task_count)
            });
        Ok(CallbackSource {
            graph_id: row.try_get("graph_id").db()?,
            task_id: row.try_get("task_id").db()?,
            task_title: task_title.clone(),
            task_objective: task_objective.clone(),
            task_status: row.try_get("status").db()?,
            task_version: u64::try_from(row.try_get::<i64, _>("version").db()?).map_err(|_| {
                ClientStorageError::InvalidState("invalid Task version".to_string())
            })?,
            task_updated_at_unix_ms: row.try_get("task_updated_at_unix_ms").db()?,
            conversation_id: row.try_get("conversation_id").db()?,
            turn_id: row.try_get("turn_id").db()?,
            source_user_message_id: row.try_get("user_message_id").db()?,
            source_run_id: row.try_get("source_run_id").db()?,
            prefers_english: callback_prefers_english(
                &source_message_content,
                &task_title,
                &task_objective,
            ),
            cancel_reason: row.try_get("cancel_reason").db()?,
            replacement_task_ids: serde_json::from_str(&replacement_task_ids_json)?,
            cancelled_because_task_id: row.try_get("cancelled_because_task_id").db()?,
            schedule_mode: task_input
                .pointer("/schedule/mode")
                .and_then(Value::as_str)
                .unwrap_or("contact_async")
                .trim()
                .to_string(),
            notification_policy,
            notification_milestone: task_input
                .get("notification_milestone")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    })
    .transpose()
}

pub(super) fn callback_prefers_english(
    source_message: &Value,
    title: &str,
    objective: &str,
) -> bool {
    !contains_cjk(source_message)
        && !contains_cjk(&Value::String(title.to_string()))
        && !contains_cjk(&Value::String(objective.to_string()))
}
