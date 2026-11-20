use super::*;

pub(super) fn require_running_todo_tool_binding(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    binding: &AgentAssetToolBinding,
) -> Result<String, String> {
    let todo_id = binding
        .todo_id
        .as_deref()
        .ok_or_else(team_todo_store_error)?;
    let valid: bool = connection
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM local_agent_todos todo
                JOIN project_agent_deliveries delivery
                  ON delivery.owner_user_id = todo.owner_user_id
                 AND delivery.room_id = todo.team_room_id
                 AND delivery.target_agent_id = todo.agent_id
                 AND delivery.trigger_kind = 'todo'
                 AND (delivery.deduplication_key = 'todo:' || todo.id
                      OR delivery.deduplication_key LIKE 'todo:' || todo.id || ':attempt:%')
                JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE todo.owner_user_id = ?1 AND todo.id = ?2
                  AND todo.team_room_id = ?3 AND todo.agent_id = ?4
                  AND todo.status = 'in_progress' AND delivery.status = 'running'
                  AND run.run_id = ?5 AND run.status IN ('ready', 'running')
            )
            "#,
            params![
                owner_user_id,
                todo_id,
                binding.team_id,
                binding.agent_id,
                run_id,
            ],
            |row| row.get(0),
        )
        .map_err(|_| team_todo_store_error())?;
    if valid {
        Ok(todo_id.to_owned())
    } else {
        Err(team_todo_conflict())
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::agent_conversation) fn insert_todo_progress(
    connection: &Connection,
    owner_user_id: &str,
    todo_id: &str,
    run_id: &str,
    kind: &str,
    stage: &str,
    detail: &str,
    asset_update_suggestions: &[AgentTeamAssetUpdateSuggestion],
    now_unix_ms: i64,
) -> Result<Value, String> {
    validate_asset_update_suggestions(asset_update_suggestions)?;
    if !asset_update_suggestions.is_empty() && kind != "completed" {
        return Err(invalid_team_asset_review_input());
    }
    let sequence: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM local_agent_todo_events \
             WHERE owner_user_id = ?1 AND todo_id = ?2",
            params![owner_user_id, todo_id],
            |row| row.get(0),
        )
        .map_err(|_| team_todo_store_error())?;
    let event_id = identifier("todo_event");
    let suggestions_json =
        serde_json::to_string(asset_update_suggestions).map_err(|_| team_todo_store_error())?;
    connection
        .execute(
            r#"
            INSERT INTO local_agent_todo_events (
                owner_user_id, id, todo_id, sequence, run_id, kind, stage, detail,
                asset_update_suggestions_json, created_at_unix_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                owner_user_id,
                event_id,
                todo_id,
                sequence,
                run_id,
                kind,
                stage,
                detail,
                suggestions_json,
                now_unix_ms,
            ],
        )
        .map_err(|_| team_todo_store_error())?;
    if !asset_update_suggestions.is_empty() {
        let (team_id, source_agent_id): (String, String) = connection
            .query_row(
                "SELECT team_room_id, agent_id FROM local_agent_todos \
                 WHERE owner_user_id = ?1 AND id = ?2",
                params![owner_user_id, todo_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| team_todo_store_error())?;
        for (index, suggestion) in asset_update_suggestions.iter().enumerate() {
            connection
                .execute(
                    r#"
                    INSERT INTO local_agent_team_asset_reviews (
                        owner_user_id, id, team_room_id, todo_id, todo_event_id,
                        suggestion_index, source_agent_id, category, title, markdown,
                        rationale, status, resolved_asset_id, resolved_asset_revision,
                        reviewed_by_agent_id, review_reason,
                        created_at_unix_ms, updated_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                              ?11, 'pending', NULL, NULL, NULL, '', ?12, ?12)
                    "#,
                    params![
                        owner_user_id,
                        identifier("asset_review"),
                        team_id,
                        todo_id,
                        event_id,
                        index as i64,
                        source_agent_id,
                        suggestion.category,
                        suggestion.title,
                        suggestion.markdown,
                        suggestion.rationale,
                        now_unix_ms,
                    ],
                )
                .map_err(|_| team_todo_store_error())?;
        }
    }
    Ok(json!({
        "sequence": sequence,
        "run_id": run_id,
        "kind": kind,
        "stage": stage,
        "detail": detail,
        "created_at_unix_ms": now_unix_ms,
    }))
}

