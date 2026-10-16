// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    decode_run, ClientStorageError, IdempotentCommand, LocalRequirementSurveyStore,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalRequirementSurvey, LocalRequirementSurveyResolution, LocalRequirementSurveyStatus,
};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};

const SURVEY_SELECT: &str =
    "SELECT owner_user_id, survey_id, project_resource_id, source_conversation_id, \
     source_run_id, source_task_id, title, description, questions_json, answers_json, status, \
     version, created_at_unix_ms, updated_at_unix_ms, resolved_at_unix_ms \
     FROM local_requirement_surveys";

pub(crate) async fn reject_resume_with_open_survey(
    database: &mut SqliteConnection,
    run_id: &str,
) -> Result<(), ClientStorageError> {
    let survey_id: Option<String> = sqlx::query_scalar(
        "SELECT survey_id FROM local_requirement_surveys \
         WHERE source_run_id = ? AND status = 'open' \
         ORDER BY created_at_unix_ms, survey_id LIMIT 1",
    )
    .bind(run_id)
    .fetch_optional(&mut *database)
    .await
    .db()?;
    if let Some(survey_id) = survey_id {
        return Err(ClientStorageError::Conflict(format!(
            "Run {run_id} must be resumed by resolving requirement survey {survey_id}"
        )));
    }
    Ok(())
}

pub(crate) async fn delete_open_surveys_for_cancelled_run(
    database: &mut SqliteConnection,
    run_id: &str,
) -> Result<u64, ClientStorageError> {
    // Cancellation removes open forms; don't leave their Main Chat notice asking
    // for an answer that can no longer be submitted. Run reconciliation updates
    // the source conversation version in this same cancellation transaction.
    sqlx::query(
        "UPDATE local_conversation_messages SET content_json = json_set(content_json, '$.status', 'cancelled') \
         WHERE message_id IN (SELECT 'survey-status:' || survey_id FROM local_requirement_surveys \
           WHERE source_run_id = ? AND status = 'open')"
    ).bind(run_id).execute(&mut *database).await.db()?;
    Ok(sqlx::query(
        "DELETE FROM local_requirement_surveys WHERE source_run_id = ? AND status = 'open'",
    )
    .bind(run_id)
    .execute(&mut *database)
    .await
    .db()?
    .rows_affected())
}

