use super::*;

impl AgentConversationStore {
    pub(super) fn list_project_team_assets(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<Vec<AgentTeamAsset>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT id, team_room_id, category, title, markdown, revision, status,
                       created_by_agent_id, updated_by_agent_id,
                       created_at_unix_ms, updated_at_unix_ms
                FROM local_agent_team_assets
                WHERE owner_user_id = ?1 AND team_room_id = ?2 AND status = 'active'
                ORDER BY category, updated_at_unix_ms DESC, id
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, team_id], map_team_asset)
            .map_err(|_| team_store_error())?;
        let assets = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for asset in &assets {
            validate_team_asset(asset)?;
        }
        Ok(assets)
    }

    pub(crate) fn load_agent_asset_tool_records(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<(AgentAssetToolBinding, Vec<AgentAssetToolRecord>), String> {
        validate_identifier(run_id).map_err(|_| team_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_asset_tool_binding(&connection, owner_user_id, run_id)?;
        let records = if let Some(todo_id) = binding.todo_id.as_deref() {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT team_room_id, asset_id, category, title, markdown, revision
                    FROM local_agent_todo_asset_snapshots
                    WHERE owner_user_id = ?1 AND todo_id = ?2 AND team_room_id = ?3
                    ORDER BY category, title, asset_id
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, todo_id, binding.team_id], |row| {
                    Ok(AgentAssetToolRecord {
                        team_id: row.get(0)?,
                        asset_id: row.get(1)?,
                        category: row.get(2)?,
                        title: row.get(3)?,
                        markdown: row.get(4)?,
                        revision: row.get(5)?,
                    })
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        } else {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT asset.team_room_id, asset.id, asset.category, asset.title,
                           asset.markdown, asset.revision
                    FROM local_agent_team_assets asset
                    JOIN project_agent_rooms room
                      ON room.owner_user_id = asset.owner_user_id
                     AND room.id = asset.team_room_id
                    JOIN project_agent_room_members member
                      ON member.owner_user_id = room.owner_user_id
                     AND member.room_id = room.id AND member.agent_id = ?2
                    JOIN local_agent_profiles profile
                      ON profile.owner_user_id = member.owner_user_id
                     AND profile.id = member.agent_id
                    WHERE asset.owner_user_id = ?1 AND asset.status = 'active'
                      AND room.status = 'active' AND room.conversation_kind = 'project_team'
                      AND member.status = 'active' AND profile.status = 'active'
                    ORDER BY asset.team_room_id, asset.category, asset.title, asset.id
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, binding.agent_id], |row| {
                    Ok(AgentAssetToolRecord {
                        team_id: row.get(0)?,
                        asset_id: row.get(1)?,
                        category: row.get(2)?,
                        title: row.get(3)?,
                        markdown: row.get(4)?,
                        revision: row.get(5)?,
                    })
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        Ok((binding, records))
    }

    pub(crate) fn resolve_agent_asset_team_reference(
        &self,
        owner_user_id: &str,
        run_id: &str,
        reference: &str,
    ) -> Result<String, String> {
        validate_identifier(run_id).map_err(|_| team_store_error())?;
        validate_text(reference, 600, false).map_err(|_| team_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_asset_tool_binding(&connection, owner_user_id, run_id)?;
        if binding.todo_id.is_some() {
            let expected =
                team_todo::todo_planning_reference(owner_user_id, run_id, "team", &binding.team_id);
            return (reference == expected)
                .then_some(binding.team_id)
                .ok_or_else(|| "The team reference is unavailable for this Run.".to_owned());
        }
        team_todo::resolve_team_reference(
            &connection,
            owner_user_id,
            run_id,
            &binding.agent_id,
            reference,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn save_project_team_asset(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: Option<&str>,
        category: &str,
        title: &str,
        markdown: &str,
        expected_revision: Option<i64>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        self.save_project_team_asset_with_editor(
            owner_user_id,
            team_id,
            asset_id,
            category,
            title,
            markdown,
            expected_revision,
            None,
            now_unix_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn save_project_team_asset_as_manager(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        asset_id: Option<&str>,
        category: &str,
        title: &str,
        markdown: &str,
        expected_revision: Option<i64>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        self.save_project_team_asset_with_editor(
            owner_user_id,
            team_id,
            asset_id,
            category,
            title,
            markdown,
            expected_revision,
            Some(agent_id),
            now_unix_ms,
        )
    }
}
