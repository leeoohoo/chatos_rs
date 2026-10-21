#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::authentication::GatewayModelConfig;

use super::{
    domain::{StoryImageCollection, StoryProject, StorySegment, StoryVideoGuidanceMode},
    planning::project_fingerprint,
};

const MAXIMUM_BATCH_STEPS: usize = 500;
const MAXIMUM_USER_IDEAS_CHARACTERS: usize = 2_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub(crate) enum StoryMediaBatchKind {
    Assets,
    Frames,
    LastFrames,
    Videos,
    Pipeline,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum StoryMediaBatchExecutionMode {
    PersistentBatch,
    DirectVideos,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryMediaBatchStepSnapshot {
    pub id: String,
    pub kind: StoryMediaBatchKind,
    pub target_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryMediaBatchPreparation {
    pub preview_id: String,
    pub project_id: String,
    pub project_fingerprint: String,
    pub kind: StoryMediaBatchKind,
    pub eligible_target_ids: Vec<String>,
    pub requested_target_ids: Vec<String>,
    pub steps: Vec<StoryMediaBatchStepSnapshot>,
    pub execution_mode: StoryMediaBatchExecutionMode,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryMediaBatchPreviewResult {
    pub eligible_target_ids: Vec<String>,
    pub preparation: Option<StoryMediaBatchPreparation>,
    pub validation_error: Option<String>,
}

pub(crate) fn preview_media_batch(
    project: &StoryProject,
    owner_user_id: &str,
    available_models: &[GatewayModelConfig],
    input: StoryMediaBatchPreviewInput,
) -> StoryMediaBatchPreviewResult {
    let eligible_target_ids = eligible_target_ids(project, input.kind);
    let has_selection = input
        .target_ids
        .as_ref()
        .map_or(!eligible_target_ids.is_empty(), |values| !values.is_empty());
    if !has_selection {
        return StoryMediaBatchPreviewResult {
            eligible_target_ids,
            preparation: None,
            validation_error: None,
        };
    }
    match prepare_media_batch(project, owner_user_id, available_models, input) {
        Ok(preparation) => StoryMediaBatchPreviewResult {
            eligible_target_ids,
            preparation: Some(preparation),
            validation_error: None,
        },
        Err(error) => StoryMediaBatchPreviewResult {
            eligible_target_ids,
            preparation: None,
            validation_error: Some(error),
        },
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoryMediaBatchPreviewInput {
    pub project_id: String,
    pub base_fingerprint: String,
    pub kind: StoryMediaBatchKind,
    /// `None` means "select every eligible target". An explicit empty selection is rejected.
    pub target_ids: Option<Vec<String>>,
    pub user_ideas: String,
}

pub(crate) fn prepare_media_batch(
    project: &StoryProject,
    owner_user_id: &str,
    available_models: &[GatewayModelConfig],
    input: StoryMediaBatchPreviewInput,
) -> Result<StoryMediaBatchPreparation, String> {
    project.validate()?;
    let fingerprint = project_fingerprint(project)?;
    if input.project_id != project.id
        || input.base_fingerprint.is_empty()
        || input.base_fingerprint != fingerprint
    {
        return Err("剧情项目已被修改，请重新打开批量制作。".into());
    }
    validate_user_ideas(&input.user_ideas)?;

    let eligible_target_ids = eligible_target_ids(project, input.kind);
    let requested_target_ids = input
        .target_ids
        .unwrap_or_else(|| eligible_target_ids.clone());
    if requested_target_ids.is_empty()
        || requested_target_ids.len() > MAXIMUM_BATCH_STEPS
        || !all_unique(requested_target_ids.iter().map(String::as_str))
    {
        return Err("请选择至少一个可执行目标。".into());
    }
    let eligible = eligible_target_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if requested_target_ids
        .iter()
        .any(|target_id| !eligible.contains(target_id.as_str()))
    {
        return Err("所选目标状态已经变化，请刷新批量制作预览。".into());
    }

    let steps = match input.kind {
        StoryMediaBatchKind::Pipeline => pipeline_steps(project, &requested_target_ids)?,
        kind => requested_target_ids
            .iter()
            .map(|target_id| batch_step(kind, target_id))
            .collect(),
    };
    if steps.is_empty()
        || steps.len() > MAXIMUM_BATCH_STEPS
        || !all_unique(steps.iter().map(|step| step.id.as_str()))
        || steps
            .iter()
            .any(|step| step.kind == StoryMediaBatchKind::Pipeline)
    {
        return Err("批量制作步骤无效或超过 500 个，请缩小执行范围。".into());
    }
    validate_models_and_durations(project, owner_user_id, available_models, &steps)?;

    Ok(StoryMediaBatchPreparation {
        preview_id: Uuid::new_v4().to_string(),
        project_id: project.id.clone(),
        project_fingerprint: fingerprint,
        kind: input.kind,
        eligible_target_ids,
        requested_target_ids,
        steps,
        execution_mode: if input.kind == StoryMediaBatchKind::Videos {
            StoryMediaBatchExecutionMode::DirectVideos
        } else {
            StoryMediaBatchExecutionMode::PersistentBatch
        },
    })
}

pub(crate) fn eligible_target_ids(
    project: &StoryProject,
    kind: StoryMediaBatchKind,
) -> Vec<String> {
    match kind {
        StoryMediaBatchKind::Assets => resource_collections(project)
            .filter(|(_, media)| media.images.is_empty() && media.generation_attempt_id.is_none())
            .map(|(id, _)| id.to_owned())
            .collect(),
        StoryMediaBatchKind::Frames => project
            .segments
            .iter()
            .filter(|segment| {
                segment.detail.is_some()
                    && segment.attempt.is_none()
                    && segment.video.is_none()
                    && segment.first_frames.images.is_empty()
                    && segment.first_frames.generation_attempt_id.is_none()
                    && resources_are_confirmed(project, segment)
            })
            .map(|segment| segment.id.clone())
            .collect(),
        StoryMediaBatchKind::LastFrames => project
            .segments
            .iter()
            .filter(|segment| {
                segment.detail.is_some()
                    && segment.attempt.is_none()
                    && segment.video.is_none()
                    && segment.last_frames.images.is_empty()
                    && segment.last_frames.generation_attempt_id.is_none()
                    && resources_are_confirmed(project, segment)
            })
            .map(|segment| segment.id.clone())
            .collect(),
        StoryMediaBatchKind::Videos => project
            .segments
            .iter()
            .filter(|segment| segment_is_ready(segment))
            .map(|segment| segment.id.clone())
            .collect(),
        StoryMediaBatchKind::Pipeline => project
            .segments
            .iter()
            .filter(|segment| {
                segment.detail.is_some()
                    && segment.attempt.is_none()
                    && segment.video.is_none()
                    && segment.first_frames.generation_attempt_id.is_none()
                    && segment.last_frames.generation_attempt_id.is_none()
            })
            .map(|segment| segment.id.clone())
            .collect(),
    }
}

fn pipeline_steps(
    project: &StoryProject,
    requested_target_ids: &[String],
) -> Result<Vec<StoryMediaBatchStepSnapshot>, String> {
    let requested = requested_target_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let segments = requested_target_ids
        .iter()
        .map(|target_id| {
            project
                .segments
                .iter()
                .find(|segment| &segment.id == target_id)
                .ok_or_else(|| "所选剧情分段已不存在。".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let referenced_resources = segments
        .iter()
        .flat_map(|segment| segment.resource_ids())
        .map(String::as_str)
        .collect::<HashSet<_>>();

    let mut steps = Vec::new();
    for (resource_id, media) in resource_collections(project) {
        if !referenced_resources.contains(resource_id) {
            continue;
        }
        if media.generation_attempt_id.is_some() {
            return Err("关联素材仍有未核对的图片提交，请先核对原任务。".into());
        }
        if media.confirmed_image_id.is_none() {
            require_no_unconfirmed_images(media)?;
            steps.push(batch_step(StoryMediaBatchKind::Assets, resource_id));
        }
    }
    // Keep each segment's first/tail pair adjacent, matching the macOS continuity queue.
    for segment in &segments {
        if segment.first_frames.confirmed_image_id.is_none() {
            require_no_unconfirmed_images(&segment.first_frames)?;
            steps.push(batch_step(StoryMediaBatchKind::Frames, &segment.id));
        }
        if segment.video_guidance_mode == StoryVideoGuidanceMode::FirstAndLastFrames
            && segment.last_frames.confirmed_image_id.is_none()
        {
            require_no_unconfirmed_images(&segment.last_frames)?;
            steps.push(batch_step(StoryMediaBatchKind::LastFrames, &segment.id));
        }
    }
    steps.extend(
        project
            .segments
            .iter()
            .filter(|segment| requested.contains(segment.id.as_str()))
            .map(|segment| batch_step(StoryMediaBatchKind::Videos, &segment.id)),
    );
    Ok(steps)
}

fn validate_models_and_durations(
    project: &StoryProject,
    owner_user_id: &str,
    available_models: &[GatewayModelConfig],
    steps: &[StoryMediaBatchStepSnapshot],
) -> Result<(), String> {
    if steps
        .iter()
        .any(|step| step.kind != StoryMediaBatchKind::Videos)
    {
        require_eligible_model(
            &project.models.image_model_id,
            owner_user_id,
            available_models,
        )?;
    }
    if steps
        .iter()
        .any(|step| step.kind == StoryMediaBatchKind::Videos)
    {
        require_eligible_model(
            &project.models.video_model_id,
            owner_user_id,
            available_models,
        )?;
        if let Some(durations) = &project.models.supported_video_durations {
            let unsupported = steps.iter().any(|step| {
                step.kind == StoryMediaBatchKind::Videos
                    && project
                        .segments
                        .iter()
                        .find(|segment| segment.id == step.target_id)
                        .is_none_or(|segment| !durations.contains(&segment.seconds))
            });
            if unsupported {
                return Err("所选分段时长不受当前视频模型支持，请先调整时长。".into());
            }
        }
    }
    Ok(())
}

fn require_eligible_model(
    model_id: &str,
    owner_user_id: &str,
    available_models: &[GatewayModelConfig],
) -> Result<(), String> {
    available_models
        .iter()
        .any(|model| {
            model.id == model_id
                && model.owner_user_id == owner_user_id
                && model.enabled
                && model.has_api_key
        })
        .then_some(())
        .ok_or_else(|| "所选模型已不可用，请刷新模型列表并重新选择。".into())
}

fn validate_user_ideas(value: &str) -> Result<(), String> {
    if value.chars().count() > MAXIMUM_USER_IDEAS_CHARACTERS
        || value
            .chars()
            .any(|character| matches!(character as u32, 0..=8 | 11 | 12 | 14..=31 | 127))
    {
        return Err("批量制作补充要求无效。".into());
    }
    Ok(())
}

fn require_no_unconfirmed_images(media: &StoryImageCollection) -> Result<(), String> {
    if media.images.is_empty() {
        Ok(())
    } else {
        Err("已有图片尚未确认版本。请先确认已有素材或首尾帧；一键执行不会替你选择已有版本。".into())
    }
}

fn resource_collections(
    project: &StoryProject,
) -> impl Iterator<Item = (&str, &StoryImageCollection)> {
    project
        .characters
        .iter()
        .map(|resource| (resource.id.as_str(), &resource.media))
        .chain(
            project
                .scenes
                .iter()
                .map(|resource| (resource.id.as_str(), &resource.media)),
        )
        .chain(
            project
                .props
                .iter()
                .map(|resource| (resource.id.as_str(), &resource.media)),
        )
}

fn resources_are_confirmed(project: &StoryProject, segment: &StorySegment) -> bool {
    segment.resource_ids().all(|resource_id| {
        project
            .resource_media(resource_id)
            .is_some_and(|media| media.confirmed_image_id.is_some())
    })
}

fn segment_is_ready(segment: &StorySegment) -> bool {
    segment.detail.is_some()
        && segment.first_frames.confirmed_image_id.is_some()
        && segment.attempt.is_none()
        && segment.video.is_none()
}

fn batch_step(kind: StoryMediaBatchKind, target_id: &str) -> StoryMediaBatchStepSnapshot {
    let prefix = match kind {
        StoryMediaBatchKind::Assets => "assets",
        StoryMediaBatchKind::Frames => "frames",
        StoryMediaBatchKind::LastFrames => "lastFrames",
        StoryMediaBatchKind::Videos => "videos",
        StoryMediaBatchKind::Pipeline => "pipeline",
    };
    StoryMediaBatchStepSnapshot {
        id: format!("{prefix}:{target_id}"),
        kind,
        target_id: target_id.to_owned(),
    }
}

fn all_unique<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = HashSet::new();
    values.into_iter().all(|value| seen.insert(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story_studio::domain::{
        StoryImage, StoryModelSelection, StoryProp, StorySegmentDetail, StorySegmentKind,
        StoryShot, StorySourceRange, STORY_SCHEMA_VERSION,
    };

    #[test]
    fn pipeline_preserves_native_asset_frame_tail_video_order() {
        let mut project = project();
        project.segments[0].detail = Some(detail());
        project.segments[0].video_guidance_mode = StoryVideoGuidanceMode::FirstAndLastFrames;
        let fingerprint = project_fingerprint(&project).unwrap();

        let preview = prepare_media_batch(
            &project,
            "alice",
            &models(),
            StoryMediaBatchPreviewInput {
                project_id: project.id.clone(),
                base_fingerprint: fingerprint,
                kind: StoryMediaBatchKind::Pipeline,
                target_ids: None,
                user_ideas: "暖色调".into(),
            },
        )
        .unwrap();

        assert_eq!(
            preview
                .steps
                .iter()
                .map(|step| (step.kind, step.target_id.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (StoryMediaBatchKind::Assets, "key"),
                (StoryMediaBatchKind::Frames, "s1"),
                (StoryMediaBatchKind::LastFrames, "s1"),
                (StoryMediaBatchKind::Videos, "s1"),
            ]
        );
        assert_eq!(
            preview.execution_mode,
            StoryMediaBatchExecutionMode::PersistentBatch
        );
    }

    #[test]
    fn pipeline_never_chooses_an_existing_unconfirmed_image() {
        let mut project = project();
        project.segments[0].detail = Some(detail());
        project.props[0]
            .media
            .images
            .push(StoryImage::test_image("asset.png"));
        let fingerprint = project_fingerprint(&project).unwrap();

        let error = prepare_media_batch(
            &project,
            "alice",
            &models(),
            StoryMediaBatchPreviewInput {
                project_id: project.id.clone(),
                base_fingerprint: fingerprint,
                kind: StoryMediaBatchKind::Pipeline,
                target_ids: None,
                user_ideas: String::new(),
            },
        )
        .unwrap_err();

        assert!(error.contains("尚未确认版本"));
    }

    #[test]
    fn video_preview_uses_the_direct_queue_and_checks_duration() {
        let mut project = project();
        project.segments[0].detail = Some(detail());
        confirm_image(&mut project.segments[0].first_frames, "first.png");
        let fingerprint = project_fingerprint(&project).unwrap();
        let preview = prepare_media_batch(
            &project,
            "alice",
            &models(),
            StoryMediaBatchPreviewInput {
                project_id: project.id.clone(),
                base_fingerprint: fingerprint,
                kind: StoryMediaBatchKind::Videos,
                target_ids: None,
                user_ideas: String::new(),
            },
        )
        .unwrap();
        assert_eq!(preview.steps.len(), 1);
        assert_eq!(
            preview.execution_mode,
            StoryMediaBatchExecutionMode::DirectVideos
        );

        project.models.supported_video_durations = Some(vec![5, 10]);
        let error = prepare_media_batch(
            &project,
            "alice",
            &models(),
            StoryMediaBatchPreviewInput {
                project_id: project.id.clone(),
                base_fingerprint: project_fingerprint(&project).unwrap(),
                kind: StoryMediaBatchKind::Videos,
                target_ids: None,
                user_ideas: String::new(),
            },
        )
        .unwrap_err();
        assert!(error.contains("时长"));
    }

    fn confirm_image(collection: &mut StoryImageCollection, filename: &str) {
        let image = StoryImage::test_image(filename);
        collection.confirmed_image_id = Some(image.id.clone());
        collection.images.push(image);
    }

    fn project() -> StoryProject {
        StoryProject {
            id: Uuid::new_v4().to_string(),
            version: STORY_SCHEMA_VERSION,
            title: "Story".into(),
            description: String::new(),
            models: StoryModelSelection {
                text_model_id: "text".into(),
                image_model_id: "image".into(),
                video_model_id: "video".into(),
                supported_video_durations: Some(vec![5, 10, 15]),
            },
            source: "x".into(),
            style: "cinematic".into(),
            ratio: "16:9".into(),
            summary: String::new(),
            characters: vec![],
            scenes: vec![],
            props: vec![StoryProp {
                id: "key".into(),
                name: "Key".into(),
                description: "An old key".into(),
                image_prompt: "An old brass key".into(),
                media: StoryImageCollection::default(),
            }],
            segments: vec![StorySegment {
                id: "s1".into(),
                title: "Segment".into(),
                synopsis: "Segment".into(),
                kind: StorySegmentKind::Story,
                seconds: 15,
                source_range: StorySourceRange { start: 0, end: 1 },
                character_ids: vec![],
                scene_ids: vec![],
                prop_ids: vec!["key".into()],
                detail: None,
                first_frames: StoryImageCollection::default(),
                user_selected_first_frame_id: None,
                inherited_first_frame_source_segment_id: None,
                last_frames: StoryImageCollection::default(),
                actual_video_last_frame_id: None,
                video_guidance_mode: StoryVideoGuidanceMode::FirstFrame,
                attempt: None,
                previous_attempts: vec![],
                video: None,
                archived_videos: vec![],
                error: None,
            }],
            relations: vec![],
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        }
    }

    fn detail() -> StorySegmentDetail {
        StorySegmentDetail {
            first_frame_prompt: "first frame".into(),
            last_frame_prompt: Some("last frame".into()),
            shots: vec![StoryShot {
                start: 0,
                end: 15,
                prompt: "continuous shot".into(),
            }],
            continuity_in: String::new(),
            continuity_out: String::new(),
            audio: String::new(),
            constraints: String::new(),
        }
    }

    fn models() -> Vec<GatewayModelConfig> {
        [("image", "gpt-image-1"), ("video", "MiniMax-H3")]
            .into_iter()
            .map(|(id, model)| GatewayModelConfig {
                id: id.into(),
                owner_user_id: "alice".into(),
                name: id.into(),
                provider: "openai".into(),
                model: model.into(),
                api_key: Some("native-only".into()),
                base_url: None,
                task_thinking_level: None,
                temperature: None,
                max_output_tokens: None,
                enabled: true,
                task_enabled: Some(false),
                has_api_key: true,
                supports_images: id == "image",
                supports_reasoning: false,
                supports_responses: false,
            })
            .collect()
    }
}
