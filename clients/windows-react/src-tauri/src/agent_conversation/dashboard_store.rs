use super::*;

impl AgentConversationStore {
    pub(crate) fn load_project_dashboard_snapshot(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<AgentProjectDashboardSnapshot, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        let dashboard = read_project_dashboard(&connection, owner_user_id, team_id)?;
        let pending_approval_count = connection
            .query_row(
                r#"
                SELECT
                    (SELECT count(*) FROM local_agent_creation_proposals
                     WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'pending') +
                    (SELECT count(*) FROM local_agent_removal_proposals
                     WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'pending') +
                    (SELECT count(*) FROM local_agent_membership_proposals
                     WHERE owner_user_id = ?1 AND source_room_id = ?2 AND status = 'pending')
                "#,
                params![owner_user_id, team_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| team_store_error())?;
        Ok(AgentProjectDashboardSnapshot {
            dashboard,
            pending_approval_count,
        })
    }

    pub(crate) fn save_project_dashboard_as_manager(
        &self,
        owner_user_id: &str,
        team_id: &str,
        editor_agent_id: &str,
        expected_revision: Option<i64>,
        update: AgentProjectDashboardUpdate,
        now_unix_ms: i64,
    ) -> Result<AgentProjectDashboard, String> {
        validate_identifier(team_id).map_err(|_| invalid_project_dashboard_input())?;
        validate_agent_identifier(editor_agent_id)
            .map_err(|_| invalid_project_dashboard_input())?;
        validate_project_dashboard_update(&update)?;
        if now_unix_ms < 0 {
            return Err(invalid_project_dashboard_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        require_project_team_manager(&transaction, owner_user_id, team_id, editor_agent_id)?;
        let existing = read_project_dashboard(&transaction, owner_user_id, team_id)?;
        let revision = match existing.as_ref() {
            Some(current) if expected_revision == Some(current.revision) => current.revision + 1,
            None if expected_revision.is_none() => 1,
            _ => return Err(project_dashboard_conflict()),
        };
        validate_project_dashboard_todo_links(&transaction, owner_user_id, team_id, &update)?;
        let encoded = serde_json::to_string(&update).map_err(|_| team_store_error())?;
        if let Some(current) = existing {
            let changed = transaction
                .execute(
                    r#"
                    UPDATE local_agent_project_dashboards
                    SET dashboard_json = ?1, revision = ?2, updated_by_agent_id = ?3,
                        updated_at_unix_ms = ?4
                    WHERE owner_user_id = ?5 AND team_room_id = ?6 AND revision = ?7
                    "#,
                    params![
                        encoded,
                        revision,
                        editor_agent_id,
                        now_unix_ms,
                        owner_user_id,
                        team_id,
                        current.revision,
                    ],
                )
                .map_err(|_| team_store_error())?;
            if changed != 1 {
                return Err(project_dashboard_conflict());
            }
        } else {
            transaction
                .execute(
                    r#"
                    INSERT INTO local_agent_project_dashboards (
                        owner_user_id, team_room_id, dashboard_json, revision,
                        updated_by_agent_id, updated_at_unix_ms
                    ) VALUES (?1, ?2, ?3, 1, ?4, ?5)
                    "#,
                    params![
                        owner_user_id,
                        team_id,
                        encoded,
                        editor_agent_id,
                        now_unix_ms,
                    ],
                )
                .map_err(|_| team_store_error())?;
        }
        transaction.commit().map_err(|_| team_store_error())?;
        Ok(AgentProjectDashboard {
            team_id: team_id.to_owned(),
            phase: update.phase,
            health: update.health,
            summary: update.summary,
            next_steps: update.next_steps,
            milestones: update.milestones,
            issues: update.issues,
            revision,
            updated_by_agent_id: editor_agent_id.to_owned(),
            updated_at_unix_ms: now_unix_ms,
        })
    }
}
