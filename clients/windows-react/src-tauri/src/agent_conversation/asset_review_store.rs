use super::*;

impl AgentConversationStore {
    pub(crate) fn list_team_asset_reviews(
        &self,
        owner_user_id: &str,
        team_id: Option<&str>,
        reviewer_agent_id: Option<&str>,
        status: &str,
        limit: usize,
    ) -> Result<Vec<AgentTeamAssetReview>, String> {
        if let Some(team_id) = team_id {
            validate_identifier(team_id).map_err(|_| invalid_team_asset_review_input())?;
        }
        if let Some(agent_id) = reviewer_agent_id {
            validate_identifier(agent_id).map_err(|_| invalid_team_asset_review_input())?;
        }
        if !matches!(status, "pending" | "applied" | "rejected") || !(1..=500).contains(&limit) {
            return Err(invalid_team_asset_review_input());
        }
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT review.id, review.team_room_id, review.todo_id, todo.title,
                       review.source_agent_id, profile.name, review.category, review.title,
                       review.markdown, review.rationale, review.status,
                       review.resolved_asset_id, review.resolved_asset_revision,
                       review.reviewed_by_agent_id, review.review_reason,
                       review.created_at_unix_ms, review.updated_at_unix_ms
                FROM local_agent_team_asset_reviews review
                JOIN local_agent_todos todo
                  ON todo.owner_user_id = review.owner_user_id AND todo.id = review.todo_id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = review.owner_user_id
                 AND profile.id = review.source_agent_id
                JOIN project_agent_rooms room
                  ON room.owner_user_id = review.owner_user_id
                 AND room.id = review.team_room_id
                WHERE review.owner_user_id = ?1 AND review.status = ?2
                  AND (?3 IS NULL OR review.team_room_id = ?3)
                  AND (?4 IS NULL OR room.project_manager_agent_id = ?4)
                  AND room.status = 'active' AND room.conversation_kind = 'project_team'
                ORDER BY review.created_at_unix_ms, review.id
                LIMIT ?5
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(
                params![
                    owner_user_id,
                    status,
                    team_id,
                    reviewer_agent_id,
                    limit as i64
                ],
                map_team_asset_review,
            )
            .map_err(|_| team_store_error())?;
        let reviews = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        for review in &reviews {
            validate_team_asset_review(review)?;
        }
        Ok(reviews)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_team_asset_review(
        &self,
        owner_user_id: &str,
        review_id: &str,
        reviewer_agent_id: &str,
        outcome: &str,
        resolved_asset: Option<(&str, i64)>,
        reason: &str,
        now_unix_ms: i64,
    ) -> Result<AgentTeamAssetReview, String> {
        validate_identifier(review_id).map_err(|_| invalid_team_asset_review_input())?;
        validate_identifier(reviewer_agent_id).map_err(|_| invalid_team_asset_review_input())?;
        validate_text(reason, 4_000, true).map_err(|_| invalid_team_asset_review_input())?;
        if now_unix_ms < 0
            || !matches!(outcome, "applied" | "rejected")
            || (outcome == "applied" && resolved_asset.is_none())
            || (outcome == "rejected" && (resolved_asset.is_some() || reason.trim().is_empty()))
        {
            return Err(invalid_team_asset_review_input());
        }
        if let Some((asset_id, revision)) = resolved_asset {
            validate_identifier(asset_id).map_err(|_| invalid_team_asset_review_input())?;
            if revision <= 0 {
                return Err(invalid_team_asset_review_input());
            }
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let team_id: String = transaction
            .query_row(
                r#"
                SELECT review.team_room_id
                FROM local_agent_team_asset_reviews review
                JOIN project_agent_rooms room
                  ON room.owner_user_id = review.owner_user_id
                 AND room.id = review.team_room_id
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id
                 AND member.room_id = room.id AND member.agent_id = ?3
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id
                 AND profile.id = member.agent_id
                WHERE review.owner_user_id = ?1 AND review.id = ?2
                  AND review.status = 'pending' AND room.status = 'active'
                  AND room.conversation_kind = 'project_team'
                  AND room.project_manager_agent_id = ?3
                  AND member.status = 'active' AND profile.status = 'active'
                "#,
                params![owner_user_id, review_id, reviewer_agent_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(team_asset_review_conflict)?;
        if let Some((asset_id, revision)) = resolved_asset {
            let valid_asset: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM local_agent_team_assets WHERE owner_user_id = ?1 \
                     AND team_room_id = ?2 AND id = ?3 AND revision = ?4 AND status = 'active')",
                    params![owner_user_id, team_id, asset_id, revision],
                    |row| row.get(0),
                )
                .map_err(|_| team_store_error())?;
            if !valid_asset {
                return Err(team_asset_review_conflict());
            }
        }
        let changed = transaction
            .execute(
                r#"
                UPDATE local_agent_team_asset_reviews
                SET status = ?1, resolved_asset_id = ?2, resolved_asset_revision = ?3,
                    reviewed_by_agent_id = ?4, review_reason = ?5,
                    updated_at_unix_ms = ?6
                WHERE owner_user_id = ?7 AND id = ?8 AND status = 'pending'
                "#,
                params![
                    outcome,
                    resolved_asset.map(|value| value.0),
                    resolved_asset.map(|value| value.1),
                    reviewer_agent_id,
                    reason,
                    now_unix_ms,
                    owner_user_id,
                    review_id,
                ],
            )
            .map_err(|_| team_store_error())?;
        if changed != 1 {
            return Err(team_asset_review_conflict());
        }
        let review = transaction
            .query_row(
                r#"
                SELECT review.id, review.team_room_id, review.todo_id, todo.title,
                       review.source_agent_id, profile.name, review.category, review.title,
                       review.markdown, review.rationale, review.status,
                       review.resolved_asset_id, review.resolved_asset_revision,
                       review.reviewed_by_agent_id, review.review_reason,
                       review.created_at_unix_ms, review.updated_at_unix_ms
                FROM local_agent_team_asset_reviews review
                JOIN local_agent_todos todo
                  ON todo.owner_user_id = review.owner_user_id AND todo.id = review.todo_id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = review.owner_user_id
                 AND profile.id = review.source_agent_id
                WHERE review.owner_user_id = ?1 AND review.id = ?2
                "#,
                params![owner_user_id, review_id],
                map_team_asset_review,
            )
            .map_err(|_| team_store_error())?;
        validate_team_asset_review(&review)?;
        transaction.commit().map_err(|_| team_store_error())?;
        Ok(review)
    }
}
