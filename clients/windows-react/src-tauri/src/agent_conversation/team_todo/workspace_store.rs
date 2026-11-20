use super::*;

impl AgentConversationStore {
    pub(crate) fn agent_workspace_snapshot(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_communication_tool_binding(&connection, owner_user_id, run_id)?;
        let profiles = {
            let mut statement = connection
                .prepare(
                    "SELECT id, name, profession_key FROM local_agent_profiles \
                     WHERE owner_user_id = ?1 AND status = 'active' ORDER BY name, id",
                )
                .map_err(|_| team_todo_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|_| team_todo_store_error())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_todo_store_error())?;
            rows
        };
        let team_rows = {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT room.id, room.project_id, room.name, room.goal,
                           room.project_manager_agent_id, member.agent_id,
                           profile.name, profile.profession_key, member.role
                    FROM project_agent_rooms room
                    LEFT JOIN project_agent_room_members member
                      ON member.owner_user_id = room.owner_user_id
                     AND member.room_id = room.id AND member.status = 'active'
                    LEFT JOIN local_agent_profiles profile
                      ON profile.owner_user_id = member.owner_user_id
                     AND profile.id = member.agent_id AND profile.status = 'active'
                    WHERE room.owner_user_id = ?1 AND room.status = 'active'
                      AND room.conversation_kind = 'project_team'
                    ORDER BY room.created_at_unix_ms, room.id,
                             member.joined_at_unix_ms, member.agent_id
                    "#,
                )
                .map_err(|_| team_todo_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                })
                .map_err(|_| team_todo_store_error())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_todo_store_error())?;
            rows
        };
        let mut teams: Vec<WorkspaceTeamSnapshot> = Vec::new();
        let mut team_names_by_agent: Vec<(String, Vec<String>)> = Vec::new();
        for (
            team_id,
            project_id,
            team_name,
            team_goal,
            project_manager_id,
            agent_id,
            agent_name,
            profession,
            role,
        ) in team_rows
        {
            let member = match (agent_id, agent_name, profession, role) {
                (Some(agent_id), Some(agent_name), Some(profession), Some(role)) => {
                    if let Some((_, names)) = team_names_by_agent
                        .iter_mut()
                        .find(|entry| entry.0 == agent_id)
                    {
                        names.push(team_name.clone());
                    } else {
                        team_names_by_agent.push((agent_id.clone(), vec![team_name.clone()]));
                    }
                    Some(json!({
                    "agent_ref": todo_planning_reference(
                        owner_user_id,
                        run_id,
                        "agent",
                            &agent_id,
                    ),
                        "name": agent_name,
                        "profession": profession,
                        "role": role,
                        "is_project_manager": project_manager_id.as_deref() == Some(&agent_id),
                    }))
                }
                _ => None,
            };
            if let Some(team) = teams.iter_mut().find(|team| team.id == team_id) {
                if let Some(member) = member {
                    team.members.push(member);
                }
            } else {
                teams.push(WorkspaceTeamSnapshot {
                    id: team_id,
                    project_id,
                    name: team_name,
                    goal: team_goal,
                    project_manager_id,
                    members: member.into_iter().collect(),
                });
            }
        }
        let team_values = teams
            .into_iter()
            .map(|team| {
                let project_manager = team.project_manager_id.as_ref().and_then(|manager_id| {
                    profiles
                        .iter()
                        .find(|profile| profile.0 == *manager_id)
                        .map(|profile| profile.1.clone())
                });
                json!({
                    "team_ref": todo_planning_reference(
                        owner_user_id,
                        run_id,
                        "team",
                    &team.id,
                    ),
                    "project_ref": todo_planning_reference(
                        owner_user_id,
                        run_id,
                        "project",
                    &team.project_id,
                    ),
                "name": team.name,
                "goal": team.goal,
                "has_project_manager": team.project_manager_id.is_some(),
                    "project_manager": project_manager,
                "members": team.members,
                })
            })
            .collect::<Vec<_>>();
        let agents = profiles
            .into_iter()
            .map(|(agent_id, name, profession)| {
                let mut team_names = team_names_by_agent
                    .iter()
                    .find(|entry| entry.0 == agent_id)
                    .map(|entry| entry.1.clone())
                    .unwrap_or_default();
                team_names.sort();
                json!({
                    "agent_ref": todo_planning_reference(
                        owner_user_id,
                        run_id,
                        "agent",
                        &agent_id,
                    ),
                    "name": name,
                    "profession": profession,
                    "is_current_agent": agent_id == binding.agent_id,
                    "teams": team_names,
                })
            })
            .collect::<Vec<_>>();
        Ok((binding, json!({"agents": agents, "teams": team_values})))
    }
}
