use super::*;

pub(super) fn team_chat_routing_stop_reason(
    connection: &Connection,
    owner_user_id: &str,
    root_message_id: &str,
    hop_count: i64,
    target_count: usize,
) -> Result<Option<String>, String> {
    if target_count == 0 {
        return Ok(None);
    }
    if hop_count > MAXIMUM_TEAM_CHAT_HOP_COUNT {
        return Ok(Some("本轮 Agent 协作已达到最大唤醒深度。".into()));
    }
    let existing_run_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM project_agent_deliveries \
             WHERE owner_user_id = ?1 AND root_message_id = ?2",
            params![owner_user_id, root_message_id],
            |row| row.get(0),
        )
        .map_err(|_| team_todo_store_error())?;
    if existing_run_count.saturating_add(target_count as i64) > MAXIMUM_TEAM_CHAT_RUNS_PER_ROOT {
        return Ok(Some("本轮 Agent 协作已达到最大运行次数。".into()));
    }
    Ok(None)
}

pub(super) fn persist_team_chat_message_reference(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    room_id: &str,
    message_id: &str,
    now_unix_ms: i64,
) -> Result<String, String> {
    let reference = todo_planning_reference(owner_user_id, run_id, "message", message_id);
    connection
        .execute(
            r#"
            INSERT OR IGNORE INTO windows_agent_chat_message_references (
                owner_user_id, run_id, message_ref, room_id, message_id,
                created_at_unix_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                owner_user_id,
                run_id,
                reference,
                room_id,
                message_id,
                now_unix_ms,
            ],
        )
        .map_err(|_| team_todo_store_error())?;
    let recorded: (String, String) = connection
        .query_row(
            "SELECT room_id, message_id FROM windows_agent_chat_message_references \
             WHERE owner_user_id = ?1 AND run_id = ?2 AND message_ref = ?3",
            params![owner_user_id, run_id, reference],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| team_todo_store_error())?;
    if recorded != (room_id.to_owned(), message_id.to_owned()) {
        return Err("The message reference could not be issued safely.".into());
    }
    Ok(reference)
}

pub(super) fn resolve_team_message_reference(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    current_agent_id: &str,
    reference: &str,
) -> Result<(String, String, String, i64), String> {
    connection
        .query_row(
            r#"
            SELECT message.room_id, message.id, message.root_message_id,
                   message.hop_count
            FROM windows_agent_chat_message_references reference
            JOIN project_agent_messages message
              ON message.owner_user_id = reference.owner_user_id
             AND message.room_id = reference.room_id
             AND message.id = reference.message_id
            JOIN project_agent_rooms room
              ON room.owner_user_id = message.owner_user_id
             AND room.id = message.room_id
            JOIN project_agent_room_members member
              ON member.owner_user_id = message.owner_user_id
             AND member.room_id = message.room_id AND member.agent_id = ?4
            JOIN local_agent_profiles profile
              ON profile.owner_user_id = member.owner_user_id
             AND profile.id = member.agent_id
            WHERE reference.owner_user_id = ?1 AND reference.run_id = ?2
              AND reference.message_ref = ?3 AND room.status = 'active'
              AND room.conversation_kind IN ('project_team', 'agent_agent_direct')
              AND member.status = 'active' AND profile.status = 'active'
            "#,
            params![owner_user_id, run_id, reference, current_agent_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|_| team_todo_store_error())?
        .ok_or_else(|| "The message reference is unavailable for this Run.".to_owned())
}

pub(in crate::agent_conversation) fn resolve_team_reference(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    current_agent_id: &str,
    reference: &str,
) -> Result<String, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT room.id
            FROM project_agent_rooms room
            JOIN project_agent_room_members member
              ON member.owner_user_id = room.owner_user_id
             AND member.room_id = room.id AND member.agent_id = ?2
            JOIN local_agent_profiles profile
              ON profile.owner_user_id = member.owner_user_id
             AND profile.id = member.agent_id
            WHERE room.owner_user_id = ?1 AND room.status = 'active'
              AND room.conversation_kind = 'project_team'
              AND member.status = 'active' AND profile.status = 'active'
            ORDER BY room.id
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, current_agent_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| team_todo_store_error())?;
    for row in rows {
        let team_id = row.map_err(|_| team_todo_store_error())?;
        if todo_planning_reference(owner_user_id, run_id, "team", &team_id) == reference {
            return Ok(team_id);
        }
    }
    Err("The team reference is unavailable for this Run.".into())
}

