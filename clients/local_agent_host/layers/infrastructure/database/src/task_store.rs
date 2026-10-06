// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, IdempotentCommand, LocalAgentTaskStore, SqliteClientStorage,
    SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalAgentRunRecord, LocalTaskDependency, LocalTaskGraph,
    LocalTaskGraphListScope, LocalTaskGraphPage, LocalTaskGraphStatus, LocalTaskRecord,
    LocalTaskStatus,
};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};
use std::{collections::HashSet, str::FromStr};

#[async_trait]
impl LocalAgentTaskStore for SqliteClientStorage {
    async fn create_task_graph(
        &self,
        command: &IdempotentCommand,
        graph: &CreateTaskGraphCommand,
        now_unix_ms: i64,
    ) -> Result<LocalTaskGraph, ClientStorageError> {
        graph.validate().map_err(ClientStorageError::InvalidState)?;
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            insert_graph(&mut connection, graph, now_unix_ms)
                .await
                .db()?;
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
        let rows = sqlx::query(
            "SELECT t.task_id, t.graph_id, t.title, t.profile_key, t.model_config_ref, \
             t.model_config_revision, t.capability_policy_revision, t.input_json, \
             t.max_iterations, t.status, t.active_run_id, t.version, \
             t.created_at_unix_ms, t.updated_at_unix_ms, g.owner_user_id, \
             g.source_entity_type, g.source_entity_id FROM local_tasks t \
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
                      AND source_conversation.conversation_id = \
                          current_conversation.conversation_id)) \
             AND (? IS NULL OR t.status = ?) \
             AND (? IS NULL OR LOWER(t.task_id || ' ' || t.title || ' ' || t.input_json) \
                  LIKE ? ESCAPE '\\') \
             AND (? IS NULL OR EXISTS (SELECT 1 FROM json_each(t.input_json, '$.tags') tag_item \
                  WHERE tag_item.value = ?)) \
             AND (? = 0 OR COALESCE(json_extract(t.input_json, '$.schedule.mode'), 'manual') \
                  <> 'manual') \
             AND (? IS NULL OR json_extract(t.input_json, '$.parent_task_id') = ?) \
             AND (? IS NULL OR json_extract(t.input_json, '$.source_run_id') = ?) \
             ORDER BY t.updated_at_unix_ms DESC, t.task_id DESC LIMIT ? OFFSET ?",
        )
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
        let row = sqlx::query(
            "SELECT t.task_id, t.graph_id, t.title, t.profile_key, t.model_config_ref, \
             t.model_config_revision, t.capability_policy_revision, t.input_json, \
             t.max_iterations, t.status, t.active_run_id, t.version, \
             t.created_at_unix_ms, t.updated_at_unix_ms, g.owner_user_id, \
             g.source_entity_type, g.source_entity_id FROM local_tasks t \
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
                      AND source_conversation.conversation_id = \
                          current_conversation.conversation_id)) \
             AND t.task_id = ?",
        )
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
             WHERE g.owner_user_id = ? AND t.status = 'ready' \
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
        let mut connection = self.pool.acquire().await.db()?;
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
             WHERE g.owner_user_id = ? AND t.graph_id = ? AND t.status = 'ready' \
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
        let mut connection = self.pool.acquire().await.db()?;
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
        let mut connection = self.pool.acquire().await.db()?;
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
        let mut connection = self.pool.acquire().await.db()?;
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
        let mut connection = self.pool.acquire().await.db()?;
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