#[async_trait]
impl LocalRequirementSurveyStore for SqliteClientStorage {
    async fn get_requirement_plan(
        &self,
        owner: &str,
        survey: &str,
        revision: Option<u64>,
    ) -> Result<Option<chatos_local_agent_protocol::LocalRequirementPlan>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        super::requirement_plan_store::get(&mut connection, owner, survey, revision).await
    }

    async fn save_requirement_plan(
        &self,
        command: &IdempotentCommand,
        save: &chatos_local_agent_protocol::SaveRequirementPlanCommand,
        now: i64,
    ) -> Result<chatos_local_agent_protocol::LocalRequirementPlan, ClientStorageError> {
        super::requirement_plan_store::save(self, command, save, now).await
    }

    async fn confirm_requirement_plan(
        &self,
        command: &IdempotentCommand,
        confirm: &chatos_local_agent_protocol::ConfirmRequirementPlanCommand,
        now: i64,
    ) -> Result<chatos_local_agent_protocol::LocalRequirementPlan, ClientStorageError> {
        super::requirement_plan_store::confirm(self, command, confirm, now).await
    }

    async fn create_requirement_survey(
        &self,
        command: &IdempotentCommand,
        survey: &LocalRequirementSurvey,
    ) -> Result<LocalRequirementSurvey, ClientStorageError> {
        let mut database = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut database).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut database, command).await? {
                return Ok(replay);
            }
            ensure_source_context(&mut database, survey).await?;
            let existing_open_survey: Option<String> = sqlx::query_scalar(
                "SELECT survey_id FROM local_requirement_surveys \
                 WHERE owner_user_id = ? AND source_run_id = ? AND status = 'open' \
                 ORDER BY created_at_unix_ms, survey_id LIMIT 1",
            )
            .bind(&survey.owner_user_id)
            .bind(&survey.source_run_id)
            .fetch_optional(&mut *database)
            .await
            .db()?;
            if let Some(existing_survey_id) = existing_open_survey {
                return Err(ClientStorageError::Conflict(format!(
                    "Run {} already has open requirement survey {existing_survey_id}",
                    survey.source_run_id
                )));
            }
            sqlx::query(
                "INSERT INTO local_requirement_surveys(\
                   owner_user_id, survey_id, project_resource_id, source_conversation_id, \
                   source_run_id, source_task_id, title, description, questions_json, \
                   answers_json, status, version, created_at_unix_ms, updated_at_unix_ms, \
                   resolved_at_unix_ms\
                 ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 'open', 1, ?, ?, NULL)",
            )
            .bind(&survey.owner_user_id)
            .bind(&survey.survey_id)
            .bind(&survey.project_resource_id)
            .bind(&survey.source_conversation_id)
            .bind(&survey.source_run_id)
            .bind(&survey.source_task_id)
            .bind(&survey.title)
            .bind(&survey.description)
            .bind(serde_json::to_string(&survey.questions)?)
            .bind(survey.created_at_unix_ms)
            .bind(survey.updated_at_unix_ms)
            .execute(&mut *database)
            .await
            .db()?;
            Self::record_receipt(&mut database, command, survey, survey.updated_at_unix_ms).await?;
            Ok(survey.clone())
        }
        .await;
        Self::finish_write(&mut database, result).await
    }

    async fn list_requirement_surveys(
        &self,
        owner_user_id: &str,
        project_resource_id: Option<&str>,
        status: Option<LocalRequirementSurveyStatus>,
        limit: u32,
    ) -> Result<Vec<LocalRequirementSurvey>, ClientStorageError> {
        let query = format!(
            "{SURVEY_SELECT} WHERE owner_user_id = ? \
             AND (? IS NULL OR project_resource_id = ?) \
             AND (? IS NULL OR status = ?) \
             ORDER BY updated_at_unix_ms DESC, survey_id DESC LIMIT ?"
        );
        let status = status.map(LocalRequirementSurveyStatus::as_str);
        sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(owner_user_id)
            .bind(project_resource_id)
            .bind(project_resource_id)
            .bind(status)
            .bind(status)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .db()?
            .iter()
            .map(decode_survey)
            .collect()
    }

    async fn get_requirement_survey(
        &self,
        owner_user_id: &str,
        survey_id: &str,
    ) -> Result<Option<LocalRequirementSurvey>, ClientStorageError> {
        let query = format!("{SURVEY_SELECT} WHERE owner_user_id = ? AND survey_id = ?");
        sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(owner_user_id)
            .bind(survey_id)
            .fetch_optional(&self.pool)
            .await
            .db()?
            .as_ref()
            .map(decode_survey)
            .transpose()
    }

    async fn resolve_requirement_survey(
        &self,
        command: &IdempotentCommand,
        survey: &LocalRequirementSurvey,
        expected_version: u64,
        run_event_id: &str,
    ) -> Result<LocalRequirementSurveyResolution, ClientStorageError> {
        let mut database = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut database).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut database, command).await? {
                return Ok(replay);
            }
            let updated = sqlx::query(
                "UPDATE local_requirement_surveys SET answers_json = ?, status = 'resolved', \
                   version = ?, updated_at_unix_ms = ?, resolved_at_unix_ms = ? \
                 WHERE owner_user_id = ? AND survey_id = ? AND status = 'open' AND version = ?",
            )
            .bind(serde_json::to_string(&survey.answers)?)
            .bind(to_i64(survey.version, "survey version")?)
            .bind(survey.updated_at_unix_ms)
            .bind(survey.resolved_at_unix_ms)
            .bind(&survey.owner_user_id)
            .bind(&survey.survey_id)
            .bind(to_i64(expected_version, "expected survey version")?)
            .execute(&mut *database)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(survey_write_conflict(
                    &mut database,
                    &survey.owner_user_id,
                    &survey.survey_id,
                )
                .await?);
            }
            let continuation = serde_json::json!({
                "type": "requirement_survey_resolved",
                "survey_id": survey.survey_id,
                "project_resource_id": survey.project_resource_id,
                "answers": survey.answers,
            });
            let resumed = sqlx::query(
                "UPDATE local_agent_runs SET status = 'continuation_ready', version = version + 1, \
                   continuation_input_json = ?, claim_token = NULL, claim_until_unix_ms = NULL, \
                   updated_at_unix_ms = ? \
                 WHERE run_id = ? AND owner_user_id = ? AND status = 'waiting_user'",
            )
            .bind(serde_json::to_string(&continuation)?)
            .bind(survey.updated_at_unix_ms)
            .bind(&survey.source_run_id)
            .bind(&survey.owner_user_id)
            .execute(&mut *database)
            .await
            .db()?;
            if resumed.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(
                    "the survey source Run must be waiting_user before resolution".to_string(),
                ));
            }
            write_back_main_chat_survey_status(
                &mut database,
                &survey.survey_id,
                true,
                survey.updated_at_unix_ms,
            )
            .await?;
            sqlx::query(
                "INSERT INTO local_agent_events(\
                   event_id, run_id, event_type, payload_json, created_at_unix_ms\
                 ) VALUES(?, ?, 'requirement_survey_resolved', ?, ?)",
            )
            .bind(run_event_id)
            .bind(&survey.source_run_id)
            .bind(serde_json::to_string(&continuation)?)
            .bind(survey.updated_at_unix_ms)
            .execute(&mut *database)
            .await
            .db()?;
            let run = sqlx::query(super::schema::RUN_SELECT)
                .bind(&survey.source_run_id)
                .fetch_one(&mut *database)
                .await
                .db()
                .and_then(decode_run)?;
            let resolution = LocalRequirementSurveyResolution {
                survey: survey.clone(),
                resumed_run: run,
            };
            Self::record_receipt(
                &mut database,
                command,
                &resolution,
                survey.updated_at_unix_ms,
            )
            .await?;
            Ok(resolution)
        }
        .await;
        Self::finish_write(&mut database, result).await
    }
}

