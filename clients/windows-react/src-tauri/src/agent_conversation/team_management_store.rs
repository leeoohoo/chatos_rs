use super::*;

impl AgentConversationStore {
    pub(super) fn list_project_teams(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<AgentTeamSummary>, String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT room.id, room.project_id, room.name, room.goal,
                       room.default_agent_id, room.project_manager_agent_id,
                       (SELECT count(*) FROM project_agent_room_members member
                        WHERE member.owner_user_id = room.owner_user_id
                          AND member.room_id = room.id AND member.status = 'active'),
                       room.created_at_unix_ms, room.updated_at_unix_ms
                FROM project_agent_rooms room
                WHERE room.owner_user_id = ?1 AND room.status = 'active'
                  AND room.conversation_kind = 'project_team'
                ORDER BY room.updated_at_unix_ms DESC, room.id DESC
                LIMIT 1000
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id], |row| {
                Ok(AgentTeamSummary {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
                    goal: row.get(3)?,
                    default_agent_id: row.get(4)?,
                    project_manager_agent_id: row.get(5)?,
                    member_count: row.get(6)?,
                    created_at_unix_ms: row.get(7)?,
                    updated_at_unix_ms: row.get(8)?,
                })
            })
            .map_err(|_| team_store_error())?;
        let teams = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for team in &teams {
            validate_team_summary(team)?;
        }
        Ok(teams)
    }

    pub(super) fn create_project_team(
        &self,
        owner_user_id: &str,
        project_id: &str,
        name: &str,
        goal: &str,
    ) -> Result<AgentTeamSummary, String> {
        validate_identifier(project_id).map_err(|_| invalid_team_input())?;
        validate_text(name, 160, false).map_err(|_| invalid_team_input())?;
        validate_text(goal, 8_000, true).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_agent_rooms \
                 WHERE owner_user_id = ?1 AND project_id = ?2 \
                   AND status = 'active' AND conversation_kind = 'project_team')",
                params![owner_user_id, project_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        if exists {
            return Err("这个项目已经有 Agent 团队，请直接打开现有团队。".into());
        }
        let now = chrono::Utc::now().timestamp_millis().max(0);
        let id = uuid::Uuid::new_v4().to_string().to_lowercase();
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_rooms (
                    owner_user_id, id, project_id, name, goal, default_agent_id, status,
                    created_at_unix_ms, updated_at_unix_ms, conversation_kind, direct_key,
                    project_manager_agent_id
                ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'active', ?6, ?6,
                          'project_team', NULL, NULL)
                "#,
                params![owner_user_id, id, project_id, name, goal, now],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())?;
        let team = AgentTeamSummary {
            id,
            project_id: project_id.to_owned(),
            name: name.to_owned(),
            goal: goal.to_owned(),
            default_agent_id: None,
            project_manager_agent_id: None,
            member_count: 0,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        };
        validate_team_summary(&team)?;
        Ok(team)
    }

    pub(super) fn load_project_team(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let team = connection
            .query_row(
                r#"
                SELECT room.id, room.project_id, room.name, room.goal,
                       room.default_agent_id, room.project_manager_agent_id,
                       (SELECT count(*) FROM project_agent_room_members member
                        WHERE member.owner_user_id = room.owner_user_id
                          AND member.room_id = room.id AND member.status = 'active'),
                       room.created_at_unix_ms, room.updated_at_unix_ms
                FROM project_agent_rooms room
                WHERE room.owner_user_id = ?1 AND room.id = ?2
                  AND room.status = 'active' AND room.conversation_kind = 'project_team'
                "#,
                params![owner_user_id, team_id],
                |row| {
                    Ok(AgentTeamSummary {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        name: row.get(2)?,
                        goal: row.get(3)?,
                        default_agent_id: row.get(4)?,
                        project_manager_agent_id: row.get(5)?,
                        member_count: row.get(6)?,
                        created_at_unix_ms: row.get(7)?,
                        updated_at_unix_ms: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 团队不存在或已停用。".to_owned())?;
        validate_team_summary(&team)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT member.agent_id, member.role, member.responsibility,
                       member.joined_at_unix_ms
                FROM project_agent_room_members member
                WHERE member.owner_user_id = ?1 AND member.room_id = ?2
                  AND member.status = 'active'
                ORDER BY member.joined_at_unix_ms, member.agent_id
                LIMIT 1000
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, team_id], |row| {
                Ok(AgentTeamMemberSummary {
                    agent_id: row.get(0)?,
                    role: row.get(1)?,
                    responsibility: row.get(2)?,
                    joined_at_unix_ms: row.get(3)?,
                })
            })
            .map_err(|_| team_store_error())?;
        let members = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for member in &members {
            validate_team_member(member)?;
        }
        if members.len() as i64 != team.member_count {
            return Err(team_store_error());
        }
        Ok(AgentTeamDetail { team, members })
    }

    pub(super) fn invite_agent_to_project_team(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let team_exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_agent_rooms \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active' \
                   AND conversation_kind = 'project_team')",
                params![owner_user_id, team_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        if !team_exists {
            return Err("Agent 团队不存在或已停用。".into());
        }
        let profile = transaction
            .query_row(
                "SELECT name, description FROM local_agent_profiles \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
                params![owner_user_id, agent_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 不存在或已停用。".to_owned())?;
        validate_text(&profile.0, 160, false).map_err(|_| team_store_error())?;
        validate_text(&profile.1, 8_000, true).map_err(|_| team_store_error())?;
        let existing_status = transaction
            .query_row(
                "SELECT status FROM project_agent_room_members \
                 WHERE owner_user_id = ?1 AND room_id = ?2 AND agent_id = ?3",
                params![owner_user_id, team_id, agent_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| team_store_error())?;
        if existing_status.as_deref() == Some("active") {
            return Err("这个 Agent 已经在当前团队中。".into());
        }
        let member_count: i64 = transaction
            .query_row(
                "SELECT count(*) FROM project_agent_room_members \
                 WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'active'",
                params![owner_user_id, team_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        let now = chrono::Utc::now().timestamp_millis().max(0);
        if existing_status.as_deref() == Some("removed") {
            transaction
                .execute(
                    "UPDATE project_agent_room_members \
                     SET role = ?1, responsibility = ?2, plugin_allowlist_json = '[]', \
                         status = 'active', joined_at_unix_ms = ?3 \
                     WHERE owner_user_id = ?4 AND room_id = ?5 AND agent_id = ?6 \
                       AND status = 'removed'",
                    params![profile.0, profile.1, now, owner_user_id, team_id, agent_id],
                )
                .map_err(|_| team_store_error())?;
        } else if existing_status.is_none() {
            transaction
                .execute(
                    r#"
                    INSERT INTO project_agent_room_members (
                        owner_user_id, room_id, agent_id, role, responsibility,
                        plugin_allowlist_json, status, joined_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, '[]', 'active', ?6)
                    "#,
                    params![owner_user_id, team_id, agent_id, profile.0, profile.1, now],
                )
                .map_err(|_| team_store_error())?;
        } else {
            return Err(team_store_error());
        }
        transaction
            .execute(
                "UPDATE project_agent_rooms \
                 SET default_agent_id = CASE WHEN ?1 = 0 THEN ?2 ELSE default_agent_id END, \
                     updated_at_unix_ms = ?3 \
                 WHERE owner_user_id = ?4 AND id = ?5 AND status = 'active'",
                params![member_count, agent_id, now, owner_user_id, team_id],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())?;
        self.load_project_team(owner_user_id, team_id)
    }

    pub(super) fn update_project_team_member(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        role: &str,
        responsibility: &str,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        validate_text(role, 160, false).map_err(|_| invalid_team_input())?;
        validate_text(responsibility, 8_000, true).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, false)?;
        let changed = connection
            .execute(
                r#"
                UPDATE project_agent_room_members
                SET role = ?1, responsibility = ?2
                WHERE owner_user_id = ?3 AND room_id = ?4 AND agent_id = ?5
                  AND status = 'active'
                  AND EXISTS (
                    SELECT 1 FROM project_agent_rooms room
                    WHERE room.owner_user_id = ?3 AND room.id = ?4
                      AND room.status = 'active' AND room.conversation_kind = 'project_team'
                  )
                "#,
                params![role, responsibility, owner_user_id, team_id, agent_id],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err("Agent 不是当前团队的活动成员。".into());
        }
        self.load_project_team(owner_user_id, team_id)
    }

    pub(super) fn set_project_team_default_agent(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        self.assign_project_team_agent(owner_user_id, team_id, agent_id, false)
    }

    pub(super) fn set_project_team_manager(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        self.assign_project_team_agent(owner_user_id, team_id, agent_id, true)
    }

    pub(super) fn assign_project_team_agent(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        project_manager: bool,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let member_exists: bool = transaction
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM project_agent_room_members member
                    JOIN project_agent_rooms room
                      ON room.owner_user_id = member.owner_user_id AND room.id = member.room_id
                    WHERE member.owner_user_id = ?1 AND member.room_id = ?2
                      AND member.agent_id = ?3 AND member.status = 'active'
                      AND room.status = 'active' AND room.conversation_kind = 'project_team'
                )
                "#,
                params![owner_user_id, team_id, agent_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        if !member_exists {
            return Err("Agent 不是当前团队的活动成员。".into());
        }
        if project_manager {
            let profession = transaction
                .query_row(
                    "SELECT profession_key FROM local_agent_profiles \
                     WHERE owner_user_id = ?1 AND id = ?2",
                    params![owner_user_id, agent_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|_| team_store_error())?;
            if profession.as_deref() != Some("project_manager") {
                return Err("只有职业为“项目经理”的 Agent 才能担任项目经理。".into());
            }
        }
        let now = chrono::Utc::now().timestamp_millis().max(0);
        let column = if project_manager {
            "project_manager_agent_id"
        } else {
            "default_agent_id"
        };
        let changed = transaction
            .execute(
                &format!(
                    "UPDATE project_agent_rooms SET {column} = ?1, updated_at_unix_ms = \
                     max(updated_at_unix_ms, ?2) WHERE owner_user_id = ?3 AND id = ?4 \
                     AND status = 'active' AND conversation_kind = 'project_team'"
                ),
                params![agent_id, now, owner_user_id, team_id],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err("Agent 团队不存在或已停用。".into());
        }
        transaction.commit().map_err(|_| team_store_error())?;
        self.load_project_team(owner_user_id, team_id)
    }

    pub(super) fn remove_agent_from_project_team(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
    ) -> Result<AgentTeamDetail, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let room = transaction
            .query_row(
                "SELECT default_agent_id, project_manager_agent_id, updated_at_unix_ms \
                 FROM project_agent_rooms WHERE owner_user_id = ?1 AND id = ?2 \
                 AND status = 'active' AND conversation_kind = 'project_team'",
                params![owner_user_id, team_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 团队不存在或已停用。".to_owned())?;
        if room.1.as_deref() == Some(agent_id) {
            return Err("请先指定新的项目经理，再移出当前项目经理。".into());
        }
        let now = chrono::Utc::now().timestamp_millis().max(room.2);
        transaction
            .execute(
                "UPDATE project_agent_deliveries \
                 SET status = CASE WHEN status = 'pending' THEN 'cancelled' ELSE 'failed' END, \
                     last_error = '成员已由 Human 移出当前团队。', completed_at_unix_ms = ?1 \
                 WHERE owner_user_id = ?2 AND room_id = ?3 AND target_agent_id = ?4 \
                   AND status IN ('pending', 'running')",
                params![now, owner_user_id, team_id, agent_id],
            )
            .map_err(|_| team_store_error())?;
        let changed = transaction
            .execute(
                "UPDATE project_agent_room_members SET status = 'removed' \
                 WHERE owner_user_id = ?1 AND room_id = ?2 AND agent_id = ?3 \
                   AND status = 'active'",
                params![owner_user_id, team_id, agent_id],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err("Agent 不是当前团队的活动成员。".into());
        }
        if room.0.as_deref() == Some(agent_id) {
            let replacement = transaction
                .query_row(
                    "SELECT agent_id FROM project_agent_room_members \
                     WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'active' \
                     ORDER BY joined_at_unix_ms, agent_id LIMIT 1",
                    params![owner_user_id, team_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|_| team_store_error())?;
            transaction
                .execute(
                    "UPDATE project_agent_rooms SET default_agent_id = ?1, \
                     updated_at_unix_ms = ?2 WHERE owner_user_id = ?3 AND id = ?4 \
                     AND status = 'active'",
                    params![replacement, now, owner_user_id, team_id],
                )
                .map_err(|_| team_store_error())?;
        }
        transaction.commit().map_err(|_| team_store_error())?;
        self.load_project_team(owner_user_id, team_id)
    }
}
