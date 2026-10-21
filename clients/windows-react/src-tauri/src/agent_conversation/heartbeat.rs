use super::*;
use serde_json::Value;

struct DueHeartbeat {
    agent_id: String,
    name: String,
    description: String,
    interval_seconds: i64,
    prompt: String,
    scheduled_at_unix_ms: i64,
    room_id: Option<String>,
    has_outstanding: bool,
}

impl AgentConversationStore {
    pub(crate) fn enqueue_due_agent_heartbeats(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        if now_unix_ms < 0 || !(1..=128).contains(&limit) {
            return Err(heartbeat_error());
        }
        reject_link_or_non_file(&self.database)?;
        let due_exists: bool = open_database(&self.database, true)?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_profiles \
                 WHERE owner_user_id = ?1 AND status = 'active' \
                   AND heartbeat_enabled = 1 AND next_heartbeat_at_unix_ms IS NOT NULL \
                   AND next_heartbeat_at_unix_ms <= ?2)",
                params![owner_user_id, now_unix_ms],
                |row| row.get(0),
            )
            .map_err(|_| heartbeat_error())?;
        if !due_exists {
            return Ok(Vec::new());
        }
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| heartbeat_error())?;
        let due = {
            let mut statement = transaction
                .prepare(
                    r#"
                    SELECT profile.id, profile.name, profile.description,
                           profile.heartbeat_interval_seconds, profile.heartbeat_prompt,
                           profile.next_heartbeat_at_unix_ms,
                           (
                             SELECT room.id FROM project_agent_rooms room
                             WHERE room.owner_user_id = profile.owner_user_id
                               AND room.direct_key = 'human:' || profile.owner_user_id
                                   || '|agent:' || profile.id
                               AND room.conversation_kind = 'human_agent_direct'
                               AND room.status = 'active'
                             LIMIT 1
                           ),
                           EXISTS(
                             SELECT 1 FROM project_agent_deliveries delivery
                             WHERE delivery.owner_user_id = profile.owner_user_id
                               AND delivery.target_agent_id = profile.id
                               AND delivery.trigger_kind = 'heartbeat'
                               AND delivery.status IN ('pending', 'running')
                           )
                    FROM local_agent_profiles profile
                    WHERE profile.owner_user_id = ?1 AND profile.status = 'active'
                      AND profile.heartbeat_enabled = 1
                      AND profile.next_heartbeat_at_unix_ms IS NOT NULL
                      AND profile.next_heartbeat_at_unix_ms <= ?2
                    ORDER BY profile.next_heartbeat_at_unix_ms, profile.id
                    LIMIT ?3
                    "#,
                )
                .map_err(|_| heartbeat_error())?;
            let rows = statement
                .query_map(params![owner_user_id, now_unix_ms, limit as i64], |row| {
                    Ok(DueHeartbeat {
                        agent_id: row.get(0)?,
                        name: row.get(1)?,
                        description: row.get(2)?,
                        interval_seconds: row.get(3)?,
                        prompt: row.get(4)?,
                        scheduled_at_unix_ms: row.get(5)?,
                        room_id: row.get(6)?,
                        has_outstanding: row.get::<_, i64>(7)? == 1,
                    })
                })
                .map_err(|_| heartbeat_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| heartbeat_error())?
        };
        let mut scheduled_rooms = Vec::new();
        for heartbeat in due {
            validate_agent_identifier(&heartbeat.agent_id).map_err(|_| heartbeat_error())?;
            validate_text(&heartbeat.name, 120, false).map_err(|_| heartbeat_error())?;
            validate_text(&heartbeat.description, 2_000, true).map_err(|_| heartbeat_error())?;
            validate_text(&heartbeat.prompt, 8_000, true).map_err(|_| heartbeat_error())?;
            if !(60..=86_400).contains(&heartbeat.interval_seconds)
                || heartbeat.scheduled_at_unix_ms < 0
            {
                return Err(heartbeat_error());
            }
            if !heartbeat.has_outstanding {
                let room_id = if let Some(room_id) = heartbeat.room_id {
                    validate_identifier(&room_id).map_err(|_| heartbeat_error())?;
                    room_id
                } else {
                    let room_id = identifier("room");
                    let direct_key = format!("human:{owner_user_id}|agent:{}", heartbeat.agent_id);
                    transaction
                        .execute(
                            r#"
                            INSERT INTO project_agent_rooms (
                                owner_user_id, id, project_id, name, goal, default_agent_id,
                                status, created_at_unix_ms, updated_at_unix_ms,
                                conversation_kind, direct_key, project_manager_agent_id
                            ) VALUES (?1, ?2, ?3, ?4, '', ?5, 'active', ?6, ?6,
                                      'human_agent_direct', ?7, NULL)
                            "#,
                            params![
                                owner_user_id,
                                room_id,
                                format!("direct:{room_id}"),
                                heartbeat.name,
                                heartbeat.agent_id,
                                now_unix_ms,
                                direct_key,
                            ],
                        )
                        .map_err(|_| heartbeat_error())?;
                    transaction
                        .execute(
                            r#"
                            INSERT INTO project_agent_room_members (
                                owner_user_id, room_id, agent_id, role, responsibility,
                                plugin_allowlist_json, status, joined_at_unix_ms
                            ) VALUES (?1, ?2, ?3, ?4, ?5, '[]', 'active', ?6)
                            "#,
                            params![
                                owner_user_id,
                                room_id,
                                heartbeat.agent_id,
                                heartbeat.name,
                                heartbeat.description,
                                now_unix_ms,
                            ],
                        )
                        .map_err(|_| heartbeat_error())?;
                    room_id
                };
                let message_id = identifier("message");
                let delivery_id = identifier("delivery");
                let content = if heartbeat.prompt.is_empty() {
                    "主动巡检：读取全部未读消息，整理并继续处理自己的 TodoList。"
                } else {
                    &heartbeat.prompt
                };
                transaction
                    .execute(
                        r#"
                        INSERT INTO project_agent_messages (
                            owner_user_id, id, room_id, sender_kind, sender_id, content,
                            reply_to_message_id, source_run_id, causation_id,
                            root_message_id, hop_count, created_at_unix_ms
                        ) VALUES (?1, ?2, ?3, 'system', 'system', ?4, NULL, NULL,
                                  'heartbeat', ?2, 0, ?5)
                        "#,
                        params![owner_user_id, message_id, room_id, content, now_unix_ms],
                    )
                    .map_err(|_| heartbeat_error())?;
                transaction
                    .execute(
                        r#"
                        INSERT INTO project_agent_deliveries (
                            owner_user_id, id, room_id, message_id, root_message_id,
                            target_agent_id, trigger_kind, status, attempt, hop_count,
                            deduplication_key, response_message_id, last_error,
                            claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                        ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, 'heartbeat', 'pending', 0, 0,
                                  ?6, NULL, NULL, NULL, NULL, ?7)
                        "#,
                        params![
                            owner_user_id,
                            delivery_id,
                            room_id,
                            message_id,
                            heartbeat.agent_id,
                            format!(
                                "heartbeat:{}:{}",
                                heartbeat.agent_id, heartbeat.scheduled_at_unix_ms
                            ),
                            now_unix_ms,
                        ],
                    )
                    .map_err(|_| heartbeat_error())?;
                scheduled_rooms.push(room_id);
            }
            let changed = transaction
                .execute(
                    "UPDATE local_agent_profiles SET last_heartbeat_at_unix_ms = ?1, \
                     next_heartbeat_at_unix_ms = ?2 WHERE owner_user_id = ?3 AND id = ?4 \
                     AND status = 'active' AND heartbeat_enabled = 1 \
                     AND next_heartbeat_at_unix_ms = ?5",
                    params![
                        now_unix_ms,
                        now_unix_ms.saturating_add(heartbeat.interval_seconds * 1_000),
                        owner_user_id,
                        heartbeat.agent_id,
                        heartbeat.scheduled_at_unix_ms,
                    ],
                )
                .map_err(|_| heartbeat_error())?;
            if changed != 1 {
                return Err(heartbeat_error());
            }
        }
        transaction.commit().map_err(|_| heartbeat_error())?;
        scheduled_rooms.sort();
        scheduled_rooms.dedup();
        Ok(scheduled_rooms)
    }

    pub(crate) fn validate_agent_communication_cycle_completion(
        &self,
        owner_user_id: &str,
        run_id: &str,
        heartbeat_only: bool,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_communication_tool_binding(&connection, owner_user_id, run_id)?;
        let trigger_kind: String = connection
            .query_row(
                "SELECT delivery.trigger_kind \
                 FROM windows_agent_delivery_runs run \
                 JOIN project_agent_deliveries delivery \
                   ON delivery.owner_user_id = run.owner_user_id \
                  AND delivery.id = run.delivery_id \
                 WHERE run.owner_user_id = ?1 AND run.run_id = ?2 \
                   AND run.status IN ('ready', 'running') \
                   AND delivery.status = 'running' AND delivery.trigger_kind != 'todo' \
                   AND delivery.room_id = ?3 AND delivery.target_agent_id = ?4",
                params![owner_user_id, run_id, binding.room_id, binding.agent_id],
                |row| row.get(0),
            )
            .map_err(|_| heartbeat_error())?;
        if heartbeat_only && trigger_kind != "heartbeat" {
            return Err("chat_heartbeat_complete is only available for heartbeat runs.".into());
        }
        Ok((binding, json!({"status": "completed"})))
    }
}

fn heartbeat_error() -> String {
    "The Agent heartbeat state is unavailable.".into()
}
