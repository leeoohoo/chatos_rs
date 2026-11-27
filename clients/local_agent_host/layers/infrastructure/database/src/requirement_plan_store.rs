// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, IdempotentCommand, SqliteClientStorage, SqliteResultExt};
use chatos_local_agent_protocol::{
    ConfirmRequirementPlanCommand, CreateTaskGraphCommand, LocalAgentRunRecord,
    LocalAgentRunStatus, LocalPluginTaskPolicy, LocalRequirementPlan, SaveRequirementPlanCommand,
    LOCAL_AGENT_MAX_INPUT_BYTES, LOCAL_REQUIREMENT_PLAN_MAX_BYTES,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};

const SELECT: &str = "SELECT owner_user_id, plan_id, revision, project_resource_id, \
    source_conversation_id, source_survey_id, source_run_id, title, markdown, \
    created_at_unix_ms, confirmed_at_unix_ms, consumed_at_unix_ms \
    FROM local_requirement_plans";

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

pub(super) async fn latest_for_scope(
    connection: &mut SqliteConnection,
    owner: &str,
    scope_id: &str,
) -> Result<Option<LocalRequirementPlan>, ClientStorageError> {
    let query = format!("SELECT * FROM ({SELECT} WHERE owner_user_id = ? AND project_resource_id = ? \
        AND revision = (SELECT MAX(p.revision) FROM local_requirement_plans p \
            WHERE p.owner_user_id = local_requirement_plans.owner_user_id AND p.plan_id = local_requirement_plans.plan_id) \
        ORDER BY created_at_unix_ms DESC, revision DESC, plan_id DESC LIMIT 1) \
        WHERE consumed_at_unix_ms IS NULL");
    sqlx::query(sqlx::AssertSqlSafe(query))
        .bind(owner)
        .bind(scope_id)
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
    let mut connection = store.acquire_write().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? { return Ok(replay); }
        let run = SqliteClientStorage::fetch_run_on(&mut connection, &save.source_run_id).await?
            .ok_or_else(|| ClientStorageError::NotFound(save.source_run_id.clone()))?;
        if run.owner_user_id != save.owner_user_id { return Err(ClientStorageError::NotFound(save.source_run_id.clone())); }
        ensure_direct_plan_review_survey(&mut connection, save, &run, now).await?;
        let survey = sqlx::query("SELECT project_resource_id, source_conversation_id, source_run_id, source_task_id, title \
            FROM local_requirement_surveys WHERE owner_user_id = ? AND survey_id = ? AND status = 'resolved'")
            .bind(&save.owner_user_id).bind(&save.survey_id).fetch_optional(&mut *connection).await.db()?
            .ok_or_else(|| ClientStorageError::NotFound(save.survey_id.clone()))?;
        let project: String = survey.try_get("project_resource_id").db()?;
        let mut conversation: String = survey.try_get("source_conversation_id").db()?;
        let survey_source_task: Option<String> = survey.try_get("source_task_id").db()?;
        match (run.profile_key.as_str(), run.owner_entity_type.as_str()) {
            ("main_chat", "conversation_turn") => {
                let valid: Option<String> = sqlx::query_scalar("SELECT c.conversation_id FROM local_conversation_turns t JOIN local_conversations c \
                    ON c.conversation_id = t.conversation_id WHERE t.run_id = ? AND t.turn_id = ? AND c.owner_user_id = ? \
                    AND CASE WHEN c.resource_kind = 'project' THEN c.resource_id \
                             ELSE 'conversation:' || c.conversation_id END = ?")
                    .bind(&run.run_id).bind(&run.owner_entity_id).bind(&run.owner_user_id).bind(&project)
                    .fetch_optional(&mut *connection).await.db()?;
                conversation = valid.ok_or_else(|| ClientStorageError::InvalidState("plan source Run belongs to another resource or conversation".into()))?;
            }
            ("task_execution", "task") if survey.try_get::<String,_>("source_run_id").db()? == run.run_id
                && survey_source_task.as_deref() == Some(run.owner_entity_id.as_str()) => {}
            ("task_execution", "conversation_turn") if run.input.get("team_executor_context")
                .and_then(Value::as_object)
                .is_some_and(|team| {
                    team.get("todo_id").and_then(Value::as_str)
                        == survey_source_task.as_deref()
                        && team.get("project_id").and_then(Value::as_str) == Some(project.as_str())
                }) => {
                let source_conversation = run.input.get("conversation_id").and_then(Value::as_str)
                    .ok_or_else(|| ClientStorageError::InvalidState("team plan source Run has no conversation".into()))?;
                let valid: Option<String> = sqlx::query_scalar("SELECT c.conversation_id FROM local_conversation_turns t JOIN local_conversations c \
                    ON c.conversation_id = t.conversation_id WHERE t.run_id = ? AND t.turn_id = ? AND c.owner_user_id = ? \
                    AND c.conversation_id = ? AND c.resource_kind IS NULL")
                    .bind(&run.run_id).bind(&run.owner_entity_id).bind(&run.owner_user_id).bind(source_conversation)
                    .fetch_optional(&mut *connection).await.db()?;
                conversation = valid.ok_or_else(|| ClientStorageError::InvalidState("team plan source Run does not match its internal conversation turn".into()))?;
            }
            _ => return Err(ClientStorageError::InvalidState("plan requires a resource-bound Main Chat, Task, or team Todo source Run".into())),
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

async fn ensure_direct_plan_review_survey(
    connection: &mut SqliteConnection,
    save: &SaveRequirementPlanCommand,
    run: &LocalAgentRunRecord,
    now: i64,
) -> Result<(), ClientStorageError> {
    if save.survey_id != format!("plan-review:{}", run.run_id) {
        return Ok(());
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM local_requirement_surveys \
         WHERE owner_user_id = ? AND survey_id = ?)",
    )
    .bind(&save.owner_user_id)
    .bind(&save.survey_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    if exists {
        return Ok(());
    }
    if run.profile_key != "task_execution"
        || run.owner_entity_type != "task"
        || run.input.get("task_kind").and_then(Value::as_str) != Some("planning")
        || active_plugin_task_policies(connection, &save.owner_user_id)
            .await?
            .is_empty()
    {
        return Err(ClientStorageError::InvalidState(
            "direct plan publishing requires a planning Task authorized by an active Plugin task policy".into(),
        ));
    }
    let source = sqlx::query(
        "SELECT task.title, turn.conversation_id, \
                CASE WHEN conversation.resource_kind = 'project' THEN conversation.resource_id \
                     ELSE 'conversation:' || conversation.conversation_id END AS plan_scope_id \
         FROM local_tasks task \
         JOIN local_task_graphs graph ON graph.graph_id = task.graph_id \
         JOIN local_conversation_turns turn \
           ON graph.source_entity_type = 'conversation_turn' \
          AND turn.turn_id = graph.source_entity_id \
         JOIN local_conversations conversation \
           ON conversation.conversation_id = turn.conversation_id \
         WHERE task.task_id = ? AND graph.owner_user_id = ? \
           AND conversation.owner_user_id = ?",
    )
    .bind(&run.owner_entity_id)
    .bind(&save.owner_user_id)
    .bind(&save.owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .ok_or_else(|| {
        ClientStorageError::InvalidState(
            "direct plan publishing requires a conversation-bound coordinator".into(),
        )
    })?;
    let title: String = source.try_get("title").db()?;
    let conversation_id: String = source.try_get("conversation_id").db()?;
    let project_resource_id: String = source.try_get("plan_scope_id").db()?;
    sqlx::query(
        "INSERT INTO local_requirement_surveys(\
         owner_user_id, survey_id, project_resource_id, source_conversation_id, \
         source_run_id, source_task_id, title, description, questions_json, answers_json, \
         status, version, created_at_unix_ms, updated_at_unix_ms, resolved_at_unix_ms) \
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, '[]', '{}', 'resolved', 1, ?, ?, ?)",
    )
    .bind(&save.owner_user_id)
    .bind(&save.survey_id)
    .bind(project_resource_id)
    .bind(conversation_id)
    .bind(&run.run_id)
    .bind(&run.owner_entity_id)
    .bind(format!("方案评审：{title}"))
    .bind("Planning Task 已从现有需求与项目证据生成方案，无需补充问卷。")
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
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
    let mut connection = store.acquire_write().await.db()?;
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
            let (mut coordinator, coordinator_turn_id, coordinator_conversation_id) =
                coordinator_for_plan(&mut connection, &plan, &source).await?;
            if !matches!(
                coordinator.status,
                LocalAgentRunStatus::Succeeded | LocalAgentRunStatus::Failed
            ) {
                return Err(ClientStorageError::Conflict(
                    "the plan coordinator is not ready to resume".into(),
                ));
            }
            let latest_turn_id: Option<String> = sqlx::query_scalar(
                "SELECT turn_id FROM local_conversation_turns WHERE conversation_id = ? \
                 ORDER BY created_at_unix_ms DESC, rowid DESC LIMIT 1",
            )
            .bind(&coordinator_conversation_id)
            .fetch_optional(&mut *connection)
            .await
            .db()?;
            if latest_turn_id.as_deref() != Some(coordinator_turn_id.as_str()) {
                return Err(ClientStorageError::Conflict(
                    "the source conversation has a newer turn; review the plan from the latest turn"
                        .into(),
                ));
            }
            sqlx::query("UPDATE local_requirement_plans SET confirmed_at_unix_ms = ? \
                WHERE owner_user_id = ? AND plan_id = ? AND revision = ? AND confirmed_at_unix_ms IS NULL")
                .bind(now).bind(&plan.owner_user_id).bind(&plan.plan_id).bind(plan.revision as i64)
                .execute(&mut *connection).await.db()?;
            plan.confirmed_at_unix_ms = Some(now);
            let plan_value = serde_json::to_value(&plan)?;
            coordinator
                .input
                .as_object_mut()
                .ok_or_else(|| {
                    ClientStorageError::InvalidState(
                        "plan coordinator input must be an object".into(),
                    )
                })?
                .insert("requirement_plan".into(), plan_value.clone());
            let requested = json!({
                "type": "resume",
                "reason": "requirement_plan_confirmed",
                "input": {
                    "message": "The user confirmed the current requirement plan. Continue this same workflow now: create the execution Task Graph from the Host-bound plan without asking the user to send another message.",
                    "requirement_plan": plan_value,
                }
            });
            let continuation = super::run_continuation::for_resume(
                &mut connection,
                &coordinator,
                &requested,
            )
            .await?;
            let resumed = sqlx::query(
                "UPDATE local_agent_runs SET input_json = ?, status = 'continuation_ready', \
                 version = version + 1, continuation_input_json = ?, \
                 terminal_outcome_json = NULL, claim_token = NULL, claim_until_unix_ms = NULL, \
                 updated_at_unix_ms = ? \
                 WHERE run_id = ? AND version = ? AND status IN ('succeeded','failed')",
            )
            .bind(serde_json::to_string(&coordinator.input)?)
            .bind(serde_json::to_string(&continuation)?)
            .bind(now)
            .bind(&coordinator.run_id)
            .bind(coordinator.version as i64)
            .execute(&mut *connection)
            .await
            .db()?;
            if resumed.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(
                    "the plan coordinator changed while confirmation was being applied".into(),
                ));
            }
            let reopened = sqlx::query(
                "UPDATE local_conversation_turns SET status = 'running', updated_at_unix_ms = ? \
                 WHERE turn_id = ? AND run_id = ? AND status IN ('succeeded','failed')",
            )
            .bind(now)
            .bind(&coordinator_turn_id)
            .bind(&coordinator.run_id)
            .execute(&mut *connection)
            .await
            .db()?;
            if reopened.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(
                    "the plan coordinator turn changed while confirmation was being applied".into(),
                ));
            }
            sqlx::query(
                "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
                 WHERE conversation_id = ?",
            )
            .bind(now)
            .bind(&coordinator_conversation_id)
            .execute(&mut *connection)
            .await
            .db()?;
            SqliteClientStorage::insert_event(&mut connection, &uuid::Uuid::new_v4().to_string(), &plan.source_run_id,
                "requirement_plan_confirmed", &json!({"survey_id": plan.source_survey_id, "revision": plan.revision}), now).await?;
            SqliteClientStorage::insert_event(
                &mut connection,
                &format!("requirement-plan-dispatch:{}:{}", plan.plan_id, plan.revision),
                &coordinator.run_id,
                "requirement_plan_dispatch_scheduled",
                &json!({
                    "survey_id": plan.source_survey_id,
                    "plan_id": plan.plan_id,
                    "revision": plan.revision,
                    "conversation_id": coordinator_conversation_id,
                    "turn_id": coordinator_turn_id,
                }),
                now,
            )
            .await?;
        }
        SqliteClientStorage::record_receipt(&mut connection, command, &plan, now).await?;
        Ok(plan)
    }.await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

