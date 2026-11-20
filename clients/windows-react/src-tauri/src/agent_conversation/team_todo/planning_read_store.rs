use super::*;

impl AgentConversationStore {
    pub(super) fn list_project_team_todo_progress(
        &self,
        owner_user_id: &str,
        team_id: &str,
        todo_id: &str,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
        validate_identifier(todo_id).map_err(|_| invalid_team_todo_input())?;
        if !(1..=500).contains(&limit) {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        let belongs_to_team: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_todos \
                 WHERE owner_user_id = ?1 AND team_room_id = ?2 AND id = ?3)",
                params![owner_user_id, team_id, todo_id],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())?;
        if !belongs_to_team {
            return Err("团队任务不存在。".into());
        }
        load_todo_progress(&connection, owner_user_id, todo_id, limit)?
            .into_iter()
            .map(todo_progress_for_ui)
            .collect()
    }

    pub(crate) fn todo_planning_tool_snapshot(
        &self,
        owner_user_id: &str,
        run_id: &str,
        include_terminal: bool,
    ) -> Result<(AgentAssetToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_asset_tool_binding(&connection, owner_user_id, run_id)?;
        if binding.todo_id.is_some() {
            return Err("Todo planning tools are unavailable in an executor Run.".into());
        }
        let (team_name, team_goal): (String, String) = connection
            .query_row(
                "SELECT name, goal FROM project_agent_rooms WHERE owner_user_id = ?1 AND id = ?2 \
                 AND status = 'active' AND conversation_kind = 'project_team'",
                params![owner_user_id, binding.team_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| team_todo_store_error())?;
        let assignees = load_todo_assignee_options(
            &connection,
            owner_user_id,
            run_id,
            &binding.team_id,
            &binding.agent_id,
        )?;
        let todos =
            self.list_project_team_todos(owner_user_id, &binding.team_id, include_terminal)?;
        let todo_values = todos
            .iter()
            .map(|todo| todo_planning_response(owner_user_id, run_id, &binding, todo, &todos))
            .collect::<Vec<_>>();
        let output = json!({
            "team": {
                "team_ref": todo_planning_reference(
                    owner_user_id,
                    run_id,
                    "team",
                    &binding.team_id,
                ),
                "name": team_name,
                "goal": team_goal,
                "assignees": assignees,
            },
            "builtin_capabilities": TODO_BUILTIN_CAPABILITIES,
            "plugins": [],
            "todos": todo_values,
        });
        Ok((binding, output))
    }

    pub(crate) fn resolve_dashboard_todo_references(
        &self,
        owner_user_id: &str,
        run_id: &str,
        references: &[String],
    ) -> Result<(AgentAssetToolBinding, Vec<String>), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        validate_reference_list(references, 192)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_asset_tool_binding(&connection, owner_user_id, run_id)?;
        if binding.todo_id.is_some() {
            return Err("Project dashboard tools are unavailable in an executor Run.".into());
        }
        let resolved = resolve_todo_references(
            &connection,
            owner_user_id,
            run_id,
            &binding.team_id,
            references,
            false,
        )?;
        Ok((binding, resolved))
    }

    pub(crate) fn todo_planning_progress(
        &self,
        owner_user_id: &str,
        run_id: &str,
        todo_reference: &str,
        limit: usize,
    ) -> Result<(AgentAssetToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        validate_text(todo_reference, 600, false).map_err(|_| invalid_team_todo_input())?;
        if !(1..=500).contains(&limit) {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_asset_tool_binding(&connection, owner_user_id, run_id)?;
        if binding.todo_id.is_some() {
            return Err("Todo planning tools are unavailable in an executor Run.".into());
        }
        let todos = self.list_project_team_todos(owner_user_id, &binding.team_id, true)?;
        let todo = todos
            .iter()
            .find(|todo| {
                todo_planning_reference(owner_user_id, run_id, "todo", &todo.id) == todo_reference
            })
            .ok_or_else(|| "The Todo reference is unavailable for this Run.".to_owned())?;
        let progress = load_todo_progress(&connection, owner_user_id, &todo.id, limit)?;
        Ok((
            binding,
            json!({
                "todo_ref": todo_reference,
                "title": todo.title,
                "progress": progress,
            }),
        ))
    }
}
