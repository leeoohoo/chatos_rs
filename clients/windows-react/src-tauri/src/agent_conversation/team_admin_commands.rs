use super::*;

#[tauri::command]
pub(crate) async fn list_agent_team_assets(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamAsset>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let assets =
        tauri::async_runtime::spawn_blocking(move || store.list_project_team_assets(&owner, &team))
            .await
            .map_err(|_| "团队共享资产读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(assets)
}

#[tauri::command]
pub(crate) async fn list_agent_team_asset_reviews(
    owner_user_id: String,
    team_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamAssetReview>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let reviews = tauri::async_runtime::spawn_blocking(move || {
        store.list_team_asset_reviews(&owner, Some(&team), None, "pending", 500)
    })
    .await
    .map_err(|_| "团队共享资产审核读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(reviews)
}

#[tauri::command]
pub(crate) async fn save_agent_team_asset(
    input: SaveAgentTeamAssetInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamAsset, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    if let Some(asset_id) = input.asset_id.as_deref() {
        validate_identifier(asset_id).map_err(|_| invalid_team_asset_input())?;
    }
    validate_team_asset_category(&input.category)?;
    let title = input.title.trim().to_owned();
    validate_text(&title, 240, false).map_err(|_| invalid_team_asset_input())?;
    validate_text(&input.markdown, 128_000, true).map_err(|_| invalid_team_asset_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let asset_id = input.asset_id.clone();
    let category = input.category.clone();
    let markdown = input.markdown.clone();
    let expected_revision = input.expected_revision;
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let asset = tauri::async_runtime::spawn_blocking(move || {
        store.save_project_team_asset(
            &owner,
            &team,
            asset_id.as_deref(),
            &category,
            &title,
            &markdown,
            expected_revision,
            now,
        )
    })
    .await
    .map_err(|_| "团队共享资产保存任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(asset)
}

#[tauri::command]
pub(crate) async fn archive_agent_team_asset(
    input: ArchiveAgentTeamAssetInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamAsset, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_identifier(&input.asset_id).map_err(|_| invalid_team_asset_input())?;
    if input.expected_revision <= 0 {
        return Err(invalid_team_asset_input());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let asset_id = input.asset_id.clone();
    let revision = input.expected_revision;
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let asset = tauri::async_runtime::spawn_blocking(move || {
        store.archive_project_team_asset(&owner, &team, &asset_id, revision, now)
    })
    .await
    .map_err(|_| "团队共享资产归档任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(asset)
}

#[tauri::command]
pub(crate) async fn list_agent_team_asset_revisions(
    owner_user_id: String,
    team_id: String,
    asset_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamAssetRevision>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    validate_identifier(&asset_id).map_err(|_| invalid_team_asset_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let selected_asset = asset_id.clone();
    let revisions = tauri::async_runtime::spawn_blocking(move || {
        store.list_project_team_asset_revisions(&owner, &team, &selected_asset)
    })
    .await
    .map_err(|_| "团队共享资产版本读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(revisions)
}

#[tauri::command]
pub(crate) async fn invite_agent_to_team(
    input: InviteAgentToTeamInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_agent_identifier(&input.agent_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let agent = input.agent_id.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || {
        store.invite_agent_to_project_team(&owner, &team, &agent)
    })
    .await
    .map_err(|_| "Agent 团队邀请任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(detail)
}

#[tauri::command]
pub(crate) async fn update_agent_team_member(
    input: UpdateAgentTeamMemberInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_agent_identifier(&input.agent_id).map_err(|_| invalid_team_input())?;
    validate_text(&input.role, 160, false).map_err(|_| invalid_team_input())?;
    validate_text(&input.responsibility, 8_000, true).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let agent = input.agent_id.clone();
    let role = input.role.clone();
    let responsibility = input.responsibility.clone();
    let detail = tauri::async_runtime::spawn_blocking(move || {
        store.update_project_team_member(&owner, &team, &agent, &role, &responsibility)
    })
    .await
    .map_err(|_| "Agent 团队成员保存任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(detail)
}

async fn manage_agent_team_member(
    input: ManageAgentTeamMemberInput,
    authentication: &AuthenticationManager,
    conversations: &AgentConversationStore,
    operation: fn(&AgentConversationStore, &str, &str, &str) -> Result<AgentTeamDetail, String>,
    task_error: &'static str,
) -> Result<AgentTeamDetail, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.team_id).map_err(|_| invalid_team_input())?;
    validate_agent_identifier(&input.agent_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.clone();
    let owner = input.owner_user_id.clone();
    let team = input.team_id.clone();
    let agent = input.agent_id.clone();
    let detail =
        tauri::async_runtime::spawn_blocking(move || operation(&store, &owner, &team, &agent))
            .await
            .map_err(|_| task_error.to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(detail)
}

#[tauri::command]
pub(crate) async fn set_agent_team_default_agent(
    input: ManageAgentTeamMemberInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    manage_agent_team_member(
        input,
        authentication.inner(),
        conversations.inner(),
        AgentConversationStore::set_project_team_default_agent,
        "默认 Agent 设置任务异常退出。",
    )
    .await
}

#[tauri::command]
pub(crate) async fn set_agent_team_project_manager(
    input: ManageAgentTeamMemberInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    manage_agent_team_member(
        input,
        authentication.inner(),
        conversations.inner(),
        AgentConversationStore::set_project_team_manager,
        "项目经理设置任务异常退出。",
    )
    .await
}

#[tauri::command]
pub(crate) async fn remove_agent_from_team(
    input: ManageAgentTeamMemberInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamDetail, String> {
    manage_agent_team_member(
        input,
        authentication.inner(),
        conversations.inner(),
        AgentConversationStore::remove_agent_from_project_team,
        "Agent 移出团队任务异常退出。",
    )
    .await
}
