#![cfg_attr(not(test), allow(dead_code))]

use serde::Serialize;

use super::domain::{
    StoryCharacter, StoryCharacterProfile, StoryImageCollection, StoryModelSelection, StoryProject,
    StoryProp, StoryScene, StorySceneProfile, StorySegment, StorySegmentDetail, StorySegmentKind,
    StorySegmentRelation, StorySourceRange, StoryVideo, StoryVideoAttempt, StoryVideoGuidanceMode,
};

/// IPC-safe projection of the native Story project. It deliberately has no `Deserialize`
/// implementation: React must never submit a persisted project shape back to the native layer.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryProjectSnapshot {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_fingerprint: Option<String>,
    version: u32,
    title: String,
    description: String,
    models: StoryModelSelectionSnapshot,
    source: String,
    style: String,
    ratio: String,
    summary: String,
    characters: Vec<StoryCharacterSnapshot>,
    scenes: Vec<StorySceneSnapshot>,
    props: Vec<StoryPropSnapshot>,
    segments: Vec<StorySegmentSnapshot>,
    relations: Vec<StorySegmentRelationSnapshot>,
    created_at_unix_ms: u64,
    updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StoryMediaPresentation {
    pub content_token: String,
    pub preview_url: String,
}

impl StoryProjectSnapshot {
    pub(super) fn from_project(project: &StoryProject) -> Self {
        Self::from_project_with_media(project, None, |_| None)
    }