pub(in crate::agent_conversation) fn resolve_agent_reference(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    reference: &str,
) -> Result<String, String> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM local_agent_profiles WHERE owner_user_id = ?1 \
             AND status = 'active' ORDER BY id",
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id], |row| row.get::<_, String>(0))
        .map_err(|_| team_todo_store_error())?;
    for row in rows {
        let agent_id = row.map_err(|_| team_todo_store_error())?;
        if todo_planning_reference(owner_user_id, run_id, "agent", &agent_id) == reference {
            return Ok(agent_id);
        }
    }
    Err("The target Agent reference is unavailable for this Run.".into())
}

pub(super) fn active_room_member_ids(
    connection: &Connection,
    owner_user_id: &str,
    room_id: &str,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT member.agent_id
            FROM project_agent_room_members member
            JOIN local_agent_profiles profile
              ON profile.owner_user_id = member.owner_user_id
             AND profile.id = member.agent_id
            WHERE member.owner_user_id = ?1 AND member.room_id = ?2
              AND member.status = 'active' AND profile.status = 'active'
            ORDER BY member.joined_at_unix_ms, member.agent_id
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let members = statement
        .query_map(params![owner_user_id, room_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| team_todo_store_error())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| team_todo_store_error())?;
    Ok(members)
}

pub(super) fn open_agent_direct(
    connection: &Connection,
    owner_user_id: &str,
    initiating_agent_id: &str,
    target_agent_id: &str,
    now_unix_ms: i64,
) -> Result<String, String> {
    if initiating_agent_id == target_agent_id || now_unix_ms < 0 {
        return Err(invalid_team_todo_input());
    }
    let mut pair = [initiating_agent_id, target_agent_id];
    pair.sort_unstable();
    let direct_key = format!("agent:{}|agent:{}", pair[0], pair[1]);
    validate_identifier(&direct_key).map_err(|_| invalid_team_todo_input())?;
    let existing = connection
        .query_row(
            "SELECT id FROM project_agent_rooms WHERE owner_user_id = ?1 AND direct_key = ?2 \
             AND status = 'active' AND conversation_kind = 'agent_agent_direct'",
            params![owner_user_id, direct_key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| team_todo_store_error())?;
    if let Some(room_id) = existing {
        let members = active_room_member_ids(connection, owner_user_id, &room_id)?;
        if members.len() == 2
            && members
                .iter()
                .any(|agent_id| agent_id == initiating_agent_id)
            && members.iter().any(|agent_id| agent_id == target_agent_id)
        {
            return Ok(room_id);
        }
        return Err("The Agent direct conversation membership is invalid.".into());
    }
    let profiles = {
        let mut statement = connection
            .prepare(
                "SELECT id, name, description FROM local_agent_profiles \
                 WHERE owner_user_id = ?1 AND id IN (?2, ?3) AND status = 'active' ORDER BY id",
            )
            .map_err(|_| team_todo_store_error())?;
        let profiles = statement
            .query_map(
                params![owner_user_id, initiating_agent_id, target_agent_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(|_| team_todo_store_error())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_todo_store_error())?;
        profiles
    };
    if profiles.len() != 2 {
        return Err("The target Agent is unavailable.".into());
    }
    let room_id = identifier("agent-direct");
    let name = format!("{} · {}", profiles[0].1, profiles[1].1);
    validate_text(&name, 160, false).map_err(|_| team_todo_store_error())?;
    connection
        .execute(
            r#"
            INSERT INTO project_agent_rooms (
                owner_user_id, id, project_id, name, goal, default_agent_id, status,
                created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key,
                project_manager_agent_id
            ) VALUES (?1, ?2, ?3, ?4, '', NULL, 'active', ?5, ?5,
                      'agent_agent_direct', ?6, NULL)
            "#,
            params![
                owner_user_id,
                room_id,
                format!("direct:{room_id}"),
                name,
                now_unix_ms,
                direct_key,
            ],
        )
        .map_err(|_| team_todo_store_error())?;
    for (agent_id, agent_name, description) in profiles {
        connection
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
                    agent_id,
                    agent_name,
                    description,
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
    }
    Ok(room_id)
}
