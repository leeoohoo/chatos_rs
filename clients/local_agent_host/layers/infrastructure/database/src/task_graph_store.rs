// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{task_result::decode_result_summary, ClientStorageError, SqliteResultExt};
use chatos_local_agent_protocol::{
    CreateTaskGraphCommand, LocalTaskDependency, LocalTaskGraph, LocalTaskGraphStatus,
    LocalTaskRecord, LocalTaskStatus,
};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};
use std::{collections::HashSet, str::FromStr};

pub(super) async fn insert_graph(
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
    super::task_lifecycle::propagate_blocked(connection, &graph.graph_id, now_unix_ms).await?;
    super::task_lifecycle::unlock_satisfied(connection, &graph.graph_id, now_unix_ms).await?;
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
        "SELECT t.task_id, t.graph_id, t.title, t.profile_key, t.model_config_ref, \
         t.model_config_revision, t.capability_policy_revision, t.input_json, t.max_iterations, \
         t.status, t.active_run_id, t.version, t.created_at_unix_ms, t.updated_at_unix_ms, \
         t.cancel_reason, t.replacement_task_ids_json, t.cancelled_because_task_id, \
         (SELECT json_group_array(replacement.task_id) FROM local_tasks replacement \
            JOIN local_task_graphs replacement_graph \
              ON replacement_graph.graph_id = replacement.graph_id \
            CROSS JOIN json_each(replacement.input_json, '$.supersedes_task_ids') superseded \
            WHERE replacement_graph.owner_user_id = g.owner_user_id \
              AND superseded.value = t.task_id) AS derived_replacement_task_ids_json, \
         (SELECT f.state FROM local_task_followup_reviews f WHERE f.task_id = t.task_id) \
           AS followup_review_state, \
         (SELECT r.terminal_outcome_json FROM local_agent_runs r \
            WHERE r.owner_user_id = g.owner_user_id \
              AND r.owner_entity_type = 'task' AND r.owner_entity_id = t.task_id \
            ORDER BY r.created_at_unix_ms DESC, r.run_id DESC LIMIT 1) \
           AS latest_terminal_outcome_json \
         FROM local_tasks t JOIN local_task_graphs g ON g.graph_id = t.graph_id \
         WHERE t.graph_id = ? ORDER BY t.task_id",
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

pub(super) async fn fetch_graph_for_owner(
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
        input: super::task_followup_gate::task_input(&row)?,
        max_iterations: u32::try_from(row.try_get::<i64, _>("max_iterations").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid max_iterations".to_string()))?,
        status: LocalTaskStatus::from_str(&status).map_err(ClientStorageError::InvalidState)?,
        active_run_id: row.try_get("active_run_id").db()?,
        cancel_reason: row.try_get("cancel_reason").db()?,
        replacement_task_ids: {
            let mut ids = serde_json::from_str::<Vec<String>>(
                &row.try_get::<String, _>("replacement_task_ids_json").db()?,
            )?;
            ids.extend(serde_json::from_str::<Vec<String>>(
                &row.try_get::<String, _>("derived_replacement_task_ids_json")
                    .db()?,
            )?);
            ids.sort();
            ids.dedup();
            ids
        },
        cancelled_because_task_id: row.try_get("cancelled_because_task_id").db()?,
        result_summary: decode_result_summary(&row)?,
        version: u64::try_from(row.try_get::<i64, _>("version").db()?)
            .map_err(|_| ClientStorageError::InvalidState("invalid task version".to_string()))?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}
