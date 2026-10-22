// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    task_graph_store::{fetch_graph, fetch_graph_for_owner, insert_graph},
    ClientStorageError, IdempotentCommand, LocalAgentTaskStore, SqliteClientStorage,
    SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalAgentRunRecord, LocalTaskGraph, LocalTaskGraphListScope,
    LocalTaskGraphPage, LocalTaskRecord, LocalTaskStatus,
};
#[cfg(test)]
use chatos_local_agent_protocol::{LocalTaskDependency, LocalTaskGraphStatus};
use sqlx::{sqlite::SqliteRow, Row};
use std::str::FromStr;

use super::task_result::decode_result_summary;

pub(super) const SCOPED_TASK_SELECT: &str =
    "SELECT t.task_id, t.graph_id, t.title, t.profile_key, t.model_config_ref, \
     t.model_config_revision, t.capability_policy_revision, t.input_json, \
     t.max_iterations, t.status, t.active_run_id, t.version, \
     t.cancel_reason, t.replacement_task_ids_json, t.cancelled_because_task_id, \
     t.created_at_unix_ms, t.updated_at_unix_ms, g.owner_user_id, \
     g.source_entity_type, g.source_entity_id, \
     (SELECT f.state FROM local_task_followup_reviews f WHERE f.task_id = t.task_id) \
       AS followup_review_state, \
     (SELECT json_group_array(replacement.task_id) FROM local_tasks replacement \
        JOIN local_task_graphs replacement_graph ON replacement_graph.graph_id = replacement.graph_id \
        CROSS JOIN json_each(replacement.input_json, '$.supersedes_task_ids') superseded \
        WHERE replacement_graph.owner_user_id = g.owner_user_id \
          AND superseded.value = t.task_id) AS derived_replacement_task_ids_json, \
     (SELECT r.terminal_outcome_json FROM local_agent_runs r \
        WHERE r.owner_user_id = g.owner_user_id \
          AND r.owner_entity_type = 'task' AND r.owner_entity_id = t.task_id \
        ORDER BY r.created_at_unix_ms DESC, r.run_id DESC LIMIT 1) \
       AS latest_terminal_outcome_json FROM local_tasks t \
     JOIN local_task_graphs g ON g.graph_id = t.graph_id \
     JOIN local_conversation_turns source_turn \
       ON g.source_entity_type = 'conversation_turn' \
      AND source_turn.turn_id = g.source_entity_id \
     JOIN local_conversations source_conversation \
       ON source_conversation.conversation_id = source_turn.conversation_id \
     JOIN local_conversations current_conversation \
       ON current_conversation.conversation_id = ? \
     WHERE g.owner_user_id = ? \
     AND source_conversation.owner_user_id = ? \
     AND current_conversation.owner_user_id = ? \
     AND ((current_conversation.resource_kind = 'project' \
           AND current_conversation.resource_id IS NOT NULL \
           AND source_conversation.resource_kind = 'project' \
           AND source_conversation.resource_id = current_conversation.resource_id) \
          OR ((current_conversation.resource_kind IS NULL \
               OR current_conversation.resource_kind <> 'project') \
              AND source_conversation.conversation_id = current_conversation.conversation_id))";

