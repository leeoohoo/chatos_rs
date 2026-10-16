// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, IdempotentCommand, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{
    ConfirmRequirementPlanCommand, CreateTaskGraphCommand, LocalAgentRunRecord,
    LocalAgentRunStatus, LocalRequirementPlan, SaveRequirementPlanCommand,
    LOCAL_AGENT_MAX_INPUT_BYTES, LOCAL_REQUIREMENT_PLAN_MAX_BYTES,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};

const SELECT: &str = "SELECT owner_user_id, plan_id, revision, project_resource_id, \
    source_conversation_id, source_survey_id, source_run_id, title, markdown, \
    created_at_unix_ms, confirmed_at_unix_ms FROM local_requirement_plans";

pub(super) async fn get(
    connection: &mut SqliteConnection,
    owner: &str,
    survey: &str,
    revision: Option<u64>,
) -> Result<Option<LocalRequirementPlan>, ClientStorageError> {
    let revision = revision
        .map(|value| {
            i64::try_from(value)
                .map_err(|_| ClientStorageError::InvalidState("plan revision overflow".into()))
        })
        .transpose()?;
    let query = format!(
        "{SELECT} WHERE owner_user_id = ? AND source_survey_id = ? \
        AND (? IS NULL OR revision = ?) ORDER BY revision DESC LIMIT 1"
    );
    sqlx::query(sqlx::AssertSqlSafe(query))
        .bind(owner)
        .bind(survey)
        .bind(revision)
        .bind(revision)
        .fetch_optional(&mut *connection)
        .await
        .db()?
        .as_ref()
        .map(decode)
        .transpose()
}

pub(super) async fn latest_for_project(
    connection: &mut SqliteConnection,
    owner: &str,
    project: &str,
) -> Result<Option<LocalRequirementPlan>, ClientStorageError> {
    let query = format!("{SELECT} WHERE owner_user_id = ? AND project_resource_id = ? \
        AND revision = (SELECT MAX(p.revision) FROM local_requirement_plans p \
            WHERE p.owner_user_id = local_requirement_plans.owner_user_id AND p.plan_id = local_requirement_plans.plan_id) \
        ORDER BY created_at_unix_ms DESC, revision DESC, plan_id DESC LIMIT 1");
    sqlx::query(sqlx::AssertSqlSafe(query))
        .bind(owner)
        .bind(project)
        .fetch_optional(&mut *connection)
        .await
        .db()?
        .as_ref()
        .map(decode)
        .transpose()
}