    pub(super) fn from_project_with_media(
        project: &StoryProject,
        base_fingerprint: Option<String>,
        media: impl Fn(&str) -> Option<StoryMediaPresentation>,
    ) -> Self {
        Self {
            id: project.id.clone(),
            base_fingerprint,
            version: project.version,
            title: project.title.clone(),
            description: project.description.clone(),
            models: StoryModelSelectionSnapshot::from(&project.models),
            source: project.source.clone(),
            style: project.style.clone(),
            ratio: project.ratio.clone(),
            summary: project.summary.clone(),
            characters: project
                .characters
                .iter()
                .map(|value| StoryCharacterSnapshot::from_character(value, &media))
                .collect(),
            scenes: project
                .scenes
                .iter()
                .map(|value| StorySceneSnapshot::from_scene(value, &media))
                .collect(),
            props: project
                .props
                .iter()
                .map(|value| StoryPropSnapshot::from_prop(value, &media))
                .collect(),
            segments: project
                .segments
                .iter()
                .map(|value| StorySegmentSnapshot::from_segment(value, &media))
                .collect(),
            relations: project
                .relations
                .iter()
                .map(StorySegmentRelationSnapshot::from)
                .collect(),
            created_at_unix_ms: project.created_at_unix_ms,
            updated_at_unix_ms: project.updated_at_unix_ms,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoryModelSelectionSnapshot {
    text_model_id: String,
    image_model_id: String,
    video_model_id: String,
    supported_video_durations: Option<Vec<u8>>,
}

impl From<&StoryModelSelection> for StoryModelSelectionSnapshot {
    fn from(value: &StoryModelSelection) -> Self {
        Self {
            text_model_id: value.text_model_id.clone(),
            image_model_id: value.image_model_id.clone(),
            video_model_id: value.video_model_id.clone(),
            supported_video_durations: value.supported_video_durations.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoryImageSnapshot {
    id: String,
    mime_type: String,
    /// Filled only by the future owner/session-scoped media vault.
    content_token: Option<String>,
    /// Filled only by the future controlled preview protocol.
    preview_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoryImageCollectionSnapshot {
    images: Vec<StoryImageSnapshot>,
    confirmed_image_id: Option<String>,
    generation_attempt_id: Option<String>,
}

impl From<&StoryImageCollection> for StoryImageCollectionSnapshot {
    fn from(value: &StoryImageCollection) -> Self {
        Self::from_collection(value, &|_| None)
    }
}

impl StoryImageCollectionSnapshot {
    fn from_collection(
        value: &StoryImageCollection,
        media: &impl Fn(&str) -> Option<StoryMediaPresentation>,
    ) -> Self {
        let image_snapshot = |image: &super::domain::StoryImage| {
            let presentation = media(&image.filename);
            StoryImageSnapshot {
                id: image.id.clone(),
                mime_type: image.mime_type.clone(),
                content_token: presentation
                    .as_ref()
                    .map(|value| value.content_token.clone()),
                preview_url: presentation.map(|value| value.preview_url),
            }
        };
        Self {
            images: value.images.iter().map(image_snapshot).collect(),
            confirmed_image_id: value.confirmed_image_id.clone(),
            generation_attempt_id: value.generation_attempt_id.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoryCharacterProfileSnapshot {
    is_protagonist: bool,
    role_in_story: String,
    appearance: String,
    personality: String,
    motivation: String,
    relationships: String,
    costume: String,
    consistency_notes: String,
}

impl From<&StoryCharacterProfile> for StoryCharacterProfileSnapshot {
    fn from(value: &StoryCharacterProfile) -> Self {
        Self {
            is_protagonist: value.is_protagonist,
            role_in_story: value.role_in_story.clone(),
            appearance: value.appearance.clone(),
            personality: value.personality.clone(),
            motivation: value.motivation.clone(),
            relationships: value.relationships.clone(),
            costume: value.costume.clone(),
            consistency_notes: value.consistency_notes.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StorySceneProfileSnapshot {
    role_in_story: String,
    setting: String,
    spatial_layout: String,
    lighting_and_palette: String,
    key_elements: String,
    atmosphere: String,
    consistency_notes: String,
}

impl From<&StorySceneProfile> for StorySceneProfileSnapshot {
    fn from(value: &StorySceneProfile) -> Self {
        Self {
            role_in_story: value.role_in_story.clone(),
            setting: value.setting.clone(),
            spatial_layout: value.spatial_layout.clone(),
            lighting_and_palette: value.lighting_and_palette.clone(),
            key_elements: value.key_elements.clone(),
            atmosphere: value.atmosphere.clone(),
            consistency_notes: value.consistency_notes.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoryCharacterSnapshot {
    id: String,
    name: String,
    profile: StoryCharacterProfileSnapshot,
    image_prompt: String,
    media: StoryImageCollectionSnapshot,
}

impl StoryCharacterSnapshot {
    fn from_character(
        value: &StoryCharacter,
        media: &impl Fn(&str) -> Option<StoryMediaPresentation>,
    ) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            profile: StoryCharacterProfileSnapshot::from(&value.profile),
            image_prompt: value.image_prompt.clone(),
            media: StoryImageCollectionSnapshot::from_collection(&value.media, media),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StorySceneSnapshot {
    id: String,
    name: String,
    profile: StorySceneProfileSnapshot,
    image_prompt: String,
    media: StoryImageCollectionSnapshot,
}

impl StorySceneSnapshot {
    fn from_scene(
        value: &StoryScene,
        media: &impl Fn(&str) -> Option<StoryMediaPresentation>,
    ) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            profile: StorySceneProfileSnapshot::from(&value.profile),
            image_prompt: value.image_prompt.clone(),
            media: StoryImageCollectionSnapshot::from_collection(&value.media, media),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoryPropSnapshot {
    id: String,
    name: String,
    description: String,
    image_prompt: String,
    media: StoryImageCollectionSnapshot,
}

impl StoryPropSnapshot {
    fn from_prop(
        value: &StoryProp,
        media: &impl Fn(&str) -> Option<StoryMediaPresentation>,
    ) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            description: value.description.clone(),
            image_prompt: value.image_prompt.clone(),
            media: StoryImageCollectionSnapshot::from_collection(&value.media, media),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StorySourceRangeSnapshot {
    start: usize,
    end: usize,
}

impl From<&StorySourceRange> for StorySourceRangeSnapshot {
    fn from(value: &StorySourceRange) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoryShotSnapshot {
    start: u8,
    end: u8,
    prompt: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StorySegmentDetailSnapshot {
    first_frame_prompt: String,
    last_frame_prompt: Option<String>,
    shots: Vec<StoryShotSnapshot>,
    continuity_in: String,
    continuity_out: String,
    audio: String,
    constraints: String,
}

impl From<&StorySegmentDetail> for StorySegmentDetailSnapshot {
    fn from(value: &StorySegmentDetail) -> Self {
        Self {
            first_frame_prompt: value.first_frame_prompt.clone(),
            last_frame_prompt: value.last_frame_prompt.clone(),
            shots: value
                .shots
                .iter()
                .map(|shot| StoryShotSnapshot {
                    start: shot.start,
                    end: shot.end,
                    prompt: shot.prompt.clone(),
                })
                .collect(),
            continuity_in: value.continuity_in.clone(),
            continuity_out: value.continuity_out.clone(),
            audio: value.audio.clone(),
            constraints: value.constraints.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoryVideoAttemptSnapshot {
    id: String,
    model_config_id: String,
    prompt: String,
    size: String,
    ratio: String,
    seconds: u8,
    status: String,
    created_at_unix_ms: u64,
}

impl From<&StoryVideoAttempt> for StoryVideoAttemptSnapshot {
    fn from(value: &StoryVideoAttempt) -> Self {
        Self {
            id: value.id.clone(),
            model_config_id: value.model_config_id.clone(),
            prompt: value.prompt.clone(),
            size: value.size.clone(),
            ratio: value.ratio.clone(),
            seconds: value.seconds,
            status: value.status.clone(),
            created_at_unix_ms: value.created_at_unix_ms,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoryVideoSnapshot {
    model_name: String,
    content_token: Option<String>,
    preview_url: Option<String>,
}

impl StoryVideoSnapshot {
    fn from_video(
        value: &StoryVideo,
        media: &impl Fn(&str) -> Option<StoryMediaPresentation>,
    ) -> Self {
        let presentation = media(&value.filename);
        Self {
            model_name: value.model_name.clone(),
            content_token: presentation
                .as_ref()
                .map(|value| value.content_token.clone()),
            preview_url: presentation.map(|value| value.preview_url),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StorySegmentSnapshot {
    id: String,
    title: String,
    synopsis: String,
    kind: String,
    seconds: u8,
    source_range: StorySourceRangeSnapshot,
    character_ids: Vec<String>,
    scene_ids: Vec<String>,
    prop_ids: Vec<String>,
    detail: Option<StorySegmentDetailSnapshot>,
    first_frames: StoryImageCollectionSnapshot,
    user_selected_first_frame_id: Option<String>,
    inherited_first_frame_source_segment_id: Option<String>,
    last_frames: StoryImageCollectionSnapshot,
    actual_video_last_frame_id: Option<String>,
    video_guidance_mode: String,
    attempt: Option<StoryVideoAttemptSnapshot>,
    previous_attempts: Vec<StoryVideoAttemptSnapshot>,
    video: Option<StoryVideoSnapshot>,
    archived_videos: Vec<StoryVideoSnapshot>,
    error: Option<String>,
}

impl StorySegmentSnapshot {
    fn from_segment(
        value: &StorySegment,
        media: &impl Fn(&str) -> Option<StoryMediaPresentation>,
    ) -> Self {
        Self {
            id: value.id.clone(),
            title: value.title.clone(),
            synopsis: value.synopsis.clone(),
            kind: match value.kind {
                StorySegmentKind::Story => "story",
                StorySegmentKind::Transition => "transition",
            }
            .into(),
            seconds: value.seconds,
            source_range: StorySourceRangeSnapshot::from(&value.source_range),
            character_ids: value.character_ids.clone(),
            scene_ids: value.scene_ids.clone(),
            prop_ids: value.prop_ids.clone(),
            detail: value.detail.as_ref().map(StorySegmentDetailSnapshot::from),
            first_frames: StoryImageCollectionSnapshot::from_collection(&value.first_frames, media),
            user_selected_first_frame_id: value.user_selected_first_frame_id.clone(),
            inherited_first_frame_source_segment_id: value
                .inherited_first_frame_source_segment_id
                .clone(),
            last_frames: StoryImageCollectionSnapshot::from_collection(&value.last_frames, media),
            actual_video_last_frame_id: value.actual_video_last_frame_id.clone(),
            video_guidance_mode: match value.video_guidance_mode {
                StoryVideoGuidanceMode::FirstFrame => "first_frame",
                StoryVideoGuidanceMode::FirstAndLastFrames => "first_and_last_frames",
                StoryVideoGuidanceMode::PreviousVideo => "previous_video",
                StoryVideoGuidanceMode::SourceVideo => "source_video",
            }
            .into(),
            attempt: value.attempt.as_ref().map(StoryVideoAttemptSnapshot::from),
            previous_attempts: value
                .previous_attempts
                .iter()
                .map(StoryVideoAttemptSnapshot::from)
                .collect(),
            video: value
                .video
                .as_ref()
                .map(|value| StoryVideoSnapshot::from_video(value, media)),
            archived_videos: value
                .archived_videos
                .iter()
                .map(|value| StoryVideoSnapshot::from_video(value, media))
                .collect(),
            error: value.error.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StorySegmentRelationSnapshot {
    id: String,
    segment_id: String,
    character_id: String,
    scene_id: String,
    action: String,
    position: String,
    start_second: u8,
    end_second: u8,
}

impl From<&StorySegmentRelation> for StorySegmentRelationSnapshot {
    fn from(value: &StorySegmentRelation) -> Self {
        Self {
            id: value.id.clone(),
            segment_id: value.segment_id.clone(),
            character_id: value.character_id.clone(),
            scene_id: value.scene_id.clone(),
            action: value.action.clone(),
            position: value.position.clone(),
            start_second: value.start_second,
            end_second: value.end_second,
        }
    }
}
