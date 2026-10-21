#![cfg_attr(not(test), allow(dead_code))]

use serde::Deserialize;
use uuid::Uuid;

use crate::authentication::GatewayModelConfig;

use super::{
    domain::{
        apply_segment_edit, attach_imported_image, clear_confirmed_last_frame, confirm_image,
        confirm_latest_frames, confirm_latest_resource_images, update_resource_prompt,
        StoryFrameRole, StoryImage, StoryImageCollection, StoryImageTarget, StoryModelSelection,
        StoryProject, StorySegment, StorySegmentEdit, StorySegmentKind, StorySourceRange,
        StoryVideoGuidanceMode, STORY_SCHEMA_VERSION,
    },
    planning::project_fingerprint,
};

const DEFAULT_STORY_STYLE: &str = "自然光，电影感，保持角色外观、服装与场景一致";
const DEFAULT_STORY_RATIO: &str = "16:9";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryModelSelectionInput {
    pub text_model_id: String,
    pub image_model_id: String,
    pub video_model_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryProjectCreationInput {
    pub title: String,
    pub description: String,
    pub models: StoryModelSelectionInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryProjectSettingsInput {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub models: StoryModelSelectionInput,
}

/// Narrow source-editor intent. React never submits a persisted StoryProject, so native-only
/// media/provider identities and schema metadata remain owned by the canonical Rust project.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryProjectSourceInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub source: String,
    pub style: String,
    pub ratio: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryResourcePromptInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub resource_id: String,
    pub prompt: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StorySegmentEditInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub segment: StorySegmentEdit,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StorySegmentStructureOperation {
    Add,
    Delete,
    Move,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StorySegmentStructureMutation {
    pub operation: StorySegmentStructureOperation,
    pub segment_id: Option<String>,
    pub offset: Option<i8>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StorySegmentStructureInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub mutation: StorySegmentStructureMutation,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StoryFrameRoleInput {
    First,
    Last,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryImageConfirmationInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub resource_id: Option<String>,
    pub segment_id: Option<String>,
    pub frame_role: Option<StoryFrameRoleInput>,
    pub image_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryLastFrameClearInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub segment_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryLatestResourceImagesInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub resource_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryLatestFramesInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub segment_id: String,
    pub use_confirmed_last_frame_for_video: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryImageImportInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub resource_id: Option<String>,
    pub segment_id: Option<String>,
    pub frame_role: Option<StoryFrameRoleInput>,
    pub confirms_imported_image: bool,
}

pub(super) fn make_story_project(
    input: StoryProjectCreationInput,
    owner_user_id: &str,
    available_models: &[GatewayModelConfig],
    now_unix_ms: u64,
) -> Result<StoryProject, String> {
    let project = StoryProject {
        id: Uuid::new_v4().to_string(),
        version: STORY_SCHEMA_VERSION,
        title: input.title.trim().to_owned(),
        description: input.description.trim().to_owned(),
        models: validated_model_selection(input.models, owner_user_id, available_models)?,
        source: String::new(),
        style: DEFAULT_STORY_STYLE.into(),
        ratio: DEFAULT_STORY_RATIO.into(),
        summary: String::new(),
        characters: Vec::new(),
        scenes: Vec::new(),
        props: Vec::new(),
        segments: Vec::new(),
        relations: Vec::new(),
        created_at_unix_ms: now_unix_ms,
        updated_at_unix_ms: now_unix_ms,
    };
    project.validate()?;
    Ok(project)
}

pub(super) fn apply_story_project_settings(
    project: &mut StoryProject,
    input: StoryProjectSettingsInput,
    owner_user_id: &str,
    available_models: &[GatewayModelConfig],
    now_unix_ms: u64,
) -> Result<(), String> {
    if input.project_id != project.id {
        return Err("剧情设置已过期，请重新打开后再保存。".into());
    }
    let models = validated_model_selection(input.models, owner_user_id, available_models)?;
    if (has_unresolved_video(project) && models.video_model_id != project.models.video_model_id)
        || (has_unresolved_image(project) && models.image_model_id != project.models.image_model_id)
    {
        return Err("上次提交结果尚未确认，请先核对服务商任务记录。".into());
    }
    project.title = input.title.trim().to_owned();
    project.description = input.description;
    project.models = models;
    project.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    project.validate()
}

pub(super) fn apply_story_project_source(
    project: &mut StoryProject,
    input: StoryProjectSourceInput,
    now_unix_ms: u64,
) -> Result<(), String> {
    if input.project_id != project.id
        || input.base_fingerprint.is_empty()
        || project_fingerprint(project)? != input.base_fingerprint
    {
        return Err("剧情项目已被修改，请重新打开后再保存。".into());
    }
    // Matches macOS StoryStudioViewModel.saveSource: source metadata is frozen once an outline
    // exists, because changing character offsets would invalidate every segment source range.
    if !project.segments.is_empty() {
        return Err("剧情已经生成分段，不能再修改原文、风格或画面比例。".into());
    }
    project.source = input.source;
    project.style = input.style;
    project.ratio = input.ratio;
    project.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    project.validate()
}

pub(super) fn apply_story_resource_prompt(
    project: &mut StoryProject,
    input: StoryResourcePromptInput,
    now_unix_ms: u64,
) -> Result<(), String> {
    if input.project_id != project.id
        || input.base_fingerprint.is_empty()
        || project_fingerprint(project)? != input.base_fingerprint
    {
        return Err("剧情项目已被修改，请重新打开后再保存。".into());
    }
    let mut next = update_resource_prompt(project, &input.resource_id, input.prompt)?;
    next.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    *project = next;
    Ok(())
}

pub(super) fn apply_story_segment_edit(
    project: &mut StoryProject,
    input: StorySegmentEditInput,
    now_unix_ms: u64,
) -> Result<(), String> {
    if input.project_id != project.id
        || input.base_fingerprint.is_empty()
        || project_fingerprint(project)? != input.base_fingerprint
    {
        return Err("剧情项目已被修改，请重新打开后再保存。".into());
    }
    let mut next = apply_segment_edit(project, input.segment)?;
    next.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    *project = next;
    Ok(())
}

pub(super) fn apply_story_segment_structure(
    project: &mut StoryProject,
    input: StorySegmentStructureInput,
    now_unix_ms: u64,
) -> Result<(), String> {
    if input.project_id != project.id
        || input.base_fingerprint.is_empty()
        || project_fingerprint(project)? != input.base_fingerprint
    {
        return Err("剧情项目已被修改，请重新打开后再保存。".into());
    }
    let has_unresolved_jobs = project
        .characters
        .iter()
        .map(|value| &value.media)
        .chain(project.scenes.iter().map(|value| &value.media))
        .chain(project.props.iter().map(|value| &value.media))
        .any(|media| media.generation_attempt_id.is_some())
        || project.segments.iter().any(|segment| {
            segment.attempt.is_some()
                || segment.first_frames.generation_attempt_id.is_some()
                || segment.last_frames.generation_attempt_id.is_some()
        });
    if has_unresolved_jobs
        || project
            .segments
            .iter()
            .any(|segment| segment.video.is_some())
    {
        return Err("存在进行中的媒体任务或已完成视频，不能调整剧情结构。".into());
    }
    match input.mutation.operation {
        StorySegmentStructureOperation::Add => {
            if input.mutation.segment_id.is_some() || input.mutation.offset.is_some() {
                return Err("新增剧情分段参数无效。".into());
            }
            let end = project.source.chars().count();
            if end == 0 {
                return Err("请先填写剧情原文。".into());
            }
            project.segments.push(StorySegment {
                id: Uuid::new_v4().to_string(),
                title: "新分段".into(),
                synopsis: "请编辑本段剧情与镜头计划".into(),
                kind: StorySegmentKind::Story,
                seconds: 15,
                source_range: StorySourceRange {
                    start: end.saturating_sub(1),
                    end,
                },
                character_ids: Vec::new(),
                scene_ids: Vec::new(),
                prop_ids: Vec::new(),
                detail: None,
                first_frames: StoryImageCollection::default(),
                user_selected_first_frame_id: None,
                inherited_first_frame_source_segment_id: None,
                last_frames: StoryImageCollection::default(),
                actual_video_last_frame_id: None,
                video_guidance_mode: StoryVideoGuidanceMode::FirstFrame,
                attempt: None,
                previous_attempts: Vec::new(),
                video: None,
                archived_videos: Vec::new(),
                error: None,
            });
        }
        StorySegmentStructureOperation::Delete => {
            let segment_id = input
                .mutation
                .segment_id
                .filter(|_| input.mutation.offset.is_none())
                .ok_or_else(|| "删除剧情分段参数无效。".to_owned())?;
            let count = project.segments.len();
            project.segments.retain(|segment| segment.id != segment_id);
            if count == project.segments.len() {
                return Err("剧情分段已经变化，请重新打开后再试。".into());
            }
            project
                .relations
                .retain(|relation| relation.segment_id != segment_id);
        }
        StorySegmentStructureOperation::Move => {
            let segment_id = input
                .mutation
                .segment_id
                .ok_or_else(|| "剧情分段移动参数无效。".to_owned())?;
            let offset = input
                .mutation
                .offset
                .ok_or_else(|| "剧情分段移动参数无效。".to_owned())?;
            if !matches!(offset, -1 | 1) {
                return Err("剧情分段移动参数无效。".into());
            }
            let index = project
                .segments
                .iter()
                .position(|segment| segment.id == segment_id)
                .ok_or_else(|| "剧情分段已经变化，请重新打开后再试。".to_owned())?;
            let target = index as isize + offset as isize;
            if target < 0 || target >= project.segments.len() as isize {
                return Err("剧情分段无法继续移动。".into());
            }
            project.segments.swap(index, target as usize);
        }
    }
    for segment in &mut project.segments {
        segment.detail = None;
        segment.first_frames.confirmed_image_id = None;
        segment.last_frames.confirmed_image_id = None;
    }
    project.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    project.validate()
}

pub(super) fn apply_story_image_confirmation(
    project: &mut StoryProject,
    input: StoryImageConfirmationInput,
    now_unix_ms: u64,
) -> Result<(), String> {
    require_story_fingerprint(project, &input.project_id, &input.base_fingerprint)?;
    let target = match (input.resource_id, input.segment_id, input.frame_role) {
        (Some(resource_id), None, None) => StoryImageTarget::Resource { resource_id },
        (None, Some(segment_id), Some(role)) => StoryImageTarget::Frame {
            segment_id,
            role: match role {
                StoryFrameRoleInput::First => StoryFrameRole::First,
                StoryFrameRoleInput::Last => StoryFrameRole::Last,
            },
        },
        _ => return Err("剧情图片确认目标无效。".into()),
    };
    let mut next = confirm_image(project, &target, &input.image_id)?
        .ok_or_else(|| "剧情图片确认状态没有变化。".to_owned())?;
    next.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    *project = next;
    Ok(())
}

pub(super) fn apply_story_last_frame_clear(
    project: &mut StoryProject,
    input: StoryLastFrameClearInput,
    now_unix_ms: u64,
) -> Result<(), String> {
    require_story_fingerprint(project, &input.project_id, &input.base_fingerprint)?;
    let mut next = clear_confirmed_last_frame(project, &input.segment_id)?
        .ok_or_else(|| "剧情尾帧确认状态没有变化。".to_owned())?;
    next.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    *project = next;
    Ok(())
}

fn require_story_fingerprint(
    project: &StoryProject,
    project_id: &str,
    base_fingerprint: &str,
) -> Result<(), String> {
    if project_id != project.id
        || base_fingerprint.is_empty()
        || project_fingerprint(project)? != base_fingerprint
    {
        Err("剧情项目已被修改，请重新打开后再保存。".into())
    } else {
        Ok(())
    }
}

pub(super) fn apply_story_latest_resource_images(
    project: &mut StoryProject,
    input: StoryLatestResourceImagesInput,
    now_unix_ms: u64,
) -> Result<(), String> {
    require_story_fingerprint(project, &input.project_id, &input.base_fingerprint)?;
    let mut next = confirm_latest_resource_images(project, &input.resource_ids)?
        .ok_or_else(|| "剧情素材确认状态没有变化。".to_owned())?;
    next.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    *project = next;
    Ok(())
}

pub(super) fn apply_story_latest_frames(
    project: &mut StoryProject,
    input: StoryLatestFramesInput,
    now_unix_ms: u64,
) -> Result<(), String> {
    require_story_fingerprint(project, &input.project_id, &input.base_fingerprint)?;
    let mut next = confirm_latest_frames(
        project,
        &input.segment_id,
        input.use_confirmed_last_frame_for_video,
    )?
    .ok_or_else(|| "剧情帧确认状态没有变化。".to_owned())?;
    next.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    *project = next;
    Ok(())
}

pub(super) fn apply_story_image_import(
    project: &mut StoryProject,
    input: StoryImageImportInput,
    filename: String,
    now_unix_ms: u64,
) -> Result<(), String> {
    require_story_fingerprint(project, &input.project_id, &input.base_fingerprint)?;
    let target = match (input.resource_id, input.segment_id, input.frame_role) {
        (Some(resource_id), None, None) => StoryImageTarget::Resource { resource_id },
        (None, Some(segment_id), Some(role)) => StoryImageTarget::Frame {
            segment_id,
            role: match role {
                StoryFrameRoleInput::First => StoryFrameRole::First,
                StoryFrameRoleInput::Last => StoryFrameRole::Last,
            },
        },
        _ => return Err("剧情图片导入目标无效。".into()),
    };
    let image = StoryImage {
        id: Uuid::new_v4().to_string(),
        filename,
        mime_type: "image/png".into(),
        source_resource_id: None,
        generation_attempt_id: None,
        provider_result_id: None,
        provider_asset_id: None,
        derived_from_video_job_id: None,
    };
    let mut next = attach_imported_image(project, &target, image, input.confirms_imported_image)?;
    next.updated_at_unix_ms = now_unix_ms.max(project.updated_at_unix_ms);
    *project = next;
    Ok(())
}

fn validated_model_selection(
    input: StoryModelSelectionInput,
    owner_user_id: &str,
    available_models: &[GatewayModelConfig],
) -> Result<StoryModelSelection, String> {
    eligible_model(&input.text_model_id, owner_user_id, available_models)?;
    eligible_model(&input.image_model_id, owner_user_id, available_models)?;
    let video = eligible_model(&input.video_model_id, owner_user_id, available_models)?;
    Ok(StoryModelSelection {
        text_model_id: input.text_model_id,
        image_model_id: input.image_model_id,
        video_model_id: input.video_model_id,
        supported_video_durations: Some(video_durations(&video.model)),
    })
}

fn eligible_model<'a>(
    model_id: &str,
    owner_user_id: &str,
    available_models: &'a [GatewayModelConfig],
) -> Result<&'a GatewayModelConfig, String> {
    available_models
        .iter()
        .find(|model| {
            model.id == model_id
                && model.owner_user_id == owner_user_id
                && model.enabled
                && model.has_api_key
        })
        .ok_or_else(|| "所选模型已不可用，请刷新模型列表并重新选择。".into())
}

fn video_durations(model_name: &str) -> Vec<u8> {
    let value = model_name.trim().to_ascii_lowercase();
    match value.as_str() {
        "minimax-h3" => (4..=15).collect(),
        "minimax-h3-max" => (5..=15).collect(),
        _ if value.contains("seedance-2-5") || value.contains("seedance-2.5") => (4..=30).collect(),
        _ if value.contains("seedance-2-0") || value.contains("seedance-2.0") => (4..=15).collect(),
        _ => vec![4, 8, 12],
    }
}

fn has_unresolved_video(project: &StoryProject) -> bool {
    project
        .segments
        .iter()
        .any(|segment| segment.attempt.is_some() && segment.video.is_none())
}

fn has_unresolved_image(project: &StoryProject) -> bool {
    project
        .characters
        .iter()
        .any(|value| value.media.generation_attempt_id.is_some())
        || project
            .scenes
            .iter()
            .any(|value| value.media.generation_attempt_id.is_some())
        || project
            .props
            .iter()
            .any(|value| value.media.generation_attempt_id.is_some())
        || project.segments.iter().any(|value| {
            value.first_frames.generation_attempt_id.is_some()
                || value.last_frames.generation_attempt_id.is_some()
        })
}

#[cfg(test)]
pub(super) fn default_story_style() -> &'static str {
    DEFAULT_STORY_STYLE
}
