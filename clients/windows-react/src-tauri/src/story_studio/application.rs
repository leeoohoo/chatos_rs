#![cfg_attr(not(test), allow(dead_code))]

use serde::Deserialize;
use uuid::Uuid;

use crate::authentication::GatewayModelConfig;

use super::{
    domain::{
        apply_segment_edit, update_resource_prompt, StoryModelSelection, StoryProject,
        StorySegmentEdit, STORY_SCHEMA_VERSION,
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
