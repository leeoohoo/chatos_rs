use super::*;

pub(super) fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.chars().count() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err("Agent 私聊标识无效。".into());
    }
    Ok(())
}

pub(super) fn validate_agent_identifier(value: &str) -> Result<(), String> {
    validate_identifier(value)?;
    if value.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    }) {
        return Err("Agent 私聊标识无效。".into());
    }
    Ok(())
}

pub(super) fn validate_text(value: &str, maximum: usize, optional: bool) -> Result<(), String> {
    if (!optional && value.is_empty())
        || value.chars().count() > maximum
        || value.trim() != value
        || value.contains('\0')
    {
        return Err(store_error());
    }
    Ok(())
}

pub(super) fn validate_team_summary(team: &AgentTeamSummary) -> Result<(), String> {
    validate_identifier(&team.id).map_err(|_| team_store_error())?;
    validate_identifier(&team.project_id).map_err(|_| team_store_error())?;
    validate_text(&team.name, 160, false).map_err(|_| team_store_error())?;
    validate_text(&team.goal, 8_000, true).map_err(|_| team_store_error())?;
    if let Some(agent_id) = team.default_agent_id.as_deref() {
        validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    if let Some(agent_id) = team.project_manager_agent_id.as_deref() {
        validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    if team.member_count < 0
        || team.created_at_unix_ms < 0
        || team.updated_at_unix_ms < team.created_at_unix_ms
    {
        return Err(team_store_error());
    }
    Ok(())
}

pub(super) fn validate_team_member(member: &AgentTeamMemberSummary) -> Result<(), String> {
    validate_agent_identifier(&member.agent_id).map_err(|_| team_store_error())?;
    validate_text(&member.role, 160, false).map_err(|_| team_store_error())?;
    validate_text(&member.responsibility, 8_000, true).map_err(|_| team_store_error())?;
    if member.joined_at_unix_ms < 0 {
        return Err(team_store_error());
    }
    Ok(())
}

pub(super) fn read_project_dashboard(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
) -> Result<Option<AgentProjectDashboard>, String> {
    let stored = connection
        .query_row(
            r#"
            SELECT dashboard_json, revision, updated_by_agent_id, updated_at_unix_ms
            FROM local_agent_project_dashboards
            WHERE owner_user_id = ?1 AND team_room_id = ?2
            LIMIT 1
            "#,
            params![owner_user_id, team_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|_| team_store_error())?;
    let Some((encoded, revision, updated_by_agent_id, updated_at_unix_ms)) = stored else {
        return Ok(None);
    };
    let update = serde_json::from_str::<AgentProjectDashboardUpdate>(&encoded)
        .map_err(|_| team_store_error())?;
    validate_project_dashboard_update(&update).map_err(|_| team_store_error())?;
    validate_agent_identifier(&updated_by_agent_id).map_err(|_| team_store_error())?;
    if revision <= 0 || updated_at_unix_ms < 0 {
        return Err(team_store_error());
    }
    Ok(Some(AgentProjectDashboard {
        team_id: team_id.to_owned(),
        phase: update.phase,
        health: update.health,
        summary: update.summary,
        next_steps: update.next_steps,
        milestones: update.milestones,
        issues: update.issues,
        revision,
        updated_by_agent_id,
        updated_at_unix_ms,
    }))
}

pub(super) fn validate_project_dashboard_update(
    update: &AgentProjectDashboardUpdate,
) -> Result<(), String> {
    validate_text(&update.phase, 240, false).map_err(|_| invalid_project_dashboard_input())?;
    validate_text(&update.summary, 16_000, false).map_err(|_| invalid_project_dashboard_input())?;
    if !matches!(
        update.health.as_str(),
        "on_track" | "at_risk" | "blocked" | "completed"
    ) || update.next_steps.len() > 32
        || update.milestones.len() > 64
        || update.issues.len() > 64
    {
        return Err(invalid_project_dashboard_input());
    }
    let mut milestone_ids = std::collections::HashSet::new();
    for step in &update.next_steps {
        validate_text(step, 2_000, false).map_err(|_| invalid_project_dashboard_input())?;
    }
    for milestone in &update.milestones {
        validate_identifier(&milestone.id).map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&milestone.title, 240, false)
            .map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&milestone.detail, 8_000, true)
            .map_err(|_| invalid_project_dashboard_input())?;
        if !milestone_ids.insert(&milestone.id)
            || !matches!(
                milestone.status.as_str(),
                "pending" | "in_progress" | "blocked" | "completed"
            )
            || !(0..=100).contains(&milestone.progress_percent)
            || milestone.acceptance_criteria.len() > 32
            || milestone.linked_todo_ids.len() > 128
            || milestone.target_at_unix_ms.is_some_and(|value| value < 0)
        {
            return Err(invalid_project_dashboard_input());
        }
        let mut linked_ids = std::collections::HashSet::new();
        for criterion in &milestone.acceptance_criteria {
            validate_text(criterion, 2_000, false)
                .map_err(|_| invalid_project_dashboard_input())?;
        }
        for todo_id in &milestone.linked_todo_ids {
            validate_identifier(todo_id).map_err(|_| invalid_project_dashboard_input())?;
            if !linked_ids.insert(todo_id) {
                return Err(invalid_project_dashboard_input());
            }
        }
    }
    let mut issue_ids = std::collections::HashSet::new();
    for issue in &update.issues {
        validate_identifier(&issue.id).map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&issue.title, 240, false).map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&issue.detail, 8_000, true).map_err(|_| invalid_project_dashboard_input())?;
        validate_text(&issue.requested_action, 4_000, false)
            .map_err(|_| invalid_project_dashboard_input())?;
        if !issue_ids.insert(&issue.id)
            || !matches!(issue.severity.as_str(), "info" | "warning" | "critical")
            || !matches!(issue.owner.as_str(), "human" | "agent" | "external")
        {
            return Err(invalid_project_dashboard_input());
        }
        if let Some(todo_id) = issue.related_todo_id.as_deref() {
            validate_identifier(todo_id).map_err(|_| invalid_project_dashboard_input())?;
        }
    }
    Ok(())
}

pub(super) fn validate_project_dashboard_todo_links(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
    update: &AgentProjectDashboardUpdate,
) -> Result<(), String> {
    let todo_ids = update
        .milestones
        .iter()
        .flat_map(|milestone| milestone.linked_todo_ids.iter())
        .chain(
            update
                .issues
                .iter()
                .filter_map(|issue| issue.related_todo_id.as_ref()),
        )
        .collect::<std::collections::HashSet<_>>();
    for todo_id in todo_ids {
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_todos WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3)",
                params![owner_user_id, team_id, todo_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| team_store_error())?;
        if !exists {
            return Err(invalid_project_dashboard_input());
        }
    }
    Ok(())
}
