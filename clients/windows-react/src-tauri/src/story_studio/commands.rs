#![cfg_attr(not(test), allow(dead_code))]

use serde::Deserialize;
use tauri::State;

use crate::{authentication::AuthenticationManager, host_lifecycle::validate_owner};

use super::{
    application::{StoryProjectCreationInput, StoryProjectSettingsInput},
    media::StoryStudioManager,
    projection::StoryProjectSnapshot,
    store::{StoryProjectCursor, StoryProjectPage},
};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryCatalogInput {
    owner_user_id: String,
    presentation_id: String,
    after: Option<StoryProjectCursor>,
    limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryProjectReadInput {
    owner_user_id: String,
    presentation_id: String,
    project_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryProjectCreateCommandInput {
    owner_user_id: String,
    presentation_id: String,
    project: StoryProjectCreationInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryProjectSettingsCommandInput {
    owner_user_id: String,
    presentation_id: String,
    change: StoryProjectSettingsInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryPresentationInput {
    owner_user_id: String,
    presentation_id: String,
}

#[tauri::command]
pub(crate) async fn load_story_projects(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryCatalogInput,
) -> Result<StoryProjectPage, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let load_owner = owner.clone();
    let presentation_id = input.presentation_id;
    let after = input.after;
    let limit = input.limit.unwrap_or(40);
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        work.present_page(&load_owner, generation, &presentation_id, after, limit)
    })
    .await
    .map_err(|_| "剧情项目目录读取任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    loaded
}

#[tauri::command]
pub(crate) async fn load_story_project(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryProjectReadInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let load_owner = owner.clone();
    let presentation_id = input.presentation_id;
    let project_id = input.project_id;
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        work.present_project(&load_owner, generation, &presentation_id, &project_id)
    })
    .await
    .map_err(|_| "剧情项目读取任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    loaded
}

#[tauri::command]
pub(crate) async fn create_story_project(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryProjectCreateCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let catalog =
        load_current_catalog(&authentication, &manager, &input.owner_user_id, generation).await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let save_owner = owner.clone();
    let presentation_id = input.presentation_id;
    let project = input.project;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        work.create_project(&save_owner, generation, &presentation_id, project, &catalog)
    })
    .await
    .map_err(|_| "剧情项目创建任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    saved
}

#[tauri::command]
pub(crate) async fn update_story_project_settings(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryProjectSettingsCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let catalog =
        load_current_catalog(&authentication, &manager, &input.owner_user_id, generation).await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let save_owner = owner.clone();
    let presentation_id = input.presentation_id;
    let change = input.change;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        work.update_project_settings(&save_owner, generation, &presentation_id, change, &catalog)
    })
    .await
    .map_err(|_| "剧情设置保存任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    saved
}

#[tauri::command]
pub(crate) async fn discard_story_studio_presentation(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryPresentationInput,
) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    manager.discard_presentation(&input.owner_user_id, generation, &input.presentation_id)
}

async fn load_current_catalog(
    authentication: &AuthenticationManager,
    manager: &StoryStudioManager,
    owner_user_id: &str,
    generation: u64,
) -> Result<Vec<crate::authentication::GatewayModelConfig>, String> {
    let catalog = authentication.model_catalog_for_owner(owner_user_id).await;
    require_current_generation(authentication, manager, owner_user_id, generation).await?;
    catalog.map_err(|error| error.message)
}

async fn require_current_generation(
    authentication: &AuthenticationManager,
    manager: &StoryStudioManager,
    owner_user_id: &str,
    generation: u64,
) -> Result<(), String> {
    if let Err(error) = authentication
        .require_session_generation(owner_user_id, generation)
        .await
    {
        manager.reset_session_if_matches(owner_user_id, generation);
        return Err(error);
    }
    Ok(())
}
