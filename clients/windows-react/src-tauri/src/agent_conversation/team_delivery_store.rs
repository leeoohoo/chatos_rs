use super::*;

impl AgentConversationStore {
    pub(super) fn next_project_team_delivery(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<Option<PendingTeamDelivery>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let pending = connection
            .query_row(
                r#"
                SELECT delivery.id, delivery.message_id, message.content,
                       delivery.trigger_kind, todo.id, todo.execution_plan_json,
                       room.id, profile.id, profile.name, profile.role_prompt,
                       profile.model_config_id, profile.thinking_level,
                       profile.profession_key, profile.default_plugin_ids_json,
                       profile.default_skill_ids_json, room.project_id, room.name, room.goal,
                       member.role, member.responsibility,
                       CASE WHEN room.project_manager_agent_id = profile.id THEN 1 ELSE 0 END
                FROM project_agent_deliveries delivery
                JOIN project_agent_messages message
                  ON message.owner_user_id = delivery.owner_user_id
                 AND message.id = delivery.message_id
                JOIN project_agent_rooms room
                  ON room.owner_user_id = delivery.owner_user_id
                 AND room.id = delivery.room_id
                LEFT JOIN local_agent_todos todo
                  ON todo.owner_user_id = delivery.owner_user_id
                 AND todo.agent_id = delivery.target_agent_id
                 AND delivery.trigger_kind = 'todo'
                 AND (delivery.deduplication_key = 'todo:' || todo.id
                      OR delivery.deduplication_key LIKE 'todo:' || todo.id || ':attempt:%')
                JOIN project_agent_room_members member
                  ON member.owner_user_id = delivery.owner_user_id
                 AND member.room_id = delivery.room_id
                 AND member.agent_id = delivery.target_agent_id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id
                 AND profile.id = member.agent_id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND delivery.status = 'pending' AND room.status = 'active'
                  AND room.conversation_kind = 'project_team'
                  AND member.status = 'active' AND profile.status = 'active'
                  AND (delivery.trigger_kind != 'todo' OR todo.status = 'in_progress')
                  AND NOT EXISTS (
                    SELECT 1 FROM project_agent_deliveries active
                    WHERE active.owner_user_id = delivery.owner_user_id
                      AND active.target_agent_id = delivery.target_agent_id
                      AND active.status = 'running' AND active.id != delivery.id
                      AND ((delivery.trigger_kind = 'todo' AND active.trigger_kind = 'todo')
                           OR (delivery.trigger_kind != 'todo'
                               AND active.trigger_kind != 'todo'))
                  )
                ORDER BY delivery.created_at_unix_ms, delivery.id
                LIMIT 1
                "#,
                params![owner_user_id, team_id],
                |row| {
                    Ok(PendingTeamDelivery {
                        delivery_id: row.get(0)?,
                        message_id: row.get(1)?,
                        content: row.get(2)?,
                        trigger_kind: row.get(3)?,
                        todo_id: row.get(4)?,
                        execution_plan: row
                            .get::<_, Option<String>>(5)?
                            .map(|value| serde_json::from_str(&value))
                            .transpose()
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        binding: TeamExecutionBinding {
                            agent: AgentExecutionBinding {
                                room_id: row.get(6)?,
                                agent_id: row.get(7)?,
                                name: row.get(8)?,
                                role_prompt: row.get(9)?,
                                model_config_id: row.get(10)?,
                                thinking_level: row.get(11)?,
                                profession_key: row.get(12)?,
                                default_plugin_ids: parse_identifier_array(
                                    &row.get::<_, String>(13)?,
                                )
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                                default_skill_ids: parse_identifier_array(
                                    &row.get::<_, String>(14)?,
                                )
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                            },
                            project_id: row.get(15)?,
                            team_name: row.get(16)?,
                            team_goal: row.get(17)?,
                            team_role: row.get(18)?,
                            team_responsibility: row.get(19)?,
                            is_project_manager: row.get::<_, i64>(20)? == 1,
                        },
                    })
                },
            )
            .optional()
            .map_err(|_| team_store_error())?;
        let Some(pending) = pending else {
            return Ok(None);
        };
        validate_identifier(&pending.delivery_id).map_err(|_| team_store_error())?;
        validate_identifier(&pending.message_id).map_err(|_| team_store_error())?;
        if !matches!(
            pending.trigger_kind.as_str(),
            "mention" | "default_agent" | "agent_mention" | "heartbeat" | "todo" | "todo_status"
        ) {
            return Err(team_store_error());
        }
        match (&pending.trigger_kind[..], pending.todo_id.as_deref()) {
            ("todo", Some(todo_id)) => {
                validate_identifier(todo_id).map_err(|_| team_store_error())?;
                pending
                    .execution_plan
                    .as_ref()
                    .ok_or_else(team_store_error)?;
            }
            ("todo", None) | (_, Some(_)) => return Err(team_store_error()),
            _ => {}
        }
        if pending.trigger_kind != "todo" && pending.execution_plan.is_some() {
            return Err(team_store_error());
        }
        validate_team_execution_binding(&pending.binding)?;
        validate_text(&pending.content, 1_000_000, true).map_err(|_| team_store_error())?;
        if pending.content.is_empty() {
            let has_attachment = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM project_agent_message_attachments \
                     WHERE owner_user_id = ?1 AND message_id = ?2)",
                    params![owner_user_id, pending.message_id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|_| team_store_error())?;
            if !has_attachment {
                return Err(team_store_error());
            }
        }
        Ok(Some(pending))
    }

    pub(super) fn load_project_team_execution_binding(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<TeamExecutionBinding, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = connection
            .query_row(
                r#"
                SELECT room.id, profile.id, profile.name, profile.role_prompt,
                       profile.model_config_id, profile.thinking_level,
                       profile.profession_key, profile.default_plugin_ids_json,
                       profile.default_skill_ids_json, room.project_id, room.name, room.goal,
                       member.role, member.responsibility,
                       CASE WHEN room.project_manager_agent_id = profile.id THEN 1 ELSE 0 END
                FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id AND profile.id = member.agent_id
                WHERE room.owner_user_id = ?1 AND room.id = ?2 AND profile.id = ?3
                  AND room.status = 'active' AND room.conversation_kind = 'project_team'
                  AND member.status = 'active' AND profile.status = 'active'
                "#,
                params![owner_user_id, team_id, agent_id],
                |row| {
                    Ok(TeamExecutionBinding {
                        agent: AgentExecutionBinding {
                            room_id: row.get(0)?,
                            agent_id: row.get(1)?,
                            name: row.get(2)?,
                            role_prompt: row.get(3)?,
                            model_config_id: row.get(4)?,
                            thinking_level: row.get(5)?,
                            profession_key: row.get(6)?,
                            default_plugin_ids: parse_identifier_array(&row.get::<_, String>(7)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                            default_skill_ids: parse_identifier_array(&row.get::<_, String>(8)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        },
                        project_id: row.get(9)?,
                        team_name: row.get(10)?,
                        team_goal: row.get(11)?,
                        team_role: row.get(12)?,
                        team_responsibility: row.get(13)?,
                        is_project_manager: row.get::<_, i64>(14)? == 1,
                    })
                },
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 不是当前团队的可执行活动成员。".to_owned())?;
        validate_team_execution_binding(&binding)?;
        Ok(binding)
    }

    pub(crate) fn validate_team_executor_binding(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        project_id: &str,
    ) -> Result<(), String> {
        let binding = self.load_project_team_execution_binding(owner_user_id, team_id, agent_id)?;
        if binding.project_id != project_id {
            return Err("The team executor binding is no longer active for this Project.".into());
        }
        Ok(())
    }

    pub(super) fn claim_project_team_delivery(
        &self,
        owner_user_id: &str,
        pending: &PendingTeamDelivery,
        turn_id: &str,
        run_id: &str,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        validate_identifier(turn_id)?;
        validate_identifier(run_id)?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let changed = transaction
            .execute(
                "UPDATE project_agent_deliveries SET status = 'running', attempt = attempt + 1, \
                 claimed_at_unix_ms = ?1, last_error = NULL WHERE owner_user_id = ?2 AND id = ?3 \
                 AND room_id = ?4 AND target_agent_id = ?5 AND status = 'pending' \
                 AND NOT EXISTS (SELECT 1 FROM project_agent_deliveries active \
                   WHERE active.owner_user_id = ?2 AND active.target_agent_id = ?5 \
                     AND active.status = 'running' AND active.id != ?3 \
                     AND ((project_agent_deliveries.trigger_kind = 'todo' \
                           AND active.trigger_kind = 'todo') \
                          OR (project_agent_deliveries.trigger_kind != 'todo' \
                              AND active.trigger_kind != 'todo'))) ",
                params![
                    now_unix_ms,
                    owner_user_id,
                    pending.delivery_id,
                    pending.binding.agent.room_id,
                    pending.binding.agent.agent_id,
                ],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err("Agent Delivery 已被其他执行器领取，请刷新后重试。".into());
        }
        transaction
            .execute(
                "INSERT INTO windows_agent_delivery_runs (owner_user_id, delivery_id, turn_id, \
                 run_id, status, created_at_unix_ms, updated_at_unix_ms) \
                 VALUES (?1, ?2, ?3, ?4, 'ready', ?5, ?5)",
                params![
                    owner_user_id,
                    pending.delivery_id,
                    turn_id,
                    run_id,
                    now_unix_ms
                ],
            )
            .map_err(|_| team_store_error())?;
        if pending.trigger_kind == "todo" {
            let todo_id = pending.todo_id.as_deref().ok_or_else(team_store_error)?;
            team_todo::insert_todo_progress(
                &transaction,
                owner_user_id,
                todo_id,
                run_id,
                "started",
                "started",
                "Todo 执行线程已启动，并完成团队、项目和能力计划绑定。",
                &[],
                now_unix_ms,
            )?;
        }
        transaction.commit().map_err(|_| team_store_error())
    }

    pub(super) fn fail_project_team_delivery(
        &self,
        owner_user_id: &str,
        delivery_id: &str,
        failure: &str,
    ) -> Result<(), String> {
        validate_identifier(delivery_id)?;
        let message: String = failure.chars().take(2_000).collect();
        let now = chrono::Utc::now().timestamp_millis().max(0);
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        transaction
            .execute(
                "UPDATE project_agent_deliveries SET status = 'failed', last_error = ?1, \
                 completed_at_unix_ms = ?2 WHERE owner_user_id = ?3 AND id = ?4 \
                   AND status = 'running'",
                params![message, now, owner_user_id, delivery_id],
            )
            .map_err(|_| team_store_error())?;
        transaction
            .execute(
                "UPDATE windows_agent_delivery_runs SET status = 'failed', updated_at_unix_ms = ?1 \
                 WHERE owner_user_id = ?2 AND delivery_id = ?3 AND status IN ('ready', 'running')",
                params![now, owner_user_id, delivery_id],
            )
            .map_err(|_| team_store_error())?;
        transaction
            .execute(
                r#"
                UPDATE local_agent_todos
                SET status = 'blocked', blocked_reason = ?1,
                    updated_at_unix_ms = CASE
                      WHEN updated_at_unix_ms >= ?2 THEN updated_at_unix_ms + 1
                      ELSE ?2
                    END
                WHERE owner_user_id = ?3 AND status = 'in_progress'
                  AND EXISTS (
                    SELECT 1 FROM project_agent_deliveries delivery
                    WHERE delivery.owner_user_id = local_agent_todos.owner_user_id
                      AND delivery.id = ?4 AND delivery.trigger_kind = 'todo'
                      AND delivery.target_agent_id = local_agent_todos.agent_id
                      AND (delivery.deduplication_key = 'todo:' || local_agent_todos.id
                           OR delivery.deduplication_key LIKE
                              'todo:' || local_agent_todos.id || ':attempt:%')
                  )
                "#,
                params![message, now, owner_user_id, delivery_id],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())
    }
}
