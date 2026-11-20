use super::*;

impl AgentConversationStore {
    pub(super) fn open_human_agent_direct(
        &self,
        owner_user_id: &str,
        agent_id: &str,
    ) -> Result<DirectRoom, String> {
        validate_agent_identifier(agent_id)?;
        reject_link_or_non_file(&self.database)?;
        let direct_key = format!("human:{owner_user_id}|agent:{agent_id}");
        validate_identifier(&direct_key)?;
        let mut connection = open_database(&self.database, false)?;
        if let Some(room) = load_direct_room(&connection, owner_user_id, &direct_key)? {
            return Ok(room);
        }
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| store_error())?;
        if let Some(room) = load_direct_room(&transaction, owner_user_id, &direct_key)? {
            transaction.commit().map_err(|_| store_error())?;
            return Ok(room);
        }
        let profile: Option<(String, String)> = transaction
            .query_row(
                "SELECT name, description FROM local_agent_profiles \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
                params![owner_user_id, agent_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| store_error())?;
        let Some((name, description)) = profile else {
            return Err("Agent 不存在或已停用。".into());
        };
        validate_text(&name, 120, false)?;
        validate_text(&description, 2_000, true)?;
        let room_id = uuid::Uuid::new_v4().to_string().to_lowercase();
        let now = chrono::Utc::now().timestamp_millis().max(0);
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_rooms (
                    owner_user_id, id, project_id, name, goal, default_agent_id, status,
                    created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key,
                    project_manager_agent_id
                ) VALUES (?1, ?2, ?3, ?4, '', ?5, 'active', ?6, ?6,
                          'human_agent_direct', ?7, NULL)
                "#,
                params![
                    owner_user_id,
                    room_id,
                    format!("direct:{room_id}"),
                    name,
                    agent_id,
                    now,
                    direct_key,
                ],
            )
            .map_err(|_| store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_room_members (
                    owner_user_id, room_id, agent_id, role, responsibility,
                    plugin_allowlist_json, status, joined_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, '[]', 'active', ?6)
                "#,
                params![owner_user_id, room_id, agent_id, name, description, now],
            )
            .map_err(|_| store_error())?;
        transaction.commit().map_err(|_| store_error())?;
        Ok(DirectRoom {
            id: room_id,
            agent_id: agent_id.to_owned(),
            name,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        })
    }

    pub(super) fn load_execution_binding(
        &self,
        owner_user_id: &str,
        room_id: &str,
    ) -> Result<AgentExecutionBinding, String> {
        validate_identifier(room_id)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = connection
            .query_row(
                r#"
                SELECT room.id, profile.id, profile.name, profile.role_prompt,
                       profile.model_config_id, profile.thinking_level,
                       profile.profession_key, profile.default_plugin_ids_json,
                       profile.default_skill_ids_json
                FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id AND profile.id = member.agent_id
                WHERE room.owner_user_id = ?1 AND room.id = ?2
                  AND room.conversation_kind = 'human_agent_direct'
                  AND room.status = 'active' AND member.status = 'active'
                  AND profile.status = 'active' AND room.default_agent_id = member.agent_id
                ORDER BY member.joined_at_unix_ms, member.agent_id
                LIMIT 2
                "#,
                params![owner_user_id, room_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| store_error())?;
        let Some((
            room_id,
            agent_id,
            name,
            role_prompt,
            model_config_id,
            thinking_level,
            profession_key,
            plugin_ids_json,
            skill_ids_json,
        )) = binding
        else {
            return Err("Agent 私聊不存在、已停用或身份绑定已经失效。".into());
        };
        let active_member_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'active'",
                params![owner_user_id, room_id],
                |row| row.get(0),
            )
            .map_err(|_| store_error())?;
        if active_member_count != 1 {
            return Err("Agent 私聊成员绑定无效，已拒绝执行。".into());
        }
        validate_agent_identifier(&agent_id)?;
        validate_text(&name, 120, false)?;
        validate_text(&role_prompt, 32_000, false)?;
        validate_identifier(&model_config_id)?;
        validate_identifier(&profession_key)?;
        if thinking_level.as_deref().is_some_and(|level| {
            !matches!(
                level,
                "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            )
        }) {
            return Err(store_error());
        }
        Ok(AgentExecutionBinding {
            room_id,
            agent_id,
            name,
            role_prompt,
            model_config_id,
            thinking_level,
            profession_key,
            default_plugin_ids: parse_identifier_array(&plugin_ids_json)?,
            default_skill_ids: parse_identifier_array(&skill_ids_json)?,
        })
    }

    pub(super) fn next_agent_communication_delivery(
        &self,
        owner_user_id: &str,
        room_id: &str,
    ) -> Result<Option<PendingAgentCommunicationDelivery>, String> {
        validate_identifier(room_id)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let pending = connection
            .query_row(
                r#"
                SELECT delivery.id, delivery.message_id, message.content,
                       delivery.trigger_kind, room.conversation_kind, room.name,
                       room.id, profile.id, profile.name, profile.role_prompt,
                       profile.model_config_id, profile.thinking_level,
                       profile.profession_key, profile.default_plugin_ids_json,
                       profile.default_skill_ids_json
                FROM project_agent_deliveries delivery
                JOIN project_agent_messages message
                  ON message.owner_user_id = delivery.owner_user_id
                 AND message.id = delivery.message_id
                 AND message.room_id = delivery.room_id
                JOIN project_agent_rooms room
                  ON room.owner_user_id = delivery.owner_user_id
                 AND room.id = delivery.room_id
                JOIN project_agent_room_members member
                  ON member.owner_user_id = delivery.owner_user_id
                 AND member.room_id = delivery.room_id
                 AND member.agent_id = delivery.target_agent_id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id
                 AND profile.id = member.agent_id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND delivery.status = 'pending' AND delivery.trigger_kind != 'todo'
                  AND room.status = 'active'
                  AND (room.conversation_kind = 'agent_agent_direct'
                       OR (room.conversation_kind = 'human_agent_direct'
                           AND delivery.trigger_kind IN ('heartbeat', 'mention')))
                  AND member.status = 'active' AND profile.status = 'active'
                  AND NOT EXISTS (
                    SELECT 1 FROM project_agent_deliveries active
                    WHERE active.owner_user_id = delivery.owner_user_id
                      AND active.target_agent_id = delivery.target_agent_id
                      AND active.status = 'running' AND active.id != delivery.id
                      AND active.trigger_kind != 'todo'
                  )
                ORDER BY delivery.created_at_unix_ms, delivery.id
                LIMIT 1
                "#,
                params![owner_user_id, room_id],
                |row| {
                    Ok(PendingAgentCommunicationDelivery {
                        delivery_id: row.get(0)?,
                        message_id: row.get(1)?,
                        content: row.get(2)?,
                        trigger_kind: row.get(3)?,
                        conversation_kind: row.get(4)?,
                        room_name: row.get(5)?,
                        binding: AgentExecutionBinding {
                            room_id: row.get(6)?,
                            agent_id: row.get(7)?,
                            name: row.get(8)?,
                            role_prompt: row.get(9)?,
                            model_config_id: row.get(10)?,
                            thinking_level: row.get(11)?,
                            profession_key: row.get(12)?,
                            default_plugin_ids: parse_identifier_array(&row.get::<_, String>(13)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                            default_skill_ids: parse_identifier_array(&row.get::<_, String>(14)?)
                                .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        },
                    })
                },
            )
            .optional()
            .map_err(|_| team_store_error())?;
        let Some(pending) = pending else {
            return Ok(None);
        };
        for value in [
            pending.delivery_id.as_str(),
            pending.message_id.as_str(),
            pending.binding.room_id.as_str(),
            pending.binding.agent_id.as_str(),
        ] {
            validate_identifier(value).map_err(|_| team_store_error())?;
        }
        if !matches!(
            pending.trigger_kind.as_str(),
            "mention" | "default_agent" | "agent_mention" | "heartbeat"
        ) {
            return Err(team_store_error());
        }
        validate_agent_execution_binding(&pending.binding)?;
        if !matches!(
            pending.conversation_kind.as_str(),
            "human_agent_direct" | "agent_agent_direct"
        ) || (pending.trigger_kind == "heartbeat"
            && pending.conversation_kind != "human_agent_direct")
        {
            return Err(team_store_error());
        }
        validate_text(&pending.room_name, 160, false).map_err(|_| team_store_error())?;
        validate_text(&pending.content, 1_000_000, false).map_err(|_| team_store_error())?;
        Ok(Some(pending))
    }

    pub(super) fn claim_agent_communication_delivery(
        &self,
        owner_user_id: &str,
        pending: &PendingAgentCommunicationDelivery,
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
                r#"
                UPDATE project_agent_deliveries
                SET status = 'running', attempt = attempt + 1,
                    claimed_at_unix_ms = ?1, last_error = NULL
                WHERE owner_user_id = ?2 AND id = ?3 AND room_id = ?4
                  AND target_agent_id = ?5 AND status = 'pending'
                  AND trigger_kind != 'todo'
                  AND NOT EXISTS (
                    SELECT 1 FROM project_agent_deliveries active
                    WHERE active.owner_user_id = ?2 AND active.target_agent_id = ?5
                      AND active.status = 'running' AND active.id != ?3
                      AND active.trigger_kind != 'todo'
                  )
                "#,
                params![
                    now_unix_ms,
                    owner_user_id,
                    pending.delivery_id,
                    pending.binding.room_id,
                    pending.binding.agent_id,
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
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())
    }

    pub(super) fn active_agent_communication_run_bindings(
        &self,
        owner_user_id: &str,
        room_id: &str,
    ) -> Result<Vec<ActiveAgentCommunicationRunBinding>, String> {
        validate_identifier(room_id)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT delivery.id, delivery.target_agent_id, run.turn_id, run.run_id
                FROM project_agent_deliveries delivery
                JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                JOIN project_agent_rooms room
                  ON room.owner_user_id = delivery.owner_user_id
                 AND room.id = delivery.room_id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND room.status = 'active'
                  AND (room.conversation_kind = 'agent_agent_direct'
                       OR (room.conversation_kind = 'human_agent_direct'
                           AND delivery.trigger_kind IN ('heartbeat', 'mention')))
                  AND (delivery.status = 'running' OR run.status IN ('ready', 'running'))
                ORDER BY run.updated_at_unix_ms DESC, delivery.id
                LIMIT 500
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, room_id], |row| {
                let agent_id = row.get::<_, String>(1)?;
                Ok(ActiveAgentCommunicationRunBinding {
                    delivery_id: row.get(0)?,
                    conversation_id: agent_communication_conversation_id(room_id, &agent_id),
                    turn_id: row.get(2)?,
                    run_id: row.get(3)?,
                })
            })
            .map_err(|_| team_store_error())?;
        let bindings = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for binding in &bindings {
            for value in [
                &binding.delivery_id,
                &binding.conversation_id,
                &binding.turn_id,
                &binding.run_id,
            ] {
                validate_identifier(value).map_err(|_| team_store_error())?;
            }
        }
        Ok(bindings)
    }
}