pub(super) async fn save(
    store: &SqliteClientStorage,
    command: &IdempotentCommand,
    save: &SaveRequirementPlanCommand,
    now: i64,
) -> Result<LocalRequirementPlan, ClientStorageError> {
    save.validate().map_err(ClientStorageError::InvalidState)?;
    let mut connection = store.pool.acquire().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? { return Ok(replay); }
        let run = SqliteClientStorage::fetch_run_on(&mut connection, &save.source_run_id).await?
            .ok_or_else(|| ClientStorageError::NotFound(save.source_run_id.clone()))?;
        let survey = sqlx::query("SELECT project_resource_id, source_conversation_id, source_run_id, source_task_id, title \
            FROM local_requirement_surveys WHERE owner_user_id = ? AND survey_id = ? AND status = 'resolved'")
            .bind(&save.owner_user_id).bind(&save.survey_id).fetch_optional(&mut *connection).await.db()?
            .ok_or_else(|| ClientStorageError::NotFound(save.survey_id.clone()))?;
        let project: String = survey.try_get("project_resource_id").db()?;
        let mut conversation: String = survey.try_get("source_conversation_id").db()?;
        if run.owner_user_id != save.owner_user_id { return Err(ClientStorageError::NotFound(save.source_run_id.clone())); }
        match (run.profile_key.as_str(), run.owner_entity_type.as_str()) {
            ("main_chat", "conversation_turn") => {
                let valid: Option<String> = sqlx::query_scalar("SELECT c.conversation_id FROM local_conversation_turns t JOIN local_conversations c \
                    ON c.conversation_id = t.conversation_id WHERE t.run_id = ? AND t.turn_id = ? AND c.owner_user_id = ? \
                    AND c.resource_kind = 'project' AND c.resource_id = ?")
                    .bind(&run.run_id).bind(&run.owner_entity_id).bind(&run.owner_user_id).bind(&project)
                    .fetch_optional(&mut *connection).await.db()?;
                conversation = valid.ok_or_else(|| ClientStorageError::InvalidState("plan source Run belongs to another project or conversation".into()))?;
            }
            ("task_execution", "task") if survey.try_get::<String,_>("source_run_id").db()? == run.run_id
                && survey.try_get::<Option<String>,_>("source_task_id").db()?.as_deref() == Some(run.owner_entity_id.as_str()) => {}
            _ => return Err(ClientStorageError::InvalidState("plan requires its source Task or a project Main Chat Run".into())),
        }
        if !matches!(run.status, LocalAgentRunStatus::ModelRunning | LocalAgentRunStatus::WaitingToolResult | LocalAgentRunStatus::Succeeded) {
            return Err(ClientStorageError::InvalidState("only active generation or its successful final reply can publish a plan".into()));
        }
        if run.status == LocalAgentRunStatus::Succeeded {
            let reply = run.terminal_outcome.as_ref().and_then(|outcome| ["content", "answer", "text", "output"].iter()
                .find_map(|key| outcome.get(key).and_then(Value::as_str).filter(|text| !text.trim().is_empty())));
            if reply.map(str::trim) != Some(save.markdown.trim()) {
                return Err(ClientStorageError::InvalidState("a completed Run can only publish its exact final reply; use a new generation Run to revise a plan".into()));
            }
        }
        let current = get(&mut connection, &save.owner_user_id, &save.survey_id, None).await?;
        if current.as_ref().map_or(0, |plan| plan.revision) != save.expected_revision {
            return Err(ClientStorageError::Conflict("plan changed; load the latest revision before saving".into()));
        }
        let plan = LocalRequirementPlan {
            plan_id: format!("requirement-plan:{}", save.survey_id), revision: save.expected_revision + 1,
            owner_user_id: save.owner_user_id.clone(), project_resource_id: project,
            source_conversation_id: conversation, source_survey_id: save.survey_id.clone(),
            source_run_id: run.run_id, title: survey.try_get("title").db()?,
            markdown: save.markdown.trim().to_owned(), created_at_unix_ms: now, confirmed_at_unix_ms: None,
        };
        insert(&mut connection, &plan).await?;
        SqliteClientStorage::insert_event(&mut connection, &uuid::Uuid::new_v4().to_string(), &plan.source_run_id,
            "requirement_plan_saved", &json!({"survey_id": plan.source_survey_id, "revision": plan.revision}), now).await?;
        SqliteClientStorage::record_receipt(&mut connection, command, &plan, now).await?;
        Ok(plan)
    }.await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

pub(super) async fn confirm(
    store: &SqliteClientStorage,
    command: &IdempotentCommand,
    confirm: &ConfirmRequirementPlanCommand,
    now: i64,
) -> Result<LocalRequirementPlan, ClientStorageError> {
    confirm
        .validate()
        .map_err(ClientStorageError::InvalidState)?;
    let mut connection = store.pool.acquire().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? { return Ok(replay); }
        let mut plan = get(&mut connection, &confirm.owner_user_id, &confirm.survey_id, None).await?
            .ok_or_else(|| ClientStorageError::NotFound(confirm.survey_id.clone()))?;
        if plan.revision != confirm.expected_revision {
            return Err(ClientStorageError::Conflict("plan changed; review the latest revision before confirming".into()));
        }
        let source = SqliteClientStorage::fetch_run_on(&mut connection, &plan.source_run_id).await?
            .ok_or_else(|| ClientStorageError::NotFound(plan.source_run_id.clone()))?;
        if source.status != LocalAgentRunStatus::Succeeded {
            return Err(ClientStorageError::Conflict("plan generation has not successfully completed".into()));
        }
        if plan.confirmed_at_unix_ms.is_none() {
            sqlx::query("UPDATE local_requirement_plans SET confirmed_at_unix_ms = ? \
                WHERE owner_user_id = ? AND plan_id = ? AND revision = ? AND confirmed_at_unix_ms IS NULL")
                .bind(now).bind(&plan.owner_user_id).bind(&plan.plan_id).bind(plan.revision as i64)
                .execute(&mut *connection).await.db()?;
            plan.confirmed_at_unix_ms = Some(now);
            SqliteClientStorage::insert_event(&mut connection, &uuid::Uuid::new_v4().to_string(), &plan.source_run_id,
                "requirement_plan_confirmed", &json!({"survey_id": plan.source_survey_id, "revision": plan.revision}), now).await?;
        }
        SqliteClientStorage::record_receipt(&mut connection, command, &plan, now).await?;
        Ok(plan)
    }.await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

