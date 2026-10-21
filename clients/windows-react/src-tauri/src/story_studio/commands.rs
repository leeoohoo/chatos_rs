#![cfg_attr(not(test), allow(dead_code))]

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::{
    authentication::AuthenticationManager, host_lifecycle::validate_owner,
    media_studio::MediaStudioManager,
};

use super::{
    application::{
        StoryImageConfirmationInput, StoryImageImportInput, StoryLastFrameClearInput,
        StoryLatestFramesInput, StoryLatestResourceImagesInput, StoryProjectCreationInput,
        StoryProjectSettingsInput, StoryProjectSourceInput, StoryResourcePromptInput,
        StorySegmentEditInput, StorySegmentStructureInput,
    },
    media::StoryStudioManager,
    planning::{StoryPlanningRunSnapshot, StoryPlanningStage},
    projection::StoryProjectSnapshot,
    runtime::{abandon_run, StoryPlanningOutcome, StoryPlanningRuntime},
    store::{StoryPlanningHistoryPage, StoryProjectCursor, StoryProjectPage},
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
pub(crate) struct StoryProjectSourceCommandInput {
    owner_user_id: String,
    presentation_id: String,
    edit: StoryProjectSourceInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryResourcePromptCommandInput {
    owner_user_id: String,
    presentation_id: String,
    edit: StoryResourcePromptInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StorySegmentEditCommandInput {
    owner_user_id: String,
    presentation_id: String,
    edit: StorySegmentEditInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StorySegmentStructureCommandInput {
    owner_user_id: String,
    presentation_id: String,
    edit: StorySegmentStructureInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryImageConfirmationCommandInput {
    owner_user_id: String,
    presentation_id: String,
    confirmation: StoryImageConfirmationInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryLastFrameClearCommandInput {
    owner_user_id: String,
    presentation_id: String,
    change: StoryLastFrameClearInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryLatestResourceImagesCommandInput {
    owner_user_id: String,
    presentation_id: String,
    confirmation: StoryLatestResourceImagesInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryLatestFramesCommandInput {
    owner_user_id: String,
    presentation_id: String,
    confirmation: StoryLatestFramesInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryLocalImageImportCommandInput {
    owner_user_id: String,
    presentation_id: String,
    import: StoryImageImportInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryHistoryImageImportCommandInput {
    owner_user_id: String,
    presentation_id: String,
    record_id: String,
    content_token: String,
    import: StoryImageImportInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryPresentationInput {
    owner_user_id: String,
    presentation_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryPlanningStartInput {
    owner_user_id: String,
    presentation_id: String,
    project_id: String,
    stage: StoryPlanningStage,
    target_ids: Vec<String>,
    user_ideas: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryPlanningResumeInput {
    owner_user_id: String,
    presentation_id: String,
    project_id: String,
    run_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryPlanningAbandonInput {
    owner_user_id: String,
    presentation_id: String,
    project_id: String,
    run_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryPlanningHistoryInput {
    owner_user_id: String,
    presentation_id: String,
    project_id: String,
    after: Option<StoryProjectCursor>,
    limit: Option<usize>,
}

#[tauri::command]
pub(crate) async fn load_story_planning_history(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryPlanningHistoryInput,
) -> Result<StoryPlanningHistoryPage, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    validate_planning_presentation(
        &manager,
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.project_id,
    )
    .await?;
    let store = manager.store();
    let owner = input.owner_user_id;
    let load_owner = owner.clone();
    let presentation = input.presentation_id;
    let project_id = input.project_id;
    let load_project_id = project_id.clone();
    let mut page = tauri::async_runtime::spawn_blocking(move || {
        store.load_planning_history(
            &load_owner,
            &load_project_id,
            input.after,
            input.limit.unwrap_or(20),
        )
    })
    .await
    .map_err(|_| "剧情规划历史读取任务异常退出。".to_owned())??;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    if page.recovered_project.is_some() {
        page.recovered_project =
            Some(present_project(&manager, &owner, generation, &presentation, &project_id).await?);
    }
    Ok(page)
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
pub(crate) async fn update_story_project_source(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryProjectSourceCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let save_owner = owner.clone();
    let presentation_id = input.presentation_id;
    let edit = input.edit;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        work.update_project_source(&save_owner, generation, &presentation_id, edit)
    })
    .await
    .map_err(|_| "剧情原文保存任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    saved
}

#[tauri::command]
pub(crate) async fn update_story_resource_prompt(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryResourcePromptCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let save_owner = owner.clone();
    let presentation_id = input.presentation_id;
    let edit = input.edit;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        work.update_resource_prompt(&save_owner, generation, &presentation_id, edit)
    })
    .await
    .map_err(|_| "剧情素材描述保存任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    saved
}

#[tauri::command]
pub(crate) async fn update_story_segment(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StorySegmentEditCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let save_owner = owner.clone();
    let presentation_id = input.presentation_id;
    let edit = input.edit;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        work.update_segment(&save_owner, generation, &presentation_id, edit)
    })
    .await
    .map_err(|_| "剧情分段保存任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    saved
}

#[tauri::command]
pub(crate) async fn update_story_segment_structure(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StorySegmentStructureCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let save_owner = owner.clone();
    let presentation_id = input.presentation_id;
    let edit = input.edit;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        work.update_segment_structure(&save_owner, generation, &presentation_id, edit)
    })
    .await
    .map_err(|_| "剧情结构保存任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    saved
}

#[tauri::command]
pub(crate) async fn confirm_story_image(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryImageConfirmationCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        work.confirm_story_image(
            &saved_owner,
            generation,
            &input.presentation_id,
            input.confirmation,
        )
    })
    .await
    .map_err(|_| "剧情图片确认任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    result
}

#[tauri::command]
pub(crate) async fn clear_story_last_frame(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryLastFrameClearCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        work.clear_story_last_frame(
            &saved_owner,
            generation,
            &input.presentation_id,
            input.change,
        )
    })
    .await
    .map_err(|_| "剧情尾帧取消任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    result
}

#[tauri::command]
pub(crate) async fn confirm_latest_story_resource_images(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryLatestResourceImagesCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        work.confirm_latest_story_resource_images(
            &saved_owner,
            generation,
            &input.presentation_id,
            input.confirmation,
        )
    })
    .await
    .map_err(|_| "剧情素材批量确认任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    result
}

#[tauri::command]
pub(crate) async fn confirm_latest_story_frames(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryLatestFramesCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        work.confirm_latest_story_frames(
            &saved_owner,
            generation,
            &input.presentation_id,
            input.confirmation,
        )
    })
    .await
    .map_err(|_| "剧情帧批量确认任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    result
}

#[tauri::command]
pub(crate) async fn import_local_story_image(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryLocalImageImportCommandInput,
) -> Result<Option<StoryProjectSnapshot>, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let Some(handle) = rfd::AsyncFileDialog::new()
        .set_title("选择剧情图片")
        .add_filter("图片", &["png", "jpg", "jpeg", "webp"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let path = handle.path().to_path_buf();
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        work.import_local_story_image(
            &saved_owner,
            generation,
            &input.presentation_id,
            input.import,
            &path,
        )
    })
    .await
    .map_err(|_| "剧情图片导入任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    result.map(Some)
}

#[tauri::command]
pub(crate) async fn import_story_history_image(
    authentication: State<'_, AuthenticationManager>,
    media_manager: State<'_, MediaStudioManager>,
    story_manager: State<'_, StoryStudioManager>,
    input: StoryHistoryImageImportCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let media = media_manager.inner().clone();
    let story = story_manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let bytes =
            media.read_story_history_image(&saved_owner, &input.record_id, &input.content_token)?;
        story.import_generated_story_image(
            &saved_owner,
            generation,
            &input.presentation_id,
            input.import,
            &bytes,
        )
    })
    .await
    .map_err(|_| "生成图片导入任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    result
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

#[tauri::command]
pub(crate) async fn start_story_planning(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryPlanningStartInput,
) -> Result<StoryPlanningOutcome, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    validate_planning_presentation(
        &manager,
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.project_id,
    )
    .await?;
    let runtime = StoryPlanningRuntime {
        authentication: &authentication,
        manager: &manager,
        app: &app,
        owner_user_id: &input.owner_user_id,
        authentication_generation: generation,
        presentation_id: &input.presentation_id,
    };
    let mut outcome = runtime
        .start(
            &input.project_id,
            input.stage,
            input.target_ids,
            input.user_ideas,
        )
        .await?;
    require_current_generation(&authentication, &manager, &input.owner_user_id, generation).await?;
    if outcome.project.is_some() {
        outcome.project = Some(
            present_project(
                &manager,
                &input.owner_user_id,
                generation,
                &input.presentation_id,
                &input.project_id,
            )
            .await?,
        );
    }
    Ok(outcome)
}

#[tauri::command]
pub(crate) async fn resume_story_planning(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryPlanningResumeInput,
) -> Result<StoryPlanningOutcome, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    validate_planning_presentation(
        &manager,
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.project_id,
    )
    .await?;
    let runtime = StoryPlanningRuntime {
        authentication: &authentication,
        manager: &manager,
        app: &app,
        owner_user_id: &input.owner_user_id,
        authentication_generation: generation,
        presentation_id: &input.presentation_id,
    };
    let mut outcome = runtime.resume(&input.project_id, &input.run_id).await?;
    require_current_generation(&authentication, &manager, &input.owner_user_id, generation).await?;
    if outcome.project.is_some() {
        outcome.project = Some(
            present_project(
                &manager,
                &input.owner_user_id,
                generation,
                &input.presentation_id,
                &input.project_id,
            )
            .await?,
        );
    }
    Ok(outcome)
}

#[tauri::command]
pub(crate) async fn abandon_story_planning(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryPlanningAbandonInput,
) -> Result<StoryPlanningRunSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    validate_planning_presentation(
        &manager,
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.project_id,
    )
    .await?;
    let snapshot = abandon_run(
        &authentication,
        &manager,
        &input.owner_user_id,
        generation,
        &input.project_id,
        &input.run_id,
    )
    .await?;
    app.emit(
        "story-planning-run",
        serde_json::json!({
            "presentationId": input.presentation_id,
            "run": snapshot.clone()
        }),
    )
    .map_err(|_| "剧情规划状态发布失败。".to_owned())?;
    Ok(snapshot)
}

async fn validate_planning_presentation(
    manager: &StoryStudioManager,
    owner_user_id: &str,
    generation: u64,
    presentation_id: &str,
    project_id: &str,
) -> Result<(), String> {
    present_project(
        manager,
        owner_user_id,
        generation,
        presentation_id,
        project_id,
    )
    .await
    .map(|_| ())
}

async fn present_project(
    manager: &StoryStudioManager,
    owner_user_id: &str,
    generation: u64,
    presentation_id: &str,
    project_id: &str,
) -> Result<StoryProjectSnapshot, String> {
    let work = manager.clone();
    let owner = owner_user_id.to_owned();
    let presentation = presentation_id.to_owned();
    let project = project_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        work.present_project(&owner, generation, &presentation, &project)
    })
    .await
    .map_err(|_| "剧情项目读取任务异常退出。".to_owned())?
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