pub(crate) async fn write_back_main_chat_survey_status(
    database: &mut SqliteConnection,
    survey_id: &str,
    resolved: bool,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let row = sqlx::query(
        "SELECT turn.turn_id, turn.conversation_id, survey.title \
         FROM local_requirement_surveys survey \
         JOIN local_agent_runs run ON run.run_id = survey.source_run_id \
         JOIN local_conversation_turns turn ON turn.run_id = run.run_id \
         WHERE survey.survey_id = ? AND run.profile_key = 'main_chat' \
           AND run.owner_entity_type = 'conversation_turn' \
           AND run.owner_entity_id = turn.turn_id \
           AND turn.conversation_id = survey.source_conversation_id",
    )
    .bind(survey_id)
    .fetch_optional(&mut *database)
    .await
    .db()?;
    let Some(row) = row else { return Ok(()) };
    let conversation_id: String = row.try_get("conversation_id").db()?;
    let turn_id: String = row.try_get("turn_id").db()?;
    let title: String = row.try_get("title").db()?;
    let content = serde_json::json!({
        "type": "requirement_survey_status", "survey_title": title,
        "status": if resolved { "resolved" } else { "open" }
    });
    let message_id = format!("survey-status:{survey_id}");
    let ordinal =
        super::conversation_store::next_message_ordinal(database, &conversation_id).await?;
    sqlx::query(
        "INSERT INTO local_conversation_messages(\
         message_id, conversation_id, turn_id, ordinal, role, content_json, metadata_json, created_at_unix_ms) \
         VALUES(?, ?, ?, ?, 'assistant', ?, ?, ?) \
         ON CONFLICT(message_id) DO UPDATE SET content_json = excluded.content_json"
    ).bind(message_id).bind(&conversation_id).bind(turn_id).bind(ordinal)
        .bind(serde_json::to_string(&content)?)
        .bind(serde_json::json!({"kind": "requirement_survey_status", "survey_id": survey_id}).to_string())
        .bind(now_unix_ms).execute(&mut *database).await.db()?;
    sqlx::query("UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? WHERE conversation_id = ?")
        .bind(now_unix_ms).bind(conversation_id).execute(&mut *database).await.db()?;
    Ok(())
}