pub(super) fn validate_asset_update_suggestions(
    suggestions: &[AgentTeamAssetUpdateSuggestion],
) -> Result<(), String> {
    if suggestions.len() > 8 {
        return Err(invalid_team_asset_review_input());
    }
    let mut identities = HashSet::new();
    let mut total_markdown_bytes = 0usize;
    for suggestion in suggestions {
        validate_team_asset_suggestion(suggestion)?;
        if !identities.insert((suggestion.category.as_str(), suggestion.title.as_str())) {
            return Err(invalid_team_asset_review_input());
        }
        total_markdown_bytes = total_markdown_bytes.saturating_add(suggestion.markdown.len());
        if total_markdown_bytes > 256_000 {
            return Err(invalid_team_asset_review_input());
        }
    }
    Ok(())
}

pub(super) fn load_todo_progress(
    connection: &Connection,
    owner_user_id: &str,
    todo_id: &str,
    limit: usize,
) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT sequence, run_id, kind, stage, detail, created_at_unix_ms
            FROM local_agent_todo_events
            WHERE owner_user_id = ?1 AND todo_id = ?2
            ORDER BY sequence DESC
            LIMIT ?3
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(
            params![owner_user_id, todo_id, limit.min(500) as i64],
            |row| {
                Ok(json!({
                    "sequence": row.get::<_, i64>(0)?,
                    "run_id": row.get::<_, Option<String>>(1)?,
                    "kind": row.get::<_, String>(2)?,
                    "stage": row.get::<_, String>(3)?,
                    "detail": row.get::<_, String>(4)?,
                    "created_at_unix_ms": row.get::<_, i64>(5)?,
                }))
            },
        )
        .map_err(|_| team_todo_store_error())?;
    let mut values = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| team_todo_store_error())?;
    values.reverse();
    Ok(values)
}

pub(super) fn todo_progress_for_ui(mut progress: Value) -> Result<Value, String> {
    let object = progress.as_object_mut().ok_or_else(team_todo_store_error)?;
    let run_id = object.remove("run_id").ok_or_else(team_todo_store_error)?;
    let created_at_unix_ms = object
        .remove("created_at_unix_ms")
        .ok_or_else(team_todo_store_error)?;
    object.insert("runId".to_owned(), run_id);
    object.insert("createdAtUnixMs".to_owned(), created_at_unix_ms);
    Ok(progress)
}

pub(super) fn load_todo_prerequisites(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
    todo_id: &str,
) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT prerequisite.title, profile.name, prerequisite.status,
                   prerequisite.blocked_reason, prerequisite.result
            FROM local_agent_todo_dependencies dependency
            JOIN local_agent_todos prerequisite
              ON prerequisite.owner_user_id = dependency.owner_user_id
             AND prerequisite.id = dependency.prerequisite_todo_id
             AND prerequisite.team_room_id = ?2
            JOIN local_agent_profiles profile
              ON profile.owner_user_id = prerequisite.owner_user_id
             AND profile.id = prerequisite.agent_id
            WHERE dependency.owner_user_id = ?1 AND dependency.todo_id = ?3
            ORDER BY dependency.created_at_unix_ms, dependency.prerequisite_todo_id
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, team_id, todo_id], |row| {
            Ok(json!({
                "title": row.get::<_, String>(0)?,
                "assignee": row.get::<_, String>(1)?,
                "status": row.get::<_, String>(2)?,
                "blocked_reason": row.get::<_, String>(3)?,
                "result": row.get::<_, String>(4)?,
            }))
        })
        .map_err(|_| team_todo_store_error())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| team_todo_store_error())
}

pub(super) fn load_todo_asset_summaries(
    connection: &Connection,
    owner_user_id: &str,
    todo_id: &str,
) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT category, title, revision, captured_at_unix_ms
            FROM local_agent_todo_asset_snapshots
            WHERE owner_user_id = ?1 AND todo_id = ?2
            ORDER BY category, title, asset_id
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, todo_id], |row| {
            Ok(json!({
                "category": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "revision": row.get::<_, i64>(2)?,
                "updated_at_unix_ms": row.get::<_, i64>(3)?,
            }))
        })
        .map_err(|_| team_todo_store_error())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| team_todo_store_error())
}

