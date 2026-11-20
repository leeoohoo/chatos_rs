use super::*;

impl AgentConversationStore {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn save_project_team_asset_with_editor(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: Option<&str>,
        category: &str,
        title: &str,
        markdown: &str,
        expected_revision: Option<i64>,
        editor_agent_id: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        if let Some(asset_id) = asset_id {
            validate_identifier(asset_id).map_err(|_| invalid_team_asset_input())?;
        }
        validate_team_asset_category(category)?;
        validate_text(title, 240, false).map_err(|_| invalid_team_asset_input())?;
        validate_text(markdown, 128_000, true).map_err(|_| invalid_team_asset_input())?;
        if now_unix_ms < 0 || expected_revision.is_some_and(|revision| revision <= 0) {
            return Err(invalid_team_asset_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        if let Some(agent_id) = editor_agent_id {
            require_project_team_manager(&transaction, owner_user_id, team_id, agent_id)?;
        }
        let resolved_id = asset_id
            .map(str::to_owned)
            .unwrap_or_else(|| identifier("asset"));
        let existing = transaction
            .query_row(
                r#"
                SELECT id, team_room_id, category, title, markdown, revision, status,
                       created_by_agent_id, updated_by_agent_id,
                       created_at_unix_ms, updated_at_unix_ms
                FROM local_agent_team_assets
                WHERE owner_user_id = ?1 AND team_room_id = ?2 AND id = ?3
                LIMIT 1
                "#,
                params![owner_user_id, team_id, resolved_id],
                map_team_asset,
            )
            .optional()
            .map_err(|_| team_store_error())?;
        let saved = if let Some(existing) = existing {
            if existing.status != "active" || expected_revision != Some(existing.revision) {
                return Err(team_asset_conflict());
            }
            let revision = existing.revision + 1;
            let updated_at_unix_ms = now_unix_ms.max(existing.created_at_unix_ms);
            let changed = transaction
                .execute(
                    r#"
                    UPDATE local_agent_team_assets
                    SET category = ?1, title = ?2, markdown = ?3, revision = ?4,
                        updated_by_agent_id = ?5, updated_at_unix_ms = ?6
                    WHERE owner_user_id = ?7 AND team_room_id = ?8 AND id = ?9
                      AND status = 'active' AND revision = ?10
                    "#,
                    params![
                        category,
                        title,
                        markdown,
                        revision,
                        editor_agent_id,
                        updated_at_unix_ms,
                        owner_user_id,
                        team_id,
                        resolved_id,
                        existing.revision,
                    ],
                )
                .map_err(|_| team_store_error())?;
            if changed != 1 {
                return Err(team_asset_conflict());
            }
            AgentTeamAsset {
                id: resolved_id,
                team_id: team_id.to_owned(),
                category: category.to_owned(),
                title: title.to_owned(),
                markdown: markdown.to_owned(),
                revision,
                status: "active".into(),
                created_by_agent_id: existing.created_by_agent_id,
                updated_by_agent_id: editor_agent_id.map(str::to_owned),
                created_at_unix_ms: existing.created_at_unix_ms,
                updated_at_unix_ms,
            }
        } else {
            if asset_id.is_some() || expected_revision.is_some() {
                return Err("团队共享资产不存在。".into());
            }
            transaction
                .execute(
                    r#"
                    INSERT INTO local_agent_team_assets (
                        owner_user_id, id, team_room_id, category, title, markdown,
                        revision, status, created_by_agent_id, updated_by_agent_id,
                        created_at_unix_ms, updated_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 'active', ?7, ?7, ?8, ?8)
                    "#,
                    params![
                        owner_user_id,
                        resolved_id,
                        team_id,
                        category,
                        title,
                        markdown,
                        editor_agent_id,
                        now_unix_ms,
                    ],
                )
                .map_err(|_| team_store_error())?;
            AgentTeamAsset {
                id: resolved_id,
                team_id: team_id.to_owned(),
                category: category.to_owned(),
                title: title.to_owned(),
                markdown: markdown.to_owned(),
                revision: 1,
                status: "active".into(),
                created_by_agent_id: editor_agent_id.map(str::to_owned),
                updated_by_agent_id: editor_agent_id.map(str::to_owned),
                created_at_unix_ms: now_unix_ms,
                updated_at_unix_ms: now_unix_ms,
            }
        };
        validate_team_asset(&saved)?;
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_team_asset_revisions (
                    owner_user_id, asset_id, revision, title, markdown,
                    editor_agent_id, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    owner_user_id,
                    saved.id,
                    saved.revision,
                    saved.title,
                    saved.markdown,
                    editor_agent_id,
                    saved.updated_at_unix_ms,
                ],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())?;
        Ok(saved)
    }

    pub(super) fn archive_project_team_asset(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: &str,
        expected_revision: i64,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        self.archive_project_team_asset_with_editor(
            owner_user_id,
            team_id,
            asset_id,
            expected_revision,
            None,
            now_unix_ms,
        )
    }

    pub(crate) fn archive_project_team_asset_as_manager(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        asset_id: &str,
        expected_revision: i64,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        self.archive_project_team_asset_with_editor(
            owner_user_id,
            team_id,
            asset_id,
            expected_revision,
            Some(agent_id),
            now_unix_ms,
        )
    }

    pub(super) fn archive_project_team_asset_with_editor(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: &str,
        expected_revision: i64,
        editor_agent_id: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAsset, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_identifier(asset_id).map_err(|_| invalid_team_asset_input())?;
        if expected_revision <= 0 || now_unix_ms < 0 {
            return Err(invalid_team_asset_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        if let Some(agent_id) = editor_agent_id {
            require_project_team_manager(&transaction, owner_user_id, team_id, agent_id)?;
        }
        let changed = transaction
            .execute(
                r#"
                UPDATE local_agent_team_assets
                SET status = 'archived', revision = revision + 1,
                    updated_by_agent_id = ?1, updated_at_unix_ms = ?2
                WHERE owner_user_id = ?3 AND team_room_id = ?4 AND id = ?5
                  AND status = 'active' AND revision = ?6
                "#,
                params![
                    editor_agent_id,
                    now_unix_ms,
                    owner_user_id,
                    team_id,
                    asset_id,
                    expected_revision
                ],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err(team_asset_conflict());
        }
        let archived = transaction
            .query_row(
                r#"
                SELECT id, team_room_id, category, title, markdown, revision, status,
                       created_by_agent_id, updated_by_agent_id,
                       created_at_unix_ms, updated_at_unix_ms
                FROM local_agent_team_assets
                WHERE owner_user_id = ?1 AND team_room_id = ?2 AND id = ?3
                "#,
                params![owner_user_id, team_id, asset_id],
                map_team_asset,
            )
            .map_err(|_| team_store_error())?;
        validate_team_asset(&archived)?;
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_team_asset_revisions (
                    owner_user_id, asset_id, revision, title, markdown,
                    editor_agent_id, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    owner_user_id,
                    asset_id,
                    archived.revision,
                    archived.title,
                    archived.markdown,
                    editor_agent_id,
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_store_error())?;
        transaction.commit().map_err(|_| team_store_error())?;
        Ok(archived)
    }

    pub(super) fn list_project_team_asset_revisions(
        &self,
        owner_user_id: &str,
        team_id: &str,
        asset_id: &str,
    ) -> Result<Vec<AgentTeamAssetRevision>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_identifier(asset_id).map_err(|_| invalid_team_asset_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        let belongs = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_team_assets WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3)",
                params![owner_user_id, team_id, asset_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| team_store_error())?;
        if !belongs {
            return Err("团队共享资产不存在。".into());
        }
        let mut statement = connection
            .prepare(
                r#"
                SELECT asset_id, revision, title, markdown, editor_agent_id, created_at_unix_ms
                FROM local_agent_team_asset_revisions
                WHERE owner_user_id = ?1 AND asset_id = ?2
                ORDER BY revision DESC LIMIT 500
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, asset_id], |row| {
                Ok(AgentTeamAssetRevision {
                    asset_id: row.get(0)?,
                    revision: row.get(1)?,
                    title: row.get(2)?,
                    markdown: row.get(3)?,
                    editor_agent_id: row.get(4)?,
                    created_at_unix_ms: row.get(5)?,
                })
            })
            .map_err(|_| team_store_error())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())
    }
}