async fn insert_graph(
    connection: &mut SqliteConnection,
    graph: &CreateTaskGraphCommand,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let external_dependencies = graph
        .tasks
        .iter()
        .map(|task| {
            external_prerequisite_ids(&task.input).map(|prerequisite_ids| {
                prerequisite_ids
                    .into_iter()
                    .map(|prerequisite_task_id| LocalTaskDependency {
                        task_id: task.task_id.clone(),
                        prerequisite_task_id,
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    for dependency in &external_dependencies {
        if dependency.task_id == dependency.prerequisite_task_id {
            return Err(ClientStorageError::InvalidState(
                "a task cannot depend on itself".to_string(),
            ));
        }
        let owned: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_tasks prerequisite \
             JOIN local_task_graphs prerequisite_graph \
               ON prerequisite_graph.graph_id = prerequisite.graph_id \
             WHERE prerequisite.task_id = ? AND prerequisite_graph.owner_user_id = ?",
        )
        .bind(&dependency.prerequisite_task_id)
        .bind(&graph.owner_user_id)
        .fetch_one(&mut *connection)
        .await
        .db()?;
        if owned == 0 {
            return Err(ClientStorageError::NotFound(format!(
                "prerequisite Task not found for owner: {}",
                dependency.prerequisite_task_id
            )));
        }
    }
    let insert = sqlx::query(
        "INSERT INTO local_task_graphs(\
         graph_id, owner_user_id, source_entity_type, source_entity_id, created_at_unix_ms) \
         VALUES(?, ?, ?, ?, ?)",
    )
    .bind(&graph.graph_id)
    .bind(&graph.owner_user_id)
    .bind(&graph.source_entity_type)
    .bind(&graph.source_entity_id)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await;
    map_insert(
        insert,
        format!("task graph already exists: {}", graph.graph_id),
    )?;

    let pending = graph
        .dependencies
        .iter()
        .map(|dependency| dependency.task_id.as_str())
        .chain(
            external_dependencies
                .iter()
                .map(|dependency| dependency.task_id.as_str()),
        )
        .collect::<HashSet<_>>();
    for task in &graph.tasks {
        let status = if pending.contains(task.task_id.as_str()) {
            LocalTaskStatus::Pending
        } else {
            LocalTaskStatus::Ready
        };
        let insert = sqlx::query(
            "INSERT INTO local_tasks(\
             task_id, graph_id, title, profile_key, model_config_ref, model_config_revision, \
             capability_policy_revision, input_json, max_iterations, status, active_run_id, \
             version, created_at_unix_ms, updated_at_unix_ms) \
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 1, ?, ?)",
        )
        .bind(&task.task_id)
        .bind(&graph.graph_id)
        .bind(&task.title)
        .bind(&task.profile_key)
        .bind(&task.model_config_ref)
        .bind(&task.model_config_revision)
        .bind(&task.capability_policy_revision)
        .bind(serde_json::to_string(&task.input)?)
        .bind(i64::from(task.max_iterations))
        .bind(status.as_str())
        .bind(now_unix_ms)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await;
        map_insert(insert, format!("task already exists: {}", task.task_id))?;
    }
    for dependency in &graph.dependencies {
        sqlx::query(
            "INSERT INTO local_task_dependencies(\
             graph_id, task_id, prerequisite_task_id) VALUES(?, ?, ?)",
        )
        .bind(&graph.graph_id)
        .bind(&dependency.task_id)
        .bind(&dependency.prerequisite_task_id)
        .execute(&mut *connection)
        .await
        .db()?;
    }
    for dependency in &external_dependencies {
        sqlx::query(
            "INSERT INTO local_task_external_dependencies(task_id, prerequisite_task_id) \
             VALUES(?, ?)",
        )
        .bind(&dependency.task_id)
        .bind(&dependency.prerequisite_task_id)
        .execute(&mut *connection)
        .await
        .db()?;
    }
    Ok(())
}

fn external_prerequisite_ids(input: &serde_json::Value) -> Result<Vec<String>, ClientStorageError> {
    let Some(value) = input.get("prerequisite_task_ids") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        ClientStorageError::InvalidState(
            "Task prerequisite_task_ids must be a JSON array".to_string(),
        )
    })?;
    let mut unique = HashSet::new();
    values
        .iter()
        .map(|value| {
            let task_id = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    ClientStorageError::InvalidState(
                        "Task prerequisite_task_ids contains an invalid id".to_string(),
                    )
                })?;
            if !unique.insert(task_id.to_string()) {
                return Err(ClientStorageError::InvalidState(
                    "Task prerequisite_task_ids contains a duplicate".to_string(),
                ));
            }
            Ok(task_id.to_string())
        })
        .collect()
}

fn map_insert(
    result: Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error>,
    conflict: String,
) -> Result<(), ClientStorageError> {
    match result {
        Ok(_) => Ok(()),
        Err(error)
            if error
                .as_database_error()
                .is_some_and(|value| value.is_unique_violation()) =>
        {
            Err(ClientStorageError::Conflict(conflict))
        }
        Err(error) => Err(ClientStorageError::database(error)),
    }
}

pub(super) async fn fetch_graph(
    connection: &mut SqliteConnection,
    graph_id: &str,
) -> Result<Option<LocalTaskGraph>, ClientStorageError> {
    let graph = sqlx::query(
        "SELECT graph_id, owner_user_id, source_entity_type, source_entity_id, \
         created_at_unix_ms FROM local_task_graphs WHERE graph_id = ?",
    )
    .bind(graph_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(graph) = graph else { return Ok(None) };
    let tasks = sqlx::query(
        "SELECT task_id, graph_id, title, profile_key, model_config_ref, \
         model_config_revision, capability_policy_revision, input_json, max_iterations, \
         status, active_run_id, version, created_at_unix_ms, updated_at_unix_ms \
         FROM local_tasks WHERE graph_id = ? ORDER BY task_id",
    )
    .bind(graph_id)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(|row| decode_task(row, &graph))
    .collect::<Result<Vec<_>, _>>()?;
    let dependencies = sqlx::query(
        "SELECT task_id, prerequisite_task_id FROM local_task_dependencies \
         WHERE graph_id = ? ORDER BY task_id, prerequisite_task_id",
    )
    .bind(graph_id)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(|row| {
        Ok(LocalTaskDependency {
            task_id: row.try_get("task_id").db()?,
            prerequisite_task_id: row.try_get("prerequisite_task_id").db()?,
        })
    })
    .collect::<Result<Vec<_>, ClientStorageError>>()?;
    let status = LocalTaskGraphStatus::derive(&tasks);
    Ok(Some(LocalTaskGraph {
        graph_id: graph.try_get("graph_id").db()?,
        owner_user_id: graph.try_get("owner_user_id").db()?,
        source_entity_type: graph.try_get("source_entity_type").db()?,
        source_entity_id: graph.try_get("source_entity_id").db()?,
        status,
        tasks,
        dependencies,
        created_at_unix_ms: graph.try_get("created_at_unix_ms").db()?,
    }))
}

async fn fetch_graph_for_owner(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    graph_id: &str,
) -> Result<Option<LocalTaskGraph>, ClientStorageError> {
    let owned = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM local_task_graphs WHERE graph_id = ? AND owner_user_id = ?",
    )
    .bind(graph_id)
    .bind(owner_user_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    if owned == 0 {
        return Ok(None);
    }
    fetch_graph(connection, graph_id).await
}

fn decode_task(row: SqliteRow, graph: &SqliteRow) -> Result<LocalTaskRecord, ClientStorageError> {
    let input: String = row.try_get("input_json").db()?;
    let status: String = row.try_get("status").db()?;
    Ok(LocalTaskRecord {
        graph_id: row.try_get("graph_id").db()?,
        owner_user_id: graph.try_get("owner_user_id").db()?,
        source_entity_type: graph.try_get("source_entity_type").db()?,
        source_entity_id: graph.try_get("source_entity_id").db()?,
        task_id: row.try_get("task_id").db()?,
        title: row.try_get("title").db()?,
        profile_key: row.try_get("profile_key").db()?,
        model_config_ref: row.try_get("model_config_ref").db()?,
        model_config_revision: row.try_get("model_config_revision").db()?,
        capability_policy_revision: row.try_get("capability_policy_revision").db()?,
        input: serde_json::from_str(&input)?,
        max_iterations: u32::try_from(row.try_get::<i64, _>("max_iterations").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid max_iterations".to_string()))?,
        status: LocalTaskStatus::from_str(&status).map_err(ClientStorageError::InvalidState)?,
        active_run_id: row.try_get("active_run_id").db()?,
        version: u64::try_from(row.try_get::<i64, _>("version").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid task version".to_string()))?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

fn decode_scoped_task(row: SqliteRow) -> Result<LocalTaskRecord, ClientStorageError> {
    let input: String = row.try_get("input_json").db()?;
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
        input: serde_json::from_str(&input)?,
        max_iterations: u32::try_from(row.try_get::<i64, _>("max_iterations").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid max_iterations".to_string()))?,
        status: LocalTaskStatus::from_str(&status).map_err(ClientStorageError::InvalidState)?,
        active_run_id: row.try_get("active_run_id").db()?,
        version: u64::try_from(row.try_get::<i64, _>("version").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid task version".to_string()))?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
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