pub(super) fn todo_has_terminal_receipt(
    connection: &Connection,
    owner_user_id: &str,
    delivery_id: &str,
    run_id: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM local_agent_todo_events event
                JOIN local_agent_todos todo
                  ON todo.owner_user_id = event.owner_user_id AND todo.id = event.todo_id
                JOIN project_agent_deliveries delivery
                  ON delivery.owner_user_id = todo.owner_user_id
                 AND delivery.target_agent_id = todo.agent_id
                 AND delivery.trigger_kind = 'todo'
                 AND (delivery.deduplication_key = 'todo:' || todo.id
                      OR delivery.deduplication_key LIKE 'todo:' || todo.id || ':attempt:%')
                WHERE event.owner_user_id = ?1 AND delivery.id = ?2 AND event.run_id = ?3
                  AND event.kind IN ('completed', 'blocked')
            )
            "#,
            params![owner_user_id, delivery_id, run_id],
            |row| row.get(0),
        )
        .map_err(|_| team_todo_store_error())
}

pub(super) fn executor_todo_message(
    title: &str,
    detail: &str,
    contract: &AgentTodoExecutionContract,
) -> String {
    fn list(values: &[String]) -> String {
        values
            .iter()
            .map(|value| format!("- {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
    format!(
        "你正在执行项目经理分配的独立 Todo。只处理以下工作契约，不要把团队聊天当作任务范围。\n\n\
         # {title}\n\n{detail}\n\n## 目标\n{}\n\n## 范围\n{}\n\n\
         ## 预期产出\n{}\n\n## 验收标准\n{}\n\n## 约束\n{}",
        contract.objective,
        contract.scope,
        list(&contract.expected_outputs),
        list(&contract.acceptance_criteria),
        if contract.constraints.is_empty() {
            "- 无额外约束".into()
        } else {
            list(&contract.constraints)
        },
    )
}

pub(super) fn load_dependencies(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT dependency.todo_id, dependency.prerequisite_todo_id
            FROM local_agent_todo_dependencies dependency
            JOIN local_agent_todos todo
              ON todo.owner_user_id = dependency.owner_user_id
             AND todo.id = dependency.todo_id
            JOIN local_agent_todos prerequisite
              ON prerequisite.owner_user_id = dependency.owner_user_id
             AND prerequisite.id = dependency.prerequisite_todo_id
            WHERE dependency.owner_user_id = ?1
              AND todo.team_room_id = ?2
              AND prerequisite.team_room_id = ?2
            ORDER BY dependency.todo_id, dependency.created_at_unix_ms,
                     dependency.prerequisite_todo_id
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, team_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|_| team_todo_store_error())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| team_todo_store_error())
}

pub(super) fn project_team_todo(
    stored: StoredTeamTodo,
    dependency_ids: Vec<String>,
) -> Result<AgentTeamTodo, String> {
    let execution_contract: AgentTodoExecutionContract =
        serde_json::from_str(&stored.execution_contract_json)
            .map_err(|_| team_todo_store_error())?;
    let execution_plan: AgentTodoExecutionPlan =
        serde_json::from_str(&stored.execution_plan_json).map_err(|_| team_todo_store_error())?;
    let todo = AgentTeamTodo {
        id: stored.id,
        agent_id: stored.agent_id,
        agent_name: stored.agent_name,
        team_id: stored.team_id,
        source_room_id: stored.source_room_id,
        source_message_id: stored.source_message_id,
        title: stored.title,
        detail: stored.detail,
        priority: stored.priority,
        sort_order: stored.sort_order,
        status: stored.status,
        blocked_reason: stored.blocked_reason,
        result: stored.result,
        execution_contract,
        execution_plan,
        dependency_ids,
        created_at_unix_ms: stored.created_at_unix_ms,
        updated_at_unix_ms: stored.updated_at_unix_ms,
    };
    validate_team_todo(&todo)?;
    Ok(todo)
}