#[async_trait]
impl LocalAgentTaskStore for SqliteClientStorage {
    async fn resolve_task_followup(
        &self,
        command: &IdempotentCommand,
        review: &chatos_local_agent_protocol::ResolveTaskFollowupCommand,
        now_unix_ms: i64,
    ) -> Result<serde_json::Value, ClientStorageError> {
        super::task_followup_resolution::resolve(self, command, review, now_unix_ms).await
    }
    async fn create_task_graph(
        &self,
        command: &IdempotentCommand,
        graph: &CreateTaskGraphCommand,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError> {
        graph.validate().map_err(ClientStorageError::InvalidState)?;
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            super::task_followup_gate::guard_creation(&mut connection, graph).await?;
            super::task_replacement_validation::validate_stopped_sources(&mut connection, graph)
                .await?;
            let mut frozen_graph = graph.clone();
            super::requirement_plan_store::freeze_for_graph(&mut connection, &mut frozen_graph)
                .await?;
            frozen_graph
                .validate()
                .map_err(ClientStorageError::InvalidState)?;
            insert_graph(&mut connection, &frozen_graph, now_unix_ms)
                .await
                .db()?;
            super::task_replacement_validation::link_replacements(
                &mut connection,
                &frozen_graph,
                now_unix_ms,
            )
            .await?;
            let created = fetch_graph(&mut connection, &graph.graph_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(graph.graph_id.clone()))?;
            Self::record_receipt(&mut connection, command, &created, now_unix_ms)
                .await
                .db()?;
            Ok(created)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_task_graph(
        &self,
        owner_user_id: &str,
        graph_id: &str,
    ) -> Result<Option<LocalTaskGraph>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_graph_for_owner(&mut connection, owner_user_id, graph_id).await
    }

    async fn list_task_graphs(
        &self,
        owner_user_id: &str,
        scope: LocalTaskGraphListScope,
        source_entity_type: Option<&str>,
        source_entity_id: Option<&str>,
        before_updated_at_unix_ms: Option<i64>,
        before_graph_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalTaskGraphPage, ClientStorageError> {
        super::task_query_store::list_graphs(
            self,
            owner_user_id,
            scope,
            source_entity_type,
            source_entity_id,
            before_updated_at_unix_ms,
            before_graph_id,
            limit,
        )
        .await
    }

    async fn list_task_runs(
        &self,
        owner_user_id: &str,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalAgentRunRecord>, ClientStorageError> {
        if !(1..=100).contains(&limit) {
            return Err(ClientStorageError::InvalidState(
                "task Run limit must be between 1 and 100".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM local_tasks t JOIN local_task_graphs g \
             ON g.graph_id = t.graph_id WHERE t.task_id = ? AND g.owner_user_id = ?",
        )
        .bind(task_id)
        .bind(owner_user_id)
        .fetch_one(&mut *connection)
        .await
        .db()?;
        if exists == 0 {
            return Err(ClientStorageError::NotFound(task_id.to_string()));
        }
        sqlx::query(
            "SELECT run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key, \
             model_config_ref, model_config_revision, capability_policy_revision, input_json, \
             status, iteration, model_attempt, max_iterations, version, claim_token, \
             claim_until_unix_ms, next_attempt_at_unix_ms, pending_tool_batch_json, \
             terminal_outcome_json, checkpoint_json, continuation_input_json, \
             created_at_unix_ms, updated_at_unix_ms FROM local_agent_runs \
             WHERE owner_user_id = ? AND owner_entity_type = 'task' AND owner_entity_id = ? \
             ORDER BY created_at_unix_ms DESC, run_id DESC LIMIT ?",
        )
        .bind(owner_user_id)
        .bind(task_id)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
        .into_iter()
        .map(super::decode_run)
        .collect()
    }

    async fn list_tasks_for_conversation(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        status: Option<LocalTaskStatus>,
        keyword: Option<&str>,
        tag: Option<&str>,
        scheduled_only: Option<bool>,
        parent_task_id: Option<&str>,
        source_run_id: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<LocalTaskRecord>, ClientStorageError> {
        validate_task_query(
            owner_user_id,
            conversation_id,
            keyword,
            tag,
            parent_task_id,
            source_run_id,
            limit,
            offset,
        )?;
        let normalized_keyword = keyword
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(escaped_like_pattern);
        let status = status.map(LocalTaskStatus::as_str);
        let tag = tag.map(str::trim).filter(|value| !value.is_empty());
        let scheduled_only = scheduled_only.unwrap_or(false);
        let parent_task_id = parent_task_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let source_run_id = source_run_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let sql = format!(
            "{SCOPED_TASK_SELECT} \
             AND (? IS NULL OR t.status = ?) \
             AND (? IS NULL OR LOWER(t.task_id || ' ' || t.title || ' ' || t.input_json) \
                  LIKE ? ESCAPE '\\') \
             AND (? IS NULL OR EXISTS (SELECT 1 FROM json_each(t.input_json, '$.tags') tag_item \
                  WHERE tag_item.value = ?)) \
             AND (? = 0 OR COALESCE(json_extract(t.input_json, '$.schedule.mode'), 'manual') \
                  <> 'manual') \
             AND (? IS NULL OR json_extract(t.input_json, '$.parent_task_id') = ?) \
             AND (? IS NULL OR json_extract(t.input_json, '$.source_run_id') = ?) \
             ORDER BY t.updated_at_unix_ms DESC, t.task_id DESC LIMIT ? OFFSET ?"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(conversation_id)
            .bind(owner_user_id)
            .bind(owner_user_id)
            .bind(owner_user_id)
            .bind(status)
            .bind(status)
            .bind(normalized_keyword.as_deref())
            .bind(normalized_keyword.as_deref())
            .bind(tag)
            .bind(tag)
            .bind(i64::from(scheduled_only))
            .bind(parent_task_id)
            .bind(parent_task_id)
            .bind(source_run_id)
            .bind(source_run_id)
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(&self.pool)
            .await
            .db()?;
        rows.into_iter().map(decode_scoped_task).collect()
    }

    async fn get_task_for_conversation(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        task_id: &str,
    ) -> Result<Option<LocalTaskRecord>, ClientStorageError> {
        validate_task_query(owner_user_id, conversation_id, None, None, None, None, 1, 0)?;
        if task_id.trim().is_empty() || task_id.len() > 256 || task_id.chars().any(char::is_control)
        {
            return Err(ClientStorageError::InvalidState(
                "invalid task id".to_string(),
            ));
        }
        let sql = format!("{SCOPED_TASK_SELECT} AND t.task_id = ?");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(conversation_id)
            .bind(owner_user_id)
            .bind(owner_user_id)
            .bind(owner_user_id)
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
            .db()?;
        row.map(decode_scoped_task).transpose()
    }

    async fn start_next_task_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        // The scheduler checks this on every wake and after every model/tool commit. Avoid taking
        // SQLite's write reservation when the account has no materializable task; the
        // transactional implementation below rechecks the candidate before writing.
        let has_ready_task = sqlx::query_scalar::<_, i64>(
            "SELECT 1 FROM local_tasks t \
             JOIN local_task_graphs g ON g.graph_id = t.graph_id \
             WHERE g.owner_user_id = ? \
             AND (t.status = 'ready' OR (t.status = 'pending' \
               AND NOT EXISTS (SELECT 1 FROM local_task_dependencies d \
                 JOIN local_tasks p ON p.task_id = d.prerequisite_task_id \
                 WHERE d.task_id = t.task_id AND p.status <> 'succeeded') \
               AND NOT EXISTS (SELECT 1 FROM local_task_external_dependencies d \
                 JOIN local_tasks p ON p.task_id = d.prerequisite_task_id \
                 WHERE d.task_id = t.task_id AND p.status <> 'succeeded'))) \
             AND NOT EXISTS (SELECT 1 FROM local_task_followup_reviews f \
               WHERE f.task_id = t.task_id AND f.state IN ('pending','held')) \
             AND t.active_run_id IS NULL LIMIT 1",
        )
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await
        .db()?
        .is_some();
        if !has_ready_task {
            return Ok(None);
        }
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = super::task_lifecycle::start_next_task_run(
            &mut connection,
            owner_user_id,
            None,
            run_id,
            event_id,
            now_unix_ms,
        )
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn start_next_task_run_for_graph(
        &self,
        owner_user_id: &str,
        graph_id: &str,
        run_id: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        let has_ready_task = sqlx::query_scalar::<_, i64>(
            "SELECT 1 FROM local_tasks t \
             JOIN local_task_graphs g ON g.graph_id = t.graph_id \
             WHERE g.owner_user_id = ? AND t.graph_id = ? \
             AND (t.status = 'ready' OR (t.status = 'pending' \
               AND NOT EXISTS (SELECT 1 FROM local_task_dependencies d \
                 JOIN local_tasks p ON p.task_id = d.prerequisite_task_id \
                 WHERE d.task_id = t.task_id AND p.status <> 'succeeded') \
               AND NOT EXISTS (SELECT 1 FROM local_task_external_dependencies d \
                 JOIN local_tasks p ON p.task_id = d.prerequisite_task_id \
                 WHERE d.task_id = t.task_id AND p.status <> 'succeeded'))) \
             AND t.active_run_id IS NULL LIMIT 1",
        )
        .bind(owner_user_id)
        .bind(graph_id)
        .fetch_optional(&self.pool)
        .await
        .db()?
        .is_some();
        if !has_ready_task {
            return Ok(None);
        }
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = super::task_lifecycle::start_next_task_run(
            &mut connection,
            owner_user_id,
            Some(graph_id),
            run_id,
            event_id,
            now_unix_ms,
        )
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn cancel_task(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        task_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        replacement_task_ids: &[String],
        run_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError> {
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = super::task_commands::cancel_task(
            &mut connection,
            command,
            owner_user_id,
            task_id,
            expected_version,
            reason,
            replacement_task_ids,
            run_event_id,
            now_unix_ms,
        )
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn retry_task(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        task_id: &str,
        expected_version: u64,
        retry_instruction: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError> {
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = super::task_commands::retry_task(
            &mut connection,
            command,
            owner_user_id,
            task_id,
            expected_version,
            retry_instruction,
            now_unix_ms,
        )
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn restart_task(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        task_id: &str,
        expected_version: u64,
        reason: &str,
        run_event_prefix: &str,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError> {
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = super::task_commands::restart_task(
            &mut connection,
            command,
            owner_user_id,
            task_id,
            expected_version,
            reason,
            run_event_prefix,
            now_unix_ms,
        )
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

fn decode_scoped_task(row: SqliteRow) -> Result<LocalTaskRecord, ClientStorageError> {
    let status: String = row.try_get("status").db()?;
    Ok(LocalTaskRecord {
        graph_id: row.try_get("graph_id").db()?,
        owner_user_id: row.try_get("owner_user_id").db()?,
        source_entity_type: row.try_get("source_entity_type").db()?,
        source_entity_id: row.try_get("source_entity_id").db()?,
        task_id: row.try_get("task_id").db()?,
        title: row.try_get("title").db()?,
        profile_key: row.try_get("profile_key").db()?,
        model_config_ref: row.try_get("model_config_ref").db()?,
        model_config_revision: row.try_get("model_config_revision").db()?,
        capability_policy_revision: row.try_get("capability_policy_revision").db()?,
        input: super::task_followup_gate::task_input(&row)?,
        max_iterations: u32::try_from(row.try_get::<i64, _>("max_iterations").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid max_iterations".to_string()))?,
        status: LocalTaskStatus::from_str(&status).map_err(ClientStorageError::InvalidState)?,
        active_run_id: row.try_get("active_run_id").db()?,
        cancel_reason: row.try_get("cancel_reason").db()?,
        replacement_task_ids: replacement_task_ids(&row)?,
        cancelled_because_task_id: row.try_get("cancelled_because_task_id").db()?,
        result_summary: decode_result_summary(&row)?,
        version: u64::try_from(row.try_get::<i64, _>("version").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid task version".to_string()))?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

fn replacement_task_ids(row: &SqliteRow) -> Result<Vec<String>, ClientStorageError> {
    let task_id: String = row.try_get("task_id").db()?;
    let stored: String = row.try_get("replacement_task_ids_json").db()?;
    let mut ids = serde_json::from_str::<Vec<String>>(&stored)?;
    let derived: String = row.try_get("derived_replacement_task_ids_json").db()?;
    ids.extend(serde_json::from_str::<Vec<String>>(&derived)?);
    ids.retain(|candidate| candidate != &task_id && !candidate.trim().is_empty());
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn validate_task_query(
    owner_user_id: &str,
    conversation_id: &str,
    keyword: Option<&str>,
    tag: Option<&str>,
    parent_task_id: Option<&str>,
    source_run_id: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<(), ClientStorageError> {
    let valid_identifier = |value: &str| {
        !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
    };
    if !valid_identifier(owner_user_id)
        || !valid_identifier(conversation_id)
        || !(1..=500).contains(&limit)
        || offset > 100_000
        || keyword.is_some_and(|value| value.len() > 500 || value.chars().any(char::is_control))
        || [tag, parent_task_id, source_run_id]
            .into_iter()
            .flatten()
            .any(|value| value.len() > 256 || value.chars().any(char::is_control))
    {
        return Err(ClientStorageError::InvalidState(
            "invalid task query".to_string(),
        ));
    }
    Ok(())
}

fn escaped_like_pattern(value: &str) -> String {
    let value = value
        .to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{value}%")
}

#[cfg(test)]
#[path = "task_store_tests.rs"]
mod tests;