/// Legacy/ordinary final replies remain in conversation history. Only a resolved
/// survey's exact successful source Run produces the initial unconfirmed draft.
pub(super) async fn capture_source_reply(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    now: i64,
) -> Result<(), ClientStorageError> {
    if run.status != LocalAgentRunStatus::Succeeded {
        return Ok(());
    }
    let Some(markdown) = run.terminal_outcome.as_ref().and_then(|outcome| {
        ["content", "answer", "text", "output"]
            .iter()
            .find_map(|key| {
                outcome
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
            })
    }) else {
        return Ok(());
    };
    if markdown.len() > LOCAL_REQUIREMENT_PLAN_MAX_BYTES || markdown.contains('\0') {
        return Ok(());
    }
    let surveys = sqlx::query("SELECT survey_id, project_resource_id, source_conversation_id, title \
        FROM local_requirement_surveys WHERE owner_user_id = ? AND source_run_id = ? AND status = 'resolved'")
        .bind(&run.owner_user_id).bind(&run.run_id).fetch_all(&mut *connection).await.db()?;
    for survey in surveys {
        let survey_id: String = survey.try_get("survey_id").db()?;
        // Explicit publish wins. Never replace it with a short final acknowledgement.
        if get(connection, &run.owner_user_id, &survey_id, None)
            .await?
            .is_some()
        {
            continue;
        }
        let plan = LocalRequirementPlan {
            plan_id: format!("requirement-plan:{survey_id}"),
            revision: 1,
            owner_user_id: run.owner_user_id.clone(),
            project_resource_id: survey.try_get("project_resource_id").db()?,
            source_conversation_id: survey.try_get("source_conversation_id").db()?,
            source_survey_id: survey_id,
            source_run_id: run.run_id.clone(),
            title: survey.try_get("title").db()?,
            markdown: markdown.trim().into(),
            created_at_unix_ms: now,
            confirmed_at_unix_ms: None,
        };
        insert(connection, &plan).await?;
    }
    Ok(())
}