async fn coordinator_for_plan(
    connection: &mut SqliteConnection,
    plan: &LocalRequirementPlan,
    source: &LocalAgentRunRecord,
) -> Result<(LocalAgentRunRecord, String, String), ClientStorageError> {
    let row = match source.owner_entity_type.as_str() {
        "conversation_turn" => sqlx::query(
            "SELECT turn.run_id, turn.turn_id, turn.conversation_id \
             FROM local_conversation_turns turn \
             JOIN local_conversations conversation \
               ON conversation.conversation_id = turn.conversation_id \
             WHERE turn.turn_id = ? AND turn.run_id = ? \
               AND conversation.owner_user_id = ?",
        )
        .bind(&source.owner_entity_id)
        .bind(&source.run_id)
        .bind(&plan.owner_user_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?,
        "task" => sqlx::query(
            "SELECT turn.run_id, turn.turn_id, turn.conversation_id \
             FROM local_tasks task \
             JOIN local_task_graphs graph ON graph.graph_id = task.graph_id \
             JOIN local_conversation_turns turn \
               ON graph.source_entity_type = 'conversation_turn' \
              AND turn.turn_id = graph.source_entity_id \
             WHERE task.task_id = ? AND graph.owner_user_id = ?",
        )
        .bind(&source.owner_entity_id)
        .bind(&plan.owner_user_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?,
        _ => None,
    }
    .ok_or_else(|| {
        ClientStorageError::InvalidState(
            "requirement plan has no resumable coordinator conversation turn".into(),
        )
    })?;
    let run_id: String = row.try_get("run_id").db()?;
    let turn_id: String = row.try_get("turn_id").db()?;
    let conversation_id: String = row.try_get("conversation_id").db()?;
    if conversation_id != plan.source_conversation_id {
        return Err(ClientStorageError::InvalidState(
            "requirement plan coordinator belongs to another conversation".into(),
        ));
    }
    let run = SqliteClientStorage::fetch_run_on(connection, &run_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
    if run.owner_user_id != plan.owner_user_id {
        return Err(ClientStorageError::InvalidState(
            "requirement plan coordinator belongs to another owner".into(),
        ));
    }
    Ok((run, turn_id, conversation_id))
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
    now: i64,
) -> Result<(), ClientStorageError> {
    if graph.source_entity_type != "conversation_turn" {
        return reject_unbound_snapshot(graph);
    }
    let row = sqlx::query(
        "SELECT CASE WHEN c.resource_kind = 'project' THEN c.resource_id \
                     ELSE 'conversation:' || c.conversation_id END AS plan_scope_id, \
                t.run_id \
         FROM local_conversation_turns t \
         JOIN local_conversations c ON c.conversation_id = t.conversation_id \
         WHERE t.turn_id = ? AND c.owner_user_id = ?",
    )
    .bind(&graph.source_entity_id)
    .bind(&graph.owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(row) = row else {
        return reject_unbound_snapshot(graph);
    };
    let plan_scope_id: String = row.try_get("plan_scope_id").db()?;
    let source_run_id: String = row.try_get("run_id").db()?;
    let plugin_task_policies =
        active_plugin_task_policies(connection, &graph.owner_user_id).await?;
    let requires_confirmed_plan = plugin_task_policies
        .iter()
        .any(|policy| policy.require_confirmed_plan_before_execution);
    let planning_tasks = graph
        .tasks
        .iter()
        .filter(|task| task.input.get("task_kind").and_then(Value::as_str) == Some("planning"))
        .count();
    if planning_tasks > 0 {
        if plugin_task_policies.is_empty() {
            return reject_unbound_snapshot(graph);
        }
        validate_planning_graph(graph, &plugin_task_policies)?;
        return reject_unbound_snapshot(graph);
    }
    let plan = latest_for_scope(connection, &graph.owner_user_id, &plan_scope_id).await?;
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
        let consumed = sqlx::query(
            "UPDATE local_requirement_plans SET consumed_at_unix_ms = ? \
             WHERE owner_user_id = ? AND plan_id = ? AND revision = ? \
               AND consumed_at_unix_ms IS NULL",
        )
        .bind(now)
        .bind(&plan.owner_user_id)
        .bind(&plan.plan_id)
        .bind(
            i64::try_from(plan.revision)
                .map_err(|_| ClientStorageError::InvalidState("plan revision overflow".into()))?,
        )
        .execute(&mut *connection)
        .await
        .db()?;
        if consumed.rows_affected() != 1 {
            return Err(ClientStorageError::Conflict(
                "requirement plan was already consumed by another Task Graph".into(),
            ));
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
        if requires_confirmed_plan {
            return Err(ClientStorageError::Conflict(
                "an active Plugin task policy requires a planning Task before execution Tasks"
                    .into(),
            ));
        }
        reject_unbound_snapshot(graph)?;
    }
    Ok(())
}

async fn active_plugin_task_policies(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
) -> Result<Vec<LocalPluginTaskPolicy>, ClientStorageError> {
    let specs: Vec<String> = sqlx::query_scalar(
        "SELECT bundle.spec_json FROM local_plugin_hook_bundles bundle \
         JOIN local_plugin_hook_activations active \
           ON active.owner_user_id = bundle.owner_user_id \
          AND active.bundle_id = bundle.bundle_id \
         WHERE bundle.owner_user_id = ? \
           AND coalesce(json_extract(bundle.spec_json, '$.enabled'), 1) = 1 \
           AND json_type(bundle.spec_json, '$.task_policy') = 'object' \
         ORDER BY bundle.bundle_id",
    )
    .bind(owner_user_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    specs
        .into_iter()
        .map(|spec| {
            let value: Value = serde_json::from_str(&spec)?;
            let policy = serde_json::from_value::<LocalPluginTaskPolicy>(
                value.get("task_policy").cloned().ok_or_else(|| {
                    ClientStorageError::InvalidState(
                        "active Plugin task policy is missing task_policy".into(),
                    )
                })?,
            )?;
            policy
                .validate()
                .map_err(ClientStorageError::InvalidState)?;
            Ok(policy)
        })
        .collect()
}

fn validate_planning_graph(
    graph: &CreateTaskGraphCommand,
    policies: &[LocalPluginTaskPolicy],
) -> Result<(), ClientStorageError> {
    if graph.tasks.len() != 1 {
        return Err(ClientStorageError::InvalidState(
            "a Plugin-authorized planning graph must contain exactly one Task".into(),
        ));
    }
    let task = &graph.tasks[0];
    let options = task
        .input
        .get("tool_options")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "a Plugin-authorized planning Task requires tool_options".into(),
            )
        })?;
    if policies
        .iter()
        .any(|policy| policy.planning_task.requires_execution)
        && options.get("requires_execution").and_then(Value::as_bool) != Some(true)
    {
        return Err(ClientStorageError::InvalidState(
            "the active Plugin planning template requires execution".into(),
        ));
    }
    let enabled = options
        .get("enabled_builtin_kinds")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "a Plugin-authorized planning Task requires enabled_builtin_kinds".into(),
            )
        })?;
    for required in policies
        .iter()
        .flat_map(|policy| &policy.planning_task.required_builtin_kinds)
    {
        if !enabled
            .iter()
            .any(|value| value.as_str() == Some(required.as_str()))
        {
            return Err(ClientStorageError::InvalidState(format!(
                "the active Plugin planning template requires {required}"
            )));
        }
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