async fn ensure_source_context(
    database: &mut SqliteConnection,
    survey: &LocalRequirementSurvey,
) -> Result<(), ClientStorageError> {
    let row = sqlx::query(
        "SELECT owner_user_id, owner_entity_type, owner_entity_id, input_json, status \
         FROM local_agent_runs WHERE run_id = ?",
    )
    .bind(&survey.source_run_id)
    .fetch_optional(&mut *database)
    .await
    .db()?
    .ok_or_else(|| ClientStorageError::NotFound(survey.source_run_id.clone()))?;
    let owner: String = row.try_get("owner_user_id").db()?;
    if owner != survey.owner_user_id {
        return Err(ClientStorageError::NotFound(survey.source_run_id.clone()));
    }
    let status: String = row.try_get("status").db()?;
    match status.as_str() {
        "waiting_user" => {}
        "waiting_tool_result" => {
            let matching_tool: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM local_agent_tool_invocations \
                 WHERE run_id = ? AND tool_name = 'requirement_survey_create' \
                   AND status = 'running' AND ('local-survey-' || invocation_id) = ?",
            )
            .bind(&survey.source_run_id)
            .bind(&survey.survey_id)
            .fetch_one(&mut *database)
            .await
            .db()?;
            if matching_tool != 1 {
                return Err(ClientStorageError::Conflict(
                    "survey does not match the active requirement_survey_create invocation"
                        .to_string(),
                ));
            }
        }
        _ => {
            return Err(ClientStorageError::Conflict(
                "survey source Run must be waiting_user or waiting_tool_result".to_string(),
            ));
        }
    }
    let input_json: String = row.try_get("input_json").db()?;
    let input: serde_json::Value = serde_json::from_str(&input_json)?;
    let conversation = input
        .get("source_conversation_id")
        .or_else(|| input.get("conversation_id"))
        .and_then(serde_json::Value::as_str);
    if conversation != Some(survey.source_conversation_id.as_str()) {
        return Err(ClientStorageError::Conflict(
            "survey source conversation does not match its Run".to_string(),
        ));
    }
    let project_binding: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_conversations \
         WHERE owner_user_id = ? AND conversation_id = ? \
           AND resource_kind = 'project' AND resource_id = ?",
    )
    .bind(&survey.owner_user_id)
    .bind(&survey.source_conversation_id)
    .bind(&survey.project_resource_id)
    .fetch_one(&mut *database)
    .await
    .db()?;
    if project_binding != 1 {
        return Err(ClientStorageError::Conflict(
            "survey project does not match the source conversation binding".to_string(),
        ));
    }
    if let Some(task_id) = survey.source_task_id.as_deref() {
        let entity_type: String = row.try_get("owner_entity_type").db()?;
        let entity_id: String = row.try_get("owner_entity_id").db()?;
        if entity_type != "task" || entity_id != task_id {
            return Err(ClientStorageError::Conflict(
                "survey source task does not match its Run".to_string(),
            ));
        }
    }
    Ok(())
}

fn decode_survey(row: &SqliteRow) -> Result<LocalRequirementSurvey, ClientStorageError> {
    Ok(LocalRequirementSurvey {
        owner_user_id: row.try_get("owner_user_id").db()?,
        survey_id: row.try_get("survey_id").db()?,
        project_resource_id: row.try_get("project_resource_id").db()?,
        source_conversation_id: row.try_get("source_conversation_id").db()?,
        source_run_id: row.try_get("source_run_id").db()?,
        source_task_id: row.try_get("source_task_id").db()?,
        title: row.try_get("title").db()?,
        description: row.try_get("description").db()?,
        questions: serde_json::from_str(row.try_get("questions_json").db()?)?,
        answers: row
            .try_get::<Option<String>, _>("answers_json")
            .db()?
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        status: parse_status(row.try_get("status").db()?)?,
        version: to_u64(row.try_get("version").db()?, "survey version")?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
        resolved_at_unix_ms: row.try_get("resolved_at_unix_ms").db()?,
    })
}

async fn survey_write_conflict(
    database: &mut SqliteConnection,
    owner_user_id: &str,
    survey_id: &str,
) -> Result<ClientStorageError, ClientStorageError> {
    let row = sqlx::query("SELECT status, version FROM local_requirement_surveys WHERE owner_user_id = ? AND survey_id = ?")
        .bind(owner_user_id)
        .bind(survey_id)
        .fetch_optional(&mut *database)
        .await
        .db()?;
    Ok(match row {
        None => ClientStorageError::NotFound(survey_id.to_string()),
        Some(row) => ClientStorageError::Conflict(format!(
            "requirement survey is {} at version {}",
            row.try_get::<String, _>("status").db()?,
            row.try_get::<i64, _>("version").db()?
        )),
    })
}

fn to_i64(value: u64, name: &str) -> Result<i64, ClientStorageError> {
    i64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("{name} exceeds SQLite INTEGER")))
}

fn to_u64(value: i64, name: &str) -> Result<u64, ClientStorageError> {
    u64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("{name} is negative")))
}

fn parse_status(value: &str) -> Result<LocalRequirementSurveyStatus, ClientStorageError> {
    match value {
        "open" => Ok(LocalRequirementSurveyStatus::Open),
        "resolved" => Ok(LocalRequirementSurveyStatus::Resolved),
        other => Err(ClientStorageError::InvalidState(format!(
            "unknown requirement survey status: {other}"
        ))),
    }
}
