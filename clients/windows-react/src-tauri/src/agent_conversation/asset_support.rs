use super::*;

pub(super) fn map_team_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTeamAsset> {
    Ok(AgentTeamAsset {
        id: row.get(0)?,
        team_id: row.get(1)?,
        category: row.get(2)?,
        title: row.get(3)?,
        markdown: row.get(4)?,
        revision: row.get(5)?,
        status: row.get(6)?,
        created_by_agent_id: row.get(7)?,
        updated_by_agent_id: row.get(8)?,
        created_at_unix_ms: row.get(9)?,
        updated_at_unix_ms: row.get(10)?,
    })
}

pub(super) fn map_team_asset_review(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AgentTeamAssetReview> {
    Ok(AgentTeamAssetReview {
        id: row.get(0)?,
        team_id: row.get(1)?,
        todo_id: row.get(2)?,
        todo_title: row.get(3)?,
        source_agent_id: row.get(4)?,
        source_agent_name: row.get(5)?,
        suggestion: AgentTeamAssetUpdateSuggestion {
            category: row.get(6)?,
            title: row.get(7)?,
            markdown: row.get(8)?,
            rationale: row.get(9)?,
        },
        status: row.get(10)?,
        resolved_asset_id: row.get(11)?,
        resolved_asset_revision: row.get(12)?,
        reviewed_by_agent_id: row.get(13)?,
        review_reason: row.get(14)?,
        created_at_unix_ms: row.get(15)?,
        updated_at_unix_ms: row.get(16)?,
    })
}

pub(super) fn load_agent_asset_tool_binding(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
) -> Result<AgentAssetToolBinding, String> {
    let binding = connection
        .query_row(
            r#"
            SELECT delivery.room_id, delivery.target_agent_id, todo.id,
                   CASE WHEN room.project_manager_agent_id = delivery.target_agent_id
                        THEN 1 ELSE 0 END,
                   delivery.trigger_kind
            FROM windows_agent_delivery_runs run
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = run.owner_user_id
             AND delivery.id = run.delivery_id
            JOIN project_agent_rooms room
              ON room.owner_user_id = delivery.owner_user_id
             AND room.id = delivery.room_id
            JOIN project_agent_room_members member
              ON member.owner_user_id = delivery.owner_user_id
             AND member.room_id = delivery.room_id
             AND member.agent_id = delivery.target_agent_id
            LEFT JOIN local_agent_todos todo
              ON todo.owner_user_id = delivery.owner_user_id
             AND todo.team_room_id = delivery.room_id
             AND todo.agent_id = delivery.target_agent_id
             AND delivery.trigger_kind = 'todo'
             AND (delivery.deduplication_key = 'todo:' || todo.id
                  OR delivery.deduplication_key LIKE 'todo:' || todo.id || ':attempt:%')
            WHERE run.owner_user_id = ?1 AND run.run_id = ?2
              AND run.status IN ('ready', 'running')
              AND delivery.status = 'running'
              AND room.status = 'active' AND room.conversation_kind = 'project_team'
              AND member.status = 'active'
            LIMIT 1
            "#,
            params![owner_user_id, run_id],
            |row| {
                Ok((
                    AgentAssetToolBinding {
                        team_id: row.get(0)?,
                        agent_id: row.get(1)?,
                        todo_id: row.get(2)?,
                        is_project_manager: row.get::<_, i64>(3)? == 1,
                    },
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| team_store_error())?
        .ok_or_else(|| "The Agent team asset Run binding is unavailable.".to_owned())?;
    match (binding.1.as_str(), binding.0.todo_id.is_some()) {
        ("todo", true) => {}
        ("todo", false) | (_, true) => return Err(team_store_error()),
        _ => {}
    }
    Ok(binding.0)
}

pub(super) fn load_agent_communication_tool_binding(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
) -> Result<AgentCommunicationToolBinding, String> {
    connection
        .query_row(
            r#"
            SELECT delivery.room_id, delivery.target_agent_id,
                   room.conversation_kind,
                   CASE WHEN room.project_manager_agent_id = delivery.target_agent_id
                        THEN 1 ELSE 0 END
            FROM windows_agent_delivery_runs run
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = run.owner_user_id
             AND delivery.id = run.delivery_id
            JOIN project_agent_rooms room
              ON room.owner_user_id = delivery.owner_user_id
             AND room.id = delivery.room_id
            JOIN project_agent_room_members member
              ON member.owner_user_id = delivery.owner_user_id
             AND member.room_id = delivery.room_id
             AND member.agent_id = delivery.target_agent_id
            WHERE run.owner_user_id = ?1 AND run.run_id = ?2
              AND delivery.trigger_kind != 'todo'
              AND (
                (run.status IN ('ready', 'running') AND delivery.status = 'running')
                OR (
                  run.status = 'ready' AND delivery.status = 'pending'
                  AND delivery.trigger_kind = 'default_agent'
                  AND room.conversation_kind = 'human_agent_direct'
                )
              )
              AND room.status = 'active'
              AND room.conversation_kind IN (
                  'project_team', 'agent_agent_direct', 'human_agent_direct'
              )
              AND member.status = 'active'
            LIMIT 1
            "#,
            params![owner_user_id, run_id],
            |row| {
                Ok(AgentCommunicationToolBinding {
                    room_id: row.get(0)?,
                    agent_id: row.get(1)?,
                    conversation_kind: row.get(2)?,
                    is_project_manager: row.get::<_, i64>(3)? == 1,
                })
            },
        )
        .optional()
        .map_err(|_| team_store_error())?
        .ok_or_else(|| "The Agent communication Run binding is unavailable.".to_owned())
}

pub(super) fn require_active_project_team(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
) -> Result<(), String> {
    let exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM project_agent_rooms WHERE owner_user_id = ?1 \
             AND id = ?2 AND status = 'active' AND conversation_kind = 'project_team')",
            params![owner_user_id, team_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| team_store_error())?;
    if !exists {
        return Err("Agent 团队不存在或已停用。".into());
    }
    Ok(())
}

pub(super) fn require_project_team_manager(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    let allowed = connection
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id
                 AND member.room_id = room.id
                 AND member.agent_id = room.project_manager_agent_id
                WHERE room.owner_user_id = ?1 AND room.id = ?2
                  AND room.project_manager_agent_id = ?3
                  AND room.status = 'active' AND room.conversation_kind = 'project_team'
                  AND member.status = 'active'
            )
            "#,
            params![owner_user_id, team_id, agent_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| team_store_error())?;
    if !allowed {
        return Err("Only the active project manager Agent may change team assets.".into());
    }
    Ok(())
}

pub(super) fn validate_team_asset_category(category: &str) -> Result<(), String> {
    if matches!(
        category,
        "overview"
            | "current_progress"
            | "tech_stack"
            | "architecture"
            | "conventions"
            | "decision"
            | "reference"
    ) {
        Ok(())
    } else {
        Err(invalid_team_asset_input())
    }
}

pub(super) fn validate_team_asset(asset: &AgentTeamAsset) -> Result<(), String> {
    validate_identifier(&asset.id).map_err(|_| team_store_error())?;
    validate_identifier(&asset.team_id).map_err(|_| team_store_error())?;
    validate_team_asset_category(&asset.category).map_err(|_| team_store_error())?;
    validate_text(&asset.title, 240, false).map_err(|_| team_store_error())?;
    validate_text(&asset.markdown, 128_000, true).map_err(|_| team_store_error())?;
    if !matches!(asset.status.as_str(), "active" | "archived")
        || asset.revision <= 0
        || asset.created_at_unix_ms < 0
        || asset.updated_at_unix_ms < asset.created_at_unix_ms
    {
        return Err(team_store_error());
    }
    if let Some(agent_id) = asset.created_by_agent_id.as_deref() {
        validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    if let Some(agent_id) = asset.updated_by_agent_id.as_deref() {
        validate_agent_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    Ok(())
}

pub(super) fn validate_team_asset_suggestion(
    suggestion: &AgentTeamAssetUpdateSuggestion,
) -> Result<(), String> {
    validate_team_asset_category(&suggestion.category)
        .map_err(|_| invalid_team_asset_review_input())?;
    validate_text(&suggestion.title, 240, false).map_err(|_| invalid_team_asset_review_input())?;
    validate_text(&suggestion.markdown, 128_000, false)
        .map_err(|_| invalid_team_asset_review_input())?;
    validate_text(&suggestion.rationale, 4_000, false)
        .map_err(|_| invalid_team_asset_review_input())
}

pub(super) fn validate_team_asset_review(review: &AgentTeamAssetReview) -> Result<(), String> {
    for value in [
        review.id.as_str(),
        review.team_id.as_str(),
        review.todo_id.as_str(),
        review.source_agent_id.as_str(),
    ] {
        validate_identifier(value).map_err(|_| team_store_error())?;
    }
    validate_text(&review.todo_title, 500, false).map_err(|_| team_store_error())?;
    validate_text(&review.source_agent_name, 120, false).map_err(|_| team_store_error())?;
    validate_team_asset_suggestion(&review.suggestion).map_err(|_| team_store_error())?;
    if review.created_at_unix_ms < 0 || review.updated_at_unix_ms < review.created_at_unix_ms {
        return Err(team_store_error());
    }
    if let Some(asset_id) = review.resolved_asset_id.as_deref() {
        validate_identifier(asset_id).map_err(|_| team_store_error())?;
    }
    if let Some(agent_id) = review.reviewed_by_agent_id.as_deref() {
        validate_identifier(agent_id).map_err(|_| team_store_error())?;
    }
    validate_text(&review.review_reason, 4_000, true).map_err(|_| team_store_error())?;
    match review.status.as_str() {
        "pending"
            if review.resolved_asset_id.is_none()
                && review.resolved_asset_revision.is_none()
                && review.reviewed_by_agent_id.is_none()
                && review.review_reason.is_empty() => {}
        "applied"
            if review.resolved_asset_id.is_some()
                && review
                    .resolved_asset_revision
                    .is_some_and(|revision| revision > 0)
                && review.reviewed_by_agent_id.is_some() => {}
        "rejected"
            if review.resolved_asset_id.is_none()
                && review.resolved_asset_revision.is_none()
                && review.reviewed_by_agent_id.is_some()
                && !review.review_reason.trim().is_empty() => {}
        _ => return Err(team_store_error()),
    }
    Ok(())
}

pub(super) fn validate_team_execution_binding(
    binding: &TeamExecutionBinding,
) -> Result<(), String> {
    let agent = &binding.agent;
    validate_agent_execution_binding(agent)?;
    validate_identifier(&binding.project_id).map_err(|_| team_store_error())?;
    validate_text(&binding.team_name, 160, false).map_err(|_| team_store_error())?;
    validate_text(&binding.team_goal, 8_000, true).map_err(|_| team_store_error())?;
    validate_text(&binding.team_role, 160, false).map_err(|_| team_store_error())?;
    validate_text(&binding.team_responsibility, 8_000, true).map_err(|_| team_store_error())
}

pub(super) fn validate_agent_execution_binding(
    binding: &AgentExecutionBinding,
) -> Result<(), String> {
    let agent = binding;
    validate_identifier(&agent.room_id).map_err(|_| team_store_error())?;
    validate_agent_identifier(&agent.agent_id).map_err(|_| team_store_error())?;
    validate_text(&agent.name, 120, false).map_err(|_| team_store_error())?;
    validate_text(&agent.role_prompt, 32_000, false).map_err(|_| team_store_error())?;
    validate_identifier(&agent.model_config_id).map_err(|_| team_store_error())?;
    validate_identifier(&agent.profession_key).map_err(|_| team_store_error())?;
    if agent.thinking_level.as_deref().is_some_and(|level| {
        !matches!(
            level,
            "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
        )
    }) {
        return Err(team_store_error());
    }
    Ok(())
}

pub(super) fn agent_communication_conversation_id(room_id: &str, agent_id: &str) -> String {
    format!("agent-communication-v2-{room_id}-{agent_id}")
}

pub(super) fn agent_direct_conversation_id(room_id: &str) -> String {
    format!("agent-direct-v2-{room_id}")
}

pub(super) fn agent_team_conversation_id(room_id: &str, agent_id: &str) -> String {
    format!("agent-team-v2-{room_id}-{agent_id}")
}

pub(super) fn agent_team_attachment_conversation_id(team_id: &str) -> String {
    format!("agent-team-compose-{team_id}")
}

pub(super) fn require_active_project(
    database: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<(), String> {
    let connection = crate::workspace_index::open_project_registry(database)?;
    let project = connection
        .query_row(
            "SELECT name, revision FROM local_project_records \
             WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'",
            params![owner_user_id, project_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|_| "项目注册表包含无效数据。".to_owned())?;
    let Some((name, revision)) = project else {
        return Err("项目不存在或已停用，不能创建 Agent 团队。".into());
    };
    if name.trim().is_empty()
        || name.chars().count() > 1_024
        || name.chars().any(char::is_control)
        || revision <= 0
    {
        return Err("项目注册表包含无效数据。".into());
    }
    Ok(())
}

pub(super) fn parse_identifier_array(json: &str) -> Result<Vec<String>, String> {
    let values: Vec<String> = serde_json::from_str(json).map_err(|_| store_error())?;
    if values.len() > 100 {
        return Err(store_error());
    }
    let mut unique = std::collections::HashSet::new();
    for value in &values {
        validate_identifier(value).map_err(|_| store_error())?;
        if !unique.insert(value) {
            return Err(store_error());
        }
    }
    Ok(values)
}

pub(super) fn store_error() -> String {
    "Agent 私聊数据不可用，请检查本机存储后重试。".into()
}

pub(super) fn team_store_error() -> String {
    "Agent 团队数据不可用，请检查本机存储后重试。".into()
}

pub(super) fn invalid_team_input() -> String {
    "Agent 团队信息无效，请检查后重试。".into()
}

pub(super) fn invalid_team_message() -> String {
    "Agent 团队消息无效，请检查内容和 @Agent 后重试。".into()
}

pub(super) fn invalid_team_asset_input() -> String {
    "团队共享资产内容无效。".into()
}

pub(super) fn invalid_team_asset_review_input() -> String {
    "团队共享资产审核内容无效。".into()
}

pub(super) fn invalid_project_dashboard_input() -> String {
    "项目总览内容无效。".into()
}

pub(super) fn project_dashboard_conflict() -> String {
    "项目总览已被更新，请重新读取后合并修改。".into()
}

pub(super) fn team_asset_conflict() -> String {
    "团队共享资产已被更新，请刷新后重试。".into()
}

pub(super) fn team_asset_review_conflict() -> String {
    "团队共享资产审核已处理或身份不匹配，请重新读取后重试。".into()
}
