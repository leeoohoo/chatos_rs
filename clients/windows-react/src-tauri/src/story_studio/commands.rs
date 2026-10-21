#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::{
    authentication::AuthenticationManager,
    host_lifecycle::validate_owner,
    media_studio::{MediaStudioManager, StoryVideoPollOutcome},
};

use super::{
    application::{
        StoryAssetImageGenerationInput, StoryCompletedVideoArchiveInput,
        StoryFrameImageGenerationInput, StoryImageConfirmationInput, StoryImageImportInput,
        StoryImageRetryInput, StoryLastFrameClearInput, StoryLatestFramesInput,
        StoryLatestResourceImagesInput, StoryProjectCreationInput, StoryProjectSettingsInput,
        StoryProjectSourceInput, StoryResourcePromptInput, StorySegmentDurationInput,
        StorySegmentEditInput, StorySegmentStructureInput, StoryVideoGenerationPollInput,
        StoryVideoGenerationStartInput, StoryVideoGuidanceInput, StoryVideoRetryInput,
    },
    batch::{
        batch_owner_hash, prepare_media_batch, preview_media_batch, StoryMediaBatchHistoryPage,
        StoryMediaBatchPreviewInput, StoryMediaBatchPreviewResult, StoryMediaBatchRecord,
        StoryMediaBatchSnapshot, StoryMediaBatchStartInput,
    },
    media::StoryStudioManager,
    planning::{StoryPlanningRunSnapshot, StoryPlanningStage},
    projection::StoryProjectSnapshot,
    runtime::{abandon_run, StoryPlanningOutcome, StoryPlanningRuntime},
    store::{StoryPlanningHistoryPage, StoryProjectCursor, StoryProjectPage},
};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryMediaBatchPreviewCommandInput {
    owner_user_id: String,
    presentation_id: String,
    preview: StoryMediaBatchPreviewInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryMediaBatchHistoryInput {
    owner_user_id: String,
    presentation_id: String,
    project_id: String,
    after: Option<StoryProjectCursor>,
    limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryMediaBatchStartCommandInput {
    owner_user_id: String,
    presentation_id: String,
    request: StoryMediaBatchStartInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryMediaBatchIdentityInput {
    owner_user_id: String,
    presentation_id: String,
    project_id: String,
    batch_id: String,
}

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
pub(crate) struct StorySegmentDurationCommandInput {
    owner_user_id: String,
    presentation_id: String,
    change: StorySegmentDurationInput,
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
pub(crate) struct StoryImageRetryCommandInput {
    owner_user_id: String,
    presentation_id: String,
    target: StoryImageRetryInput,
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
pub(crate) struct StoryAssetImageGenerationCommandInput {
    owner_user_id: String,
    presentation_id: String,
    request: StoryAssetImageGenerationInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryFrameImageGenerationCommandInput {
    owner_user_id: String,
    presentation_id: String,
    request: StoryFrameImageGenerationInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryVideoGenerationStartCommandInput {
    owner_user_id: String,
    presentation_id: String,
    request: StoryVideoGenerationStartInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryVideoGenerationPollCommandInput {
    owner_user_id: String,
    presentation_id: String,
    request: StoryVideoGenerationPollInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryVideoGuidanceCommandInput {
    owner_user_id: String,
    presentation_id: String,
    change: StoryVideoGuidanceInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryCompletedVideoArchiveCommandInput {
    owner_user_id: String,
    presentation_id: String,
    change: StoryCompletedVideoArchiveInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryVideoRetryCommandInput {
    owner_user_id: String,
    presentation_id: String,
    change: StoryVideoRetryInput,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryVideoGenerationCommandOutcome {
    project: StoryProjectSnapshot,
    status: String,
    percent: Option<f64>,
    terminal: bool,
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
pub(crate) async fn preview_story_media_batch(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryMediaBatchPreviewCommandInput,
) -> Result<StoryMediaBatchPreviewResult, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    validate_planning_presentation(
        &manager,
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.preview.project_id,
    )
    .await?;
    let catalog =
        load_current_catalog(&authentication, &manager, &input.owner_user_id, generation).await?;
    let store = manager.store();
    let owner = input.owner_user_id;
    let load_owner = owner.clone();
    let preview = input.preview;
    let project_id = preview.project_id.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let project = store.load_native_project(&load_owner, &project_id)?;
        Ok(preview_media_batch(
            &project,
            &load_owner,
            &catalog,
            preview,
        ))
    })
    .await
    .map_err(|_| "批量制作预览任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    prepared
}

#[tauri::command]
pub(crate) async fn load_story_media_batch_history(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryMediaBatchHistoryInput,
) -> Result<StoryMediaBatchHistoryPage, String> {
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
    let project_id = input.project_id;
    let page = tauri::async_runtime::spawn_blocking(move || {
        store.load_media_batch_history(
            &load_owner,
            &project_id,
            input.after,
            input.limit.unwrap_or(20),
        )
    })
    .await
    .map_err(|_| "批量制作历史读取任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    page
}

#[tauri::command]
pub(crate) async fn start_story_media_batch(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryMediaBatchStartCommandInput,
) -> Result<StoryMediaBatchSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    validate_planning_presentation(
        &manager,
        &input.owner_user_id,
        generation,
        &input.presentation_id,
        &input.request.project_id,
    )
    .await?;
    let catalog =
        load_current_catalog(&authentication, &manager, &input.owner_user_id, generation).await?;
    let store = manager.store();
    let owner = input.owner_user_id;
    let persist_owner = owner.clone();
    let request = input.request;
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let project = store.load_native_project(&persist_owner, &request.project_id)?;
        let mut preparation = prepare_media_batch(
            &project,
            &persist_owner,
            &catalog,
            StoryMediaBatchPreviewInput {
                project_id: request.project_id,
                base_fingerprint: request.base_fingerprint,
                kind: request.kind,
                target_ids: Some(request.target_ids),
                user_ideas: request.user_ideas.clone(),
            },
        )?;
        preparation.preview_id = uuid::Uuid::parse_str(&request.preview_id)
            .map(|value| value.to_string())
            .map_err(|_| "批量制作预览标识无效。".to_owned())?;
        let (has_unfinished, unreadable) =
            store.inspect_media_batches(&persist_owner, &preparation.project_id)?;
        if has_unfinished || unreadable > 0 {
            return Err("还有未结束或无法核对的制作批次，请先恢复或核对旧批次。".into());
        }
        let record = StoryMediaBatchRecord::from_preparation(
            batch_owner_hash(&persist_owner)?,
            project,
            &preparation,
            request.user_ideas,
            current_unix_milliseconds(),
        )?;
        store.save_media_batch(&persist_owner, &record)?;
        Ok(record.snapshot(&persist_owner))
    })
    .await
    .map_err(|_| "批量制作启动任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    prepared
}

#[tauri::command]
pub(crate) async fn abandon_story_media_batch(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryMediaBatchIdentityInput,
) -> Result<StoryMediaBatchSnapshot, String> {
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
    let persist_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let current = store.load_native_project(&persist_owner, &input.project_id)?;
        let mut batch =
            store.load_media_batch(&persist_owner, &input.project_id, &input.batch_id)?;
        batch.abandon(current, current_unix_milliseconds())?;
        store.commit_media_batch(&persist_owner, &mut batch)?;
        Ok(batch.snapshot(&persist_owner))
    })
    .await
    .map_err(|_| "批量制作结束任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    result
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
pub(crate) async fn update_story_segment_duration(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StorySegmentDurationCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let presentation_id = input.presentation_id;
    let change = input.change;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        work.update_segment_duration(&saved_owner, generation, &presentation_id, change)
    })
    .await
    .map_err(|_| "剧情分段时长保存任务异常退出。".to_owned())?;
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
pub(crate) async fn allow_story_image_retry(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryImageRetryCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        work.allow_story_image_retry(
            &saved_owner,
            generation,
            &input.presentation_id,
            input.target,
        )
    })
    .await
    .map_err(|_| "剧情图片重试解锁任务异常退出。".to_owned())?;
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
pub(crate) async fn generate_story_asset_image(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    media_manager: State<'_, MediaStudioManager>,
    story_manager: State<'_, StoryStudioManager>,
    input: StoryAssetImageGenerationCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let owner = input.owner_user_id;
    let presentation = input.presentation_id;
    let story = story_manager.inner().clone();
    let begin_owner = owner.clone();
    let begin_presentation = presentation.clone();
    let intent = tauri::async_runtime::spawn_blocking(move || {
        story.begin_asset_image_generation(
            &begin_owner,
            generation,
            &begin_presentation,
            input.request,
        )
    })
    .await
    .map_err(|_| "剧情素材图片生成准备任务异常退出。".to_owned())??;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    app.emit(
        "story-asset-image-generation",
        serde_json::json!({
            "presentationId": presentation,
            "project": intent.snapshot.clone()
        }),
    )
    .map_err(|_| "剧情素材图片生成状态发布失败。".to_owned())?;

    let generated = media_manager
        .generate_story_image(
            &authentication,
            &owner,
            generation,
            &intent.model_config_id,
            intent.prompt,
            &intent.attempt_id,
            Vec::new(),
        )
        .await?;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    let story = story_manager.inner().clone();
    let complete_owner = owner.clone();
    let complete_presentation = presentation.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        story.complete_asset_image_generation(
            &complete_owner,
            generation,
            &complete_presentation,
            &intent.project_id,
            &intent.resource_id,
            &intent.attempt_id,
            generated.provider_result_id,
            generated.provider_asset_id,
            &generated.bytes,
        )
    })
    .await
    .map_err(|_| "剧情素材图片保存任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    result
}

#[tauri::command]
pub(crate) async fn generate_story_frame_image(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    media_manager: State<'_, MediaStudioManager>,
    story_manager: State<'_, StoryStudioManager>,
    input: StoryFrameImageGenerationCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let owner = input.owner_user_id;
    let presentation = input.presentation_id;
    let story = story_manager.inner().clone();
    let begin_owner = owner.clone();
    let begin_presentation = presentation.clone();
    let intent = tauri::async_runtime::spawn_blocking(move || {
        story.begin_frame_image_generation(
            &begin_owner,
            generation,
            &begin_presentation,
            input.request,
        )
    })
    .await
    .map_err(|_| "剧情首尾帧生成准备任务异常退出。".to_owned())??;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    app.emit(
        "story-frame-image-generation",
        serde_json::json!({
            "presentationId": presentation,
            "project": intent.snapshot.clone()
        }),
    )
    .map_err(|_| "剧情首尾帧生成状态发布失败。".to_owned())?;
    let generated = media_manager
        .generate_story_image(
            &authentication,
            &owner,
            generation,
            &intent.model_config_id,
            intent.prompt,
            &intent.attempt_id,
            intent.references,
        )
        .await?;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    let story = story_manager.inner().clone();
    let complete_owner = owner.clone();
    let complete_presentation = presentation.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        story.complete_frame_image_generation(
            &complete_owner,
            generation,
            &complete_presentation,
            &intent.project_id,
            &intent.segment_id,
            intent.frame_role,
            &intent.attempt_id,
            generated.provider_result_id,
            generated.provider_asset_id,
            &generated.bytes,
        )
    })
    .await
    .map_err(|_| "剧情首尾帧图片保存任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    result
}

#[tauri::command]
pub(crate) async fn start_story_video_generation(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    media_manager: State<'_, MediaStudioManager>,
    story_manager: State<'_, StoryStudioManager>,
    input: StoryVideoGenerationStartCommandInput,
) -> Result<StoryVideoGenerationCommandOutcome, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let owner = input.owner_user_id;
    let presentation = input.presentation_id;
    let story = story_manager.inner().clone();
    let begin_owner = owner.clone();
    let begin_presentation = presentation.clone();
    let intent = tauri::async_runtime::spawn_blocking(move || {
        story.begin_video_generation(&begin_owner, generation, &begin_presentation, input.request)
    })
    .await
    .map_err(|_| "剧情视频生成准备任务异常退出。".to_owned())??;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    emit_story_video_event(
        &app,
        &presentation,
        &intent.snapshot,
        "submitting",
        None,
        false,
    )?;

    let submitted = media_manager
        .submit_story_video(&authentication, &owner, generation, intent.provider_request)
        .await;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    match submitted {
        Ok(job) => {
            let story = story_manager.inner().clone();
            let save_owner = owner.clone();
            let save_presentation = presentation.clone();
            let project_id = intent.project_id;
            let segment_id = intent.segment_id;
            let attempt_id = intent.attempt_id;
            let status = job.status;
            let event_status = status.clone();
            let job_id = job.job_id;
            let project = tauri::async_runtime::spawn_blocking(move || {
                story.record_video_submission(
                    &save_owner,
                    generation,
                    &save_presentation,
                    StoryVideoGenerationPollInput {
                        project_id,
                        segment_id,
                    },
                    &attempt_id,
                    &job_id,
                    &status,
                )
            })
            .await
            .map_err(|_| "剧情视频任务身份保存异常退出。".to_owned())??;
            require_current_generation(&authentication, &story_manager, &owner, generation).await?;
            emit_story_video_event(
                &app,
                &presentation,
                &project,
                &event_status,
                job.percent,
                false,
            )?;
            Ok(StoryVideoGenerationCommandOutcome {
                project,
                status: event_status,
                percent: job.percent,
                terminal: false,
            })
        }
        Err(error) => {
            let story = story_manager.inner().clone();
            let save_owner = owner.clone();
            let save_presentation = presentation.clone();
            let project_id = intent.project_id;
            let segment_id = intent.segment_id;
            let attempt_id = intent.attempt_id;
            let message = error.message;
            let event_message = message.clone();
            let request_may_have_been_submitted = error.request_may_have_been_submitted;
            let project = tauri::async_runtime::spawn_blocking(move || {
                story.record_video_failure(
                    &save_owner,
                    generation,
                    &save_presentation,
                    &project_id,
                    &segment_id,
                    &attempt_id,
                    &message,
                    !request_may_have_been_submitted,
                )
            })
            .await
            .map_err(|_| "剧情视频失败状态保存异常退出。".to_owned())??;
            emit_story_video_event(
                &app,
                &presentation,
                &project,
                if request_may_have_been_submitted {
                    "submission_unknown"
                } else {
                    "failed"
                },
                None,
                true,
            )?;
            Err(event_message)
        }
    }
}

#[tauri::command]
pub(crate) async fn poll_story_video_generation(
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    media_manager: State<'_, MediaStudioManager>,
    story_manager: State<'_, StoryStudioManager>,
    input: StoryVideoGenerationPollCommandInput,
) -> Result<StoryVideoGenerationCommandOutcome, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let owner = input.owner_user_id;
    let presentation = input.presentation_id;
    let story = story_manager.inner().clone();
    let prepare_owner = owner.clone();
    let prepare_presentation = presentation.clone();
    let intent = tauri::async_runtime::spawn_blocking(move || {
        story.prepare_video_poll(
            &prepare_owner,
            generation,
            &prepare_presentation,
            input.request,
        )
    })
    .await
    .map_err(|_| "剧情视频任务读取异常退出。".to_owned())??;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    emit_story_video_event(
        &app,
        &presentation,
        &intent.snapshot,
        "checking",
        None,
        false,
    )?;
    let polled = media_manager
        .poll_story_video(
            &authentication,
            &owner,
            generation,
            &intent.attempt.model_config_id,
            &intent.attempt.prompt,
            &intent.attempt.size,
            intent.attempt.seconds,
            &intent.attempt.ratio,
            &intent.attempt.id,
            &intent.provider_job_id,
        )
        .await;
    require_current_generation(&authentication, &story_manager, &owner, generation).await?;
    let story = story_manager.inner().clone();
    let save_owner = owner.clone();
    let save_presentation = presentation.clone();
    let project_id = intent.project_id;
    let segment_id = intent.segment_id;
    let attempt_id = intent.attempt.id;
    match polled {
        Ok(StoryVideoPollOutcome::Waiting {
            job_id,
            status,
            percent,
        }) => {
            let event_status = status.clone();
            let project = tauri::async_runtime::spawn_blocking(move || {
                story.record_video_waiting(
                    &save_owner,
                    generation,
                    &save_presentation,
                    &project_id,
                    &segment_id,
                    &attempt_id,
                    &job_id,
                    &status,
                )
            })
            .await
            .map_err(|_| "剧情视频进度保存异常退出。".to_owned())??;
            emit_story_video_event(&app, &presentation, &project, &event_status, percent, false)?;
            Ok(StoryVideoGenerationCommandOutcome {
                project,
                status: event_status,
                percent,
                terminal: false,
            })
        }
        Ok(StoryVideoPollOutcome::Completed {
            job_id,
            model_name,
            bytes,
        }) => {
            let project = tauri::async_runtime::spawn_blocking(move || {
                story.complete_video_generation(
                    &save_owner,
                    generation,
                    &save_presentation,
                    &project_id,
                    &segment_id,
                    &attempt_id,
                    job_id,
                    model_name,
                    &bytes,
                )
            })
            .await
            .map_err(|_| "剧情视频保存任务异常退出。".to_owned())??;
            emit_story_video_event(
                &app,
                &presentation,
                &project,
                "completed",
                Some(100.0),
                true,
            )?;
            Ok(StoryVideoGenerationCommandOutcome {
                project,
                status: "completed".into(),
                percent: Some(100.0),
                terminal: true,
            })
        }
        Ok(StoryVideoPollOutcome::Failed { message }) | Err(message) => {
            let project = tauri::async_runtime::spawn_blocking(move || {
                story.record_video_failure(
                    &save_owner,
                    generation,
                    &save_presentation,
                    &project_id,
                    &segment_id,
                    &attempt_id,
                    &message,
                    false,
                )
            })
            .await
            .map_err(|_| "剧情视频失败状态保存异常退出。".to_owned())??;
            emit_story_video_event(&app, &presentation, &project, "failed", None, true)?;
            Ok(StoryVideoGenerationCommandOutcome {
                project,
                status: "failed".into(),
                percent: None,
                terminal: true,
            })
        }
    }
}

#[tauri::command]
pub(crate) async fn update_story_video_guidance(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryVideoGuidanceCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let presentation = input.presentation_id;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        work.update_video_guidance(&saved_owner, generation, &presentation, input.change)
    })
    .await
    .map_err(|_| "剧情视频引导设置保存异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    saved
}

#[tauri::command]
pub(crate) async fn archive_story_completed_video(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryCompletedVideoArchiveCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let presentation = input.presentation_id;
    let saved = tauri::async_runtime::spawn_blocking(move || {
        work.archive_completed_video(&saved_owner, generation, &presentation, input.change)
    })
    .await
    .map_err(|_| "完成视频归档任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    saved
}

#[tauri::command]
pub(crate) async fn allow_story_video_retry(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, StoryStudioManager>,
    input: StoryVideoRetryCommandInput,
) -> Result<StoryProjectSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let work = manager.inner().clone();
    let owner = input.owner_user_id;
    let saved_owner = owner.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        work.allow_story_video_retry(
            &saved_owner,
            generation,
            &input.presentation_id,
            input.change,
        )
    })
    .await
    .map_err(|_| "剧情视频重试解锁任务异常退出。".to_owned())?;
    require_current_generation(&authentication, &manager, &owner, generation).await?;
    result
}

fn emit_story_video_event(
    app: &AppHandle,
    presentation_id: &str,
    project: &StoryProjectSnapshot,
    status: &str,
    percent: Option<f64>,
    terminal: bool,
) -> Result<(), String> {
    app.emit(
        "story-video-generation",
        serde_json::json!({
            "presentationId": presentation_id,
            "project": project,
            "status": status,
            "percent": percent,
            "terminal": terminal
        }),
    )
    .map_err(|_| "剧情视频生成状态发布失败。".to_owned())
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

fn current_unix_milliseconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