/// Read and freeze in the graph-creation transaction. Model input cannot supply
/// or change confirmation, body, provenance or the selected revision.
pub(super) async fn freeze_for_graph(
    connection: &mut SqliteConnection,
    graph: &mut CreateTaskGraphCommand,
) -> Result<(), ClientStorageError> {
    if graph.source_entity_type != "conversation_turn" {
        return reject_unbound_snapshot(graph);
    }
    let row = sqlx::query(
        "SELECT c.resource_id, t.run_id FROM local_conversation_turns t \
        JOIN local_conversations c ON c.conversation_id = t.conversation_id WHERE t.turn_id = ? \
        AND c.owner_user_id = ? AND c.resource_kind = 'project'",
    )
    .bind(&graph.source_entity_id)
    .bind(&graph.owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(row) = row else {
        return reject_unbound_snapshot(graph);
    };
    let project: String = row.try_get("resource_id").db()?;
    let source_run_id: String = row.try_get("run_id").db()?;
    let plan = latest_for_project(connection, &graph.owner_user_id, &project).await?;
    if let Some(plan) = plan {
        if plan.confirmed_at_unix_ms.is_none() {
            return Err(ClientStorageError::Conflict("requirement plan is not confirmed; ask the user to review and confirm its current revision in Requirement Survey before creating Tasks".into()));
        }
        let source = SqliteClientStorage::fetch_run_on(connection, &source_run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(source_run_id.clone()))?;
        let bound = source.input.get("requirement_plan");
        if bound
            .and_then(|value| value.get("plan_id"))
            .and_then(Value::as_str)
            != Some(plan.plan_id.as_str())
            || bound
                .and_then(|value| value.get("revision"))
                .and_then(Value::as_u64)
                != Some(plan.revision)
            || bound
                .and_then(|value| value.get("confirmed_at_unix_ms"))
                .and_then(Value::as_i64)
                != plan.confirmed_at_unix_ms
        {
            return Err(ClientStorageError::Conflict("requirement plan changed since this Main Chat started; review the current plan in a new turn before creating Tasks".into()));
        }
        let snapshot = serde_json::to_value(&plan)?;
        let hash = format!("{:x}", Sha256::digest(plan.markdown.as_bytes()));
        for task in &mut graph.tasks {
            let object = task.input.as_object_mut().ok_or_else(|| {
                ClientStorageError::InvalidState(
                    "Task input must be an object when binding a plan".into(),
                )
            })?;
            let scope = json!({"title": task.title, "prompt": object.get("prompt"),
                "expected_outputs": object.get("expected_outputs"), "acceptance_criteria": object.get("acceptance_criteria")});
            object.insert(
                "requirement_plan".into(),
                json!({"snapshot": snapshot, "markdown_sha256": hash, "task_scope": scope}),
            );
            if serde_json::to_vec(&task.input)?.len() > LOCAL_AGENT_MAX_INPUT_BYTES {
                return Err(ClientStorageError::InvalidState(
                    "Task input with frozen plan exceeds its byte limit".into(),
                ));
            }
        }
    } else {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_requirement_surveys \
            WHERE owner_user_id = ? AND source_run_id = ? AND status = 'resolved'",
        )
        .bind(&graph.owner_user_id)
        .bind(source_run_id)
        .fetch_one(&mut *connection)
        .await
        .db()?;
        if count > 0 {
            return Err(ClientStorageError::Conflict("publish a requirement plan with requirement_plan_save and wait for the user's confirmation before creating Tasks".into()));
        }
        reject_unbound_snapshot(graph)?;
    }
    Ok(())
}

fn reject_unbound_snapshot(graph: &CreateTaskGraphCommand) -> Result<(), ClientStorageError> {
    if graph
        .tasks
        .iter()
        .any(|task| task.input.get("requirement_plan").is_some())
    {
        return Err(ClientStorageError::InvalidState(
            "model-supplied requirement plan is not authoritative".into(),
        ));
    }
    Ok(())
}

async fn insert(
    connection: &mut SqliteConnection,
    plan: &LocalRequirementPlan,
) -> Result<(), ClientStorageError> {
    sqlx::query("INSERT INTO local_requirement_plans(owner_user_id, plan_id, revision, project_resource_id, \
        source_conversation_id, source_survey_id, source_run_id, title, markdown, created_at_unix_ms, confirmed_at_unix_ms) \
        VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)")
        .bind(&plan.owner_user_id).bind(&plan.plan_id).bind(plan.revision as i64).bind(&plan.project_resource_id)
        .bind(&plan.source_conversation_id).bind(&plan.source_survey_id).bind(&plan.source_run_id).bind(&plan.title)
        .bind(&plan.markdown).bind(plan.created_at_unix_ms).execute(&mut *connection).await.db()?;
    Ok(())
}

fn decode(row: &SqliteRow) -> Result<LocalRequirementPlan, ClientStorageError> {
    let revision: i64 = row.try_get("revision").db()?;
    Ok(LocalRequirementPlan {
        owner_user_id: row.try_get("owner_user_id").db()?,
        plan_id: row.try_get("plan_id").db()?,
        revision: u64::try_from(revision)
            .map_err(|_| ClientStorageError::InvalidState("invalid plan revision".into()))?,
        project_resource_id: row.try_get("project_resource_id").db()?,
        source_conversation_id: row.try_get("source_conversation_id").db()?,
        source_survey_id: row.try_get("source_survey_id").db()?,
        source_run_id: row.try_get("source_run_id").db()?,
        title: row.try_get("title").db()?,
        markdown: row.try_get("markdown").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        confirmed_at_unix_ms: row.try_get("confirmed_at_unix_ms").db()?,
    })
}
