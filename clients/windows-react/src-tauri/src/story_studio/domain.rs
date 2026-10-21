#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

pub(super) const STORY_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryModelSelection {
    pub text_model_id: String,
    pub image_model_id: String,
    pub video_model_id: String,
    pub supported_video_durations: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryImage {
    pub id: String,
    /// Native-only storage identity. Safe IPC snapshots must replace this with an opaque token.
    pub filename: String,
    pub mime_type: String,
    pub source_resource_id: Option<String>,
    pub generation_attempt_id: Option<String>,
    pub provider_result_id: Option<String>,
    pub provider_asset_id: Option<String>,
    pub derived_from_video_job_id: Option<String>,
}

impl StoryImage {
    #[cfg(test)]
    pub(super) fn test_image(filename: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            filename: filename.into(),
            mime_type: "image/png".into(),
            source_resource_id: None,
            generation_attempt_id: None,
            provider_result_id: None,
            provider_asset_id: None,
            derived_from_video_job_id: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryImageCollection {
    pub images: Vec<StoryImage>,
    pub confirmed_image_id: Option<String>,
    pub generation_attempt_id: Option<String>,
}

impl StoryImageCollection {
    fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        if self.images.iter().any(|image| {
            Uuid::parse_str(&image.id).is_err()
                || !ids.insert(image.id.as_str())
                || !valid_storage_filename(&image.filename)
                || !matches!(
                    image.mime_type.as_str(),
                    "image/png" | "image/jpeg" | "image/webp"
                )
        }) {
            return Err(invalid_story());
        }
        if self
            .confirmed_image_id
            .as_ref()
            .is_some_and(|id| !self.images.iter().any(|image| &image.id == id))
        {
            return Err(invalid_story());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryCharacterProfile {
    pub is_protagonist: bool,
    pub role_in_story: String,
    pub appearance: String,
    pub personality: String,
    pub motivation: String,
    pub relationships: String,
    pub costume: String,
    pub consistency_notes: String,
}

impl StoryCharacterProfile {
    fn validate(&self) -> Result<(), String> {
        validate_profile_fields([
            &self.role_in_story,
            &self.appearance,
            &self.personality,
            &self.motivation,
            &self.relationships,
            &self.costume,
            &self.consistency_notes,
        ])
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StorySceneProfile {
    pub role_in_story: String,
    pub setting: String,
    pub spatial_layout: String,
    pub lighting_and_palette: String,
    pub key_elements: String,
    pub atmosphere: String,
    pub consistency_notes: String,
}

impl StorySceneProfile {
    fn validate(&self) -> Result<(), String> {
        validate_profile_fields([
            &self.role_in_story,
            &self.setting,
            &self.spatial_layout,
            &self.lighting_and_palette,
            &self.key_elements,
            &self.atmosphere,
            &self.consistency_notes,
        ])
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryCharacter {
    pub id: String,
    pub name: String,
    pub profile: StoryCharacterProfile,
    pub image_prompt: String,
    pub media: StoryImageCollection,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryScene {
    pub id: String,
    pub name: String,
    pub profile: StorySceneProfile,
    pub image_prompt: String,
    pub media: StoryImageCollection,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryProp {
    pub id: String,
    pub name: String,
    pub description: String,
    pub image_prompt: String,
    pub media: StoryImageCollection,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum StorySegmentKind {
    Story,
    Transition,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum StoryVideoGuidanceMode {
    FirstFrame,
    FirstAndLastFrames,
    PreviousVideo,
    SourceVideo,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StorySourceRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryShot {
    pub start: u8,
    pub end: u8,
    pub prompt: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StorySegmentDetail {
    pub first_frame_prompt: String,
    pub last_frame_prompt: Option<String>,
    pub shots: Vec<StoryShot>,
    pub continuity_in: String,
    pub continuity_out: String,
    pub audio: String,
    pub constraints: String,
}

impl StorySegmentDetail {
    fn validate(&self, duration: u8) -> Result<(), String> {
        if !(2..=15).contains(&duration)
            || self.first_frame_prompt.trim().is_empty()
            || self.first_frame_prompt.chars().count() > 3_000
            || self
                .last_frame_prompt
                .as_ref()
                .is_some_and(|value| value.trim().is_empty() || value.chars().count() > 3_000)
            || !(1..=8).contains(&self.shots.len())
            || self.continuity_in.chars().count() > 1_000
            || self.continuity_out.chars().count() > 1_000
            || self.audio.chars().count() > 1_000
            || self.constraints.chars().count() > 2_000
        {
            return Err(invalid_story());
        }
        let mut cursor = 0;
        for shot in &self.shots {
            if shot.start != cursor
                || shot.end <= shot.start
                || shot.end > duration
                || shot.prompt.trim().is_empty()
                || shot.prompt.chars().count() > 2_000
            {
                return Err(invalid_story());
            }
            cursor = shot.end;
        }
        if cursor != duration {
            return Err(invalid_story());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryVideoAttempt {
    pub id: String,
    pub model_config_id: String,
    pub prompt: String,
    pub size: String,
    pub ratio: String,
    pub seconds: u8,
    pub job_id: Option<String>,
    pub status: String,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryVideo {
    pub filename: String,
    pub job_id: String,
    pub model_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StorySegment {
    pub id: String,
    pub title: String,
    pub synopsis: String,
    pub kind: StorySegmentKind,
    pub seconds: u8,
    pub source_range: StorySourceRange,
    pub character_ids: Vec<String>,
    pub scene_ids: Vec<String>,
    pub prop_ids: Vec<String>,
    pub detail: Option<StorySegmentDetail>,
    pub first_frames: StoryImageCollection,
    pub user_selected_first_frame_id: Option<String>,
    pub inherited_first_frame_source_segment_id: Option<String>,
    pub last_frames: StoryImageCollection,
    pub actual_video_last_frame_id: Option<String>,
    pub video_guidance_mode: StoryVideoGuidanceMode,
    pub attempt: Option<StoryVideoAttempt>,
    pub previous_attempts: Vec<StoryVideoAttempt>,
    pub video: Option<StoryVideo>,
    pub archived_videos: Vec<StoryVideo>,
    pub error: Option<String>,
}

impl StorySegment {
    fn resource_ids(&self) -> impl Iterator<Item = &String> {
        self.character_ids
            .iter()
            .chain(self.scene_ids.iter())
            .chain(self.prop_ids.iter())
    }

    fn has_unresolved_video(&self) -> bool {
        self.attempt.is_some() && self.video.is_none()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StorySegmentRelation {
    pub id: String,
    pub segment_id: String,
    pub character_id: String,
    pub scene_id: String,
    pub action: String,
    pub position: String,
    pub start_second: u8,
    pub end_second: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoryProject {
    pub id: String,
    pub version: u32,
    pub title: String,
    pub description: String,
    pub models: StoryModelSelection,
    pub source: String,
    pub style: String,
    pub ratio: String,
    pub summary: String,
    pub characters: Vec<StoryCharacter>,
    pub scenes: Vec<StoryScene>,
    pub props: Vec<StoryProp>,
    pub segments: Vec<StorySegment>,
    pub relations: Vec<StorySegmentRelation>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

impl StoryProject {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.version != STORY_SCHEMA_VERSION
            || Uuid::parse_str(&self.id).is_err()
            || self.title.trim().is_empty()
            || self.title.chars().count() > 120
            || self.description.chars().count() > 4_000
            || self.source.chars().count() > 80_000
            || self.style.chars().count() > 2_000
            || self.summary.chars().count() > 16_000
            || self.models.text_model_id.is_empty()
            || self.models.image_model_id.is_empty()
            || self.models.video_model_id.is_empty()
            || self.segments.len() > 200
            || self.characters.len() > 100
            || self.scenes.len() > 100
            || self.props.len() > 100
            || self.relations.len() > 1_600
        {
            return Err(invalid_story());
        }
        if self
            .models
            .supported_video_durations
            .as_ref()
            .is_some_and(|values| {
                values.is_empty()
                    || values.iter().any(|value| !(2..=15).contains(value))
                    || !all_unique(values.iter())
                    || !values.windows(2).all(|window| window[0] < window[1])
            })
        {
            return Err(invalid_story());
        }
        let resource_ids = self.resource_id_set()?;
        let character_ids = self
            .characters
            .iter()
            .map(|value| value.id.as_str())
            .collect::<HashSet<_>>();
        let scene_ids = self
            .scenes
            .iter()
            .map(|value| value.id.as_str())
            .collect::<HashSet<_>>();
        let prop_ids = self
            .props
            .iter()
            .map(|value| value.id.as_str())
            .collect::<HashSet<_>>();
        let mut segment_ids = HashSet::new();
        for (segment_index, segment) in self.segments.iter().enumerate() {
            if segment.id.is_empty()
                || segment.id.len() > 128
                || !segment_ids.insert(segment.id.as_str())
                || segment.title.trim().is_empty()
                || segment.title.chars().count() > 120
                || segment.synopsis.chars().count() > 2_000
                || !(2..=15).contains(&segment.seconds)
                || segment.character_ids.len() + segment.scene_ids.len() + segment.prop_ids.len()
                    > 8
                || !all_unique(segment.character_ids.iter())
                || !all_unique(segment.scene_ids.iter())
                || !all_unique(segment.prop_ids.iter())
                || segment
                    .character_ids
                    .iter()
                    .any(|id| !character_ids.contains(id.as_str()))
                || segment
                    .scene_ids
                    .iter()
                    .any(|id| !scene_ids.contains(id.as_str()))
                || segment
                    .prop_ids
                    .iter()
                    .any(|id| !prop_ids.contains(id.as_str()))
                || segment
                    .resource_ids()
                    .any(|id| !resource_ids.contains(id.as_str()))
                || segment.source_range.end > self.source.chars().count()
                || segment.source_range.end < segment.source_range.start
                || (segment.kind == StorySegmentKind::Story
                    && segment.source_range.end == segment.source_range.start)
                || (segment.kind == StorySegmentKind::Transition
                    && (segment_index == 0
                        || segment_index + 1 >= self.segments.len()
                        || self.segments[segment_index - 1].kind != StorySegmentKind::Story
                        || self.segments[segment_index + 1].kind != StorySegmentKind::Story
                        || segment.source_range.start != segment.source_range.end))
                || segment.archived_videos.len() > 100
                || !all_unique(segment.archived_videos.iter().map(|value| &value.job_id))
            {
                return Err(invalid_story());
            }
            if let Some(detail) = &segment.detail {
                detail.validate(segment.seconds)?;
            }
            segment.first_frames.validate()?;
            segment.last_frames.validate()?;
            if segment
                .video
                .iter()
                .chain(segment.archived_videos.iter())
                .any(|video| !valid_storage_filename(&video.filename))
            {
                return Err(invalid_story());
            }
            if segment
                .user_selected_first_frame_id
                .as_ref()
                .is_some_and(|id| {
                    !segment
                        .first_frames
                        .images
                        .iter()
                        .any(|image| &image.id == id)
                })
                || segment
                    .actual_video_last_frame_id
                    .as_ref()
                    .is_some_and(|id| {
                        !segment
                            .last_frames
                            .images
                            .iter()
                            .any(|image| &image.id == id)
                    })
            {
                return Err(invalid_story());
            }
        }
        let mut relation_ids = HashSet::new();
        for relation in &self.relations {
            let Some(segment) = self
                .segments
                .iter()
                .find(|segment| segment.id == relation.segment_id)
            else {
                return Err(invalid_story());
            };
            if relation.id.is_empty()
                || relation.id.len() > 128
                || !relation_ids.insert(relation.id.as_str())
                || !character_ids.contains(relation.character_id.as_str())
                || !scene_ids.contains(relation.scene_id.as_str())
                || !segment.character_ids.contains(&relation.character_id)
                || !segment.scene_ids.contains(&relation.scene_id)
                || relation.action.trim().is_empty()
                || relation.action.chars().count() > 400
                || relation.position.trim().is_empty()
                || relation.position.chars().count() > 400
                || relation.start_second >= relation.end_second
                || relation.end_second > segment.seconds
            {
                return Err(invalid_story());
            }
        }
        for (index, relation) in self.relations.iter().enumerate() {
            if self.relations.iter().skip(index + 1).any(|other| {
                other.segment_id == relation.segment_id
                    && other.character_id == relation.character_id
                    && other.scene_id != relation.scene_id
                    && other.start_second.max(relation.start_second)
                        < other.end_second.min(relation.end_second)
            }) {
                return Err(invalid_story());
            }
        }
        Ok(())
    }

    pub(super) fn resource_media(&self, id: &str) -> Option<&StoryImageCollection> {
        self.characters
            .iter()
            .find(|value| value.id == id)
            .map(|value| &value.media)
            .or_else(|| {
                self.scenes
                    .iter()
                    .find(|value| value.id == id)
                    .map(|value| &value.media)
            })
            .or_else(|| {
                self.props
                    .iter()
                    .find(|value| value.id == id)
                    .map(|value| &value.media)
            })
    }

    pub(super) fn resource_image_prompt(&self, id: &str) -> Option<&str> {
        self.characters
            .iter()
            .find(|value| value.id == id)
            .map(|value| value.image_prompt.as_str())
            .or_else(|| {
                self.scenes
                    .iter()
                    .find(|value| value.id == id)
                    .map(|value| value.image_prompt.as_str())
            })
            .or_else(|| {
                self.props
                    .iter()
                    .find(|value| value.id == id)
                    .map(|value| value.image_prompt.as_str())
            })
    }

    pub(super) fn resource_media_mut(&mut self, id: &str) -> Option<&mut StoryImageCollection> {
        if let Some(index) = self.characters.iter().position(|value| value.id == id) {
            return Some(&mut self.characters[index].media);
        }
        if let Some(index) = self.scenes.iter().position(|value| value.id == id) {
            return Some(&mut self.scenes[index].media);
        }
        self.props
            .iter_mut()
            .find(|value| value.id == id)
            .map(|value| &mut value.media)
    }

    pub(super) fn has_unresolved_dependent_video(&self, resource_id: &str) -> bool {
        self.segments.iter().any(|segment| {
            segment.resource_ids().any(|id| id == resource_id) && segment.has_unresolved_video()
        })
    }

    pub(super) fn invalidate_dependent_frames(&mut self, resource_id: &str) {
        for segment in &mut self.segments {
            if segment.resource_ids().any(|id| id == resource_id) && segment.attempt.is_none() {
                segment.first_frames.confirmed_image_id = None;
                segment.last_frames.confirmed_image_id = None;
            }
        }
    }

    fn resource_id_set(&self) -> Result<HashSet<&str>, String> {
        let mut ids = HashSet::new();
        for (id, name, prompt, media) in self
            .characters
            .iter()
            .map(|value| (&value.id, &value.name, &value.image_prompt, &value.media))
            .chain(
                self.scenes
                    .iter()
                    .map(|value| (&value.id, &value.name, &value.image_prompt, &value.media)),
            )
            .chain(
                self.props
                    .iter()
                    .map(|value| (&value.id, &value.name, &value.image_prompt, &value.media)),
            )
        {
            if id.is_empty()
                || id.len() > 128
                || name.trim().is_empty()
                || name.chars().count() > 120
                || prompt.trim().is_empty()
                || prompt.chars().count() > 4_000
                || !ids.insert(id.as_str())
            {
                return Err(invalid_story());
            }
            media.validate()?;
        }
        for character in &self.characters {
            character.profile.validate()?;
        }
        for scene in &self.scenes {
            scene.profile.validate()?;
        }
        if self.props.iter().any(|value| {
            value.description.trim().is_empty() || value.description.chars().count() > 2_000
        }) {
            return Err(invalid_story());
        }
        Ok(ids)
    }
}

pub(super) fn update_resource_prompt(
    project: &StoryProject,
    resource_id: &str,
    prompt: String,
) -> Result<StoryProject, String> {
    let mut next = project.clone();
    if next.has_unresolved_dependent_video(resource_id) {
        return Err("存在尚未核对的服务商提交，暂时不能修改素材描述。".into());
    }
    let updated = if let Some(resource) = next
        .characters
        .iter_mut()
        .find(|resource| resource.id == resource_id)
    {
        resource.image_prompt = prompt;
        resource.media.confirmed_image_id = None;
        true
    } else if let Some(resource) = next
        .scenes
        .iter_mut()
        .find(|resource| resource.id == resource_id)
    {
        resource.image_prompt = prompt;
        resource.media.confirmed_image_id = None;
        true
    } else if let Some(resource) = next
        .props
        .iter_mut()
        .find(|resource| resource.id == resource_id)
    {
        resource.image_prompt = prompt;
        resource.media.confirmed_image_id = None;
        true
    } else {
        false
    };
    if !updated {
        return Err(invalid_target());
    }
    next.invalidate_dependent_frames(resource_id);
    next.validate()?;
    Ok(next)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StorySegmentRelationEdit {
    pub id: String,
    pub character_id: String,
    pub scene_id: String,
    pub action: String,
    pub position: String,
    pub start_second: u8,
    pub end_second: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StorySegmentEdit {
    pub segment_id: String,
    pub title: String,
    pub synopsis: String,
    pub kind: StorySegmentKind,
    pub seconds: u8,
    pub detail: Option<StorySegmentDetail>,
    pub character_ids: Vec<String>,
    pub scene_ids: Vec<String>,
    pub prop_ids: Vec<String>,
    pub video_guidance_mode: StoryVideoGuidanceMode,
    pub relations: Vec<StorySegmentRelationEdit>,
}

pub(super) fn apply_segment_edit(
    project: &StoryProject,
    edit: StorySegmentEdit,
) -> Result<StoryProject, String> {
    let mut next = project.clone();
    let index = next
        .segments
        .iter()
        .position(|segment| segment.id == edit.segment_id)
        .filter(|index| {
            next.segments[*index].attempt.is_none() && next.segments[*index].video.is_none()
        })
        .ok_or_else(invalid_target)?;
    if let Some(detail) = &edit.detail {
        detail.validate(edit.seconds)?;
    }
    let relations = edit
        .relations
        .into_iter()
        .map(|relation| StorySegmentRelation {
            id: relation.id,
            segment_id: edit.segment_id.clone(),
            character_id: relation.character_id,
            scene_id: relation.scene_id,
            action: relation.action,
            position: relation.position,
            start_second: relation.start_second,
            end_second: relation.end_second,
        })
        .collect::<Vec<_>>();
    let original = &next.segments[index];
    let original_relations = next
        .relations
        .iter()
        .filter(|relation| relation.segment_id == edit.segment_id)
        .cloned()
        .collect::<Vec<_>>();
    let changes_continuity = original.synopsis != edit.synopsis
        || original.kind != edit.kind
        || original.seconds != edit.seconds
        || original.detail.as_ref().map(|detail| &detail.continuity_in)
            != edit.detail.as_ref().map(|detail| &detail.continuity_in)
        || original
            .detail
            .as_ref()
            .map(|detail| &detail.continuity_out)
            != edit.detail.as_ref().map(|detail| &detail.continuity_out)
        || original.character_ids != edit.character_ids
        || original.scene_ids != edit.scene_ids
        || original.prop_ids != edit.prop_ids
        || original_relations != relations;
    if changes_continuity {
        let segment_count = next.segments.len();
        for adjacent in [index.checked_sub(1), index.checked_add(1)]
            .into_iter()
            .flatten()
            .filter(|adjacent| *adjacent < segment_count)
        {
            if next.segments[adjacent].attempt.is_none() {
                next.segments[adjacent].detail = None;
                next.segments[adjacent].first_frames.confirmed_image_id = None;
                next.segments[adjacent].last_frames.confirmed_image_id = None;
            }
        }
    }
    let segment = &mut next.segments[index];
    segment.title = edit.title;
    segment.synopsis = edit.synopsis;
    segment.kind = edit.kind;
    segment.seconds = edit.seconds;
    segment.detail = edit.detail;
    segment.character_ids = edit.character_ids;
    segment.scene_ids = edit.scene_ids;
    segment.prop_ids = edit.prop_ids;
    segment.video_guidance_mode = edit.video_guidance_mode;
    segment.first_frames.confirmed_image_id = None;
    segment.last_frames.confirmed_image_id = None;
    next.relations
        .retain(|relation| relation.segment_id != edit.segment_id);
    next.relations.extend(relations);
    next.validate()?;
    Ok(next)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StoryImageTarget {
    Resource {
        resource_id: String,
    },
    Frame {
        segment_id: String,
        role: StoryFrameRole,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StoryFrameRole {
    First,
    Last,
}

pub(super) fn attach_imported_image(
    project: &StoryProject,
    target: &StoryImageTarget,
    image: StoryImage,
    confirms_imported_image: bool,
) -> Result<StoryProject, String> {
    let mut next = project.clone();
    match target {
        StoryImageTarget::Resource { resource_id } => {
            let should_confirm = {
                let media = next
                    .resource_media(resource_id)
                    .ok_or_else(invalid_target)?;
                confirms_imported_image || media.confirmed_image_id.is_none()
            };
            let image_id = image.id.clone();
            let media = next
                .resource_media_mut(resource_id)
                .ok_or_else(invalid_target)?;
            media.images.push(image);
            if should_confirm {
                media.confirmed_image_id = Some(image_id);
                next.invalidate_dependent_frames(resource_id);
            }
        }
        StoryImageTarget::Frame { segment_id, role } => {
            let segment = next
                .segments
                .iter_mut()
                .find(|segment| segment.id == *segment_id && segment.attempt.is_none())
                .ok_or_else(invalid_target)?;
            let image_id = image.id.clone();
            match role {
                StoryFrameRole::First => {
                    let should_confirm = confirms_imported_image
                        || segment.first_frames.confirmed_image_id.is_none();
                    segment.first_frames.images.push(image);
                    if should_confirm {
                        segment.first_frames.confirmed_image_id = Some(image_id.clone());
                        segment.user_selected_first_frame_id = Some(image_id);
                        segment.inherited_first_frame_source_segment_id = None;
                    }
                }
                StoryFrameRole::Last => {
                    let should_confirm =
                        confirms_imported_image || segment.last_frames.confirmed_image_id.is_none();
                    segment.last_frames.images.push(image);
                    if should_confirm {
                        segment.last_frames.confirmed_image_id = Some(image_id);
                    }
                }
            }
        }
    }
    next.validate()?;
    Ok(next)
}

pub(super) fn confirm_image(
    project: &StoryProject,
    target: &StoryImageTarget,
    image_id: &str,
) -> Result<Option<StoryProject>, String> {
    let mut next = project.clone();
    match target {
        StoryImageTarget::Resource { resource_id } => {
            let media = next
                .resource_media(resource_id)
                .ok_or_else(invalid_target)?;
            if !media.images.iter().any(|image| image.id == image_id)
                || media.confirmed_image_id.as_deref() == Some(image_id)
            {
                return Ok(None);
            }
            if next.has_unresolved_dependent_video(resource_id) {
                return Err("存在尚未核对的服务商提交，暂时不能切换素材版本。".into());
            }
            next.resource_media_mut(resource_id)
                .ok_or_else(invalid_target)?
                .confirmed_image_id = Some(image_id.to_owned());
            next.invalidate_dependent_frames(resource_id);
        }
        StoryImageTarget::Frame { segment_id, role } => {
            let segment = next
                .segments
                .iter_mut()
                .find(|segment| segment.id == *segment_id && segment.attempt.is_none())
                .ok_or_else(invalid_target)?;
            let media = match role {
                StoryFrameRole::First => &mut segment.first_frames,
                StoryFrameRole::Last => &mut segment.last_frames,
            };
            if !media.images.iter().any(|image| image.id == image_id) {
                return Ok(None);
            }
            media.confirmed_image_id = Some(image_id.to_owned());
            if *role == StoryFrameRole::First {
                segment.user_selected_first_frame_id = Some(image_id.to_owned());
                segment.inherited_first_frame_source_segment_id = None;
            }
        }
    }
    next.validate()?;
    Ok(Some(next))
}

pub(super) fn clear_confirmed_last_frame(
    project: &StoryProject,
    segment_id: &str,
) -> Result<Option<StoryProject>, String> {
    let mut next = project.clone();
    let segment = next
        .segments
        .iter_mut()
        .find(|segment| {
            segment.id == segment_id && segment.attempt.is_none() && segment.video.is_none()
        })
        .ok_or_else(invalid_target)?;
    if segment.last_frames.confirmed_image_id.is_none()
        && segment.video_guidance_mode != StoryVideoGuidanceMode::FirstAndLastFrames
    {
        return Ok(None);
    }
    segment.last_frames.confirmed_image_id = None;
    if segment.video_guidance_mode == StoryVideoGuidanceMode::FirstAndLastFrames {
        segment.video_guidance_mode = StoryVideoGuidanceMode::FirstFrame;
    }
    next.validate()?;
    Ok(Some(next))
}

pub(super) fn confirm_latest_resource_images(
    project: &StoryProject,
    resource_ids: &[String],
) -> Result<Option<StoryProject>, String> {
    let requested = resource_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if requested.is_empty() {
        return Ok(None);
    }
    let mut next = project.clone();
    let known_ids = next
        .characters
        .iter()
        .map(|value| value.id.clone())
        .chain(next.scenes.iter().map(|value| value.id.clone()))
        .chain(next.props.iter().map(|value| value.id.clone()))
        .filter(|id| requested.contains(id.as_str()))
        .collect::<Vec<_>>();
    let mut changed = false;
    for resource_id in known_ids {
        let latest_id = next
            .resource_media(&resource_id)
            .and_then(|media| media.images.last())
            .map(|image| image.id.clone());
        let Some(latest_id) = latest_id else { continue };
        if next
            .resource_media(&resource_id)
            .is_some_and(|media| media.confirmed_image_id.as_deref() == Some(&latest_id))
        {
            continue;
        }
        if next.has_unresolved_dependent_video(&resource_id) {
            return Err("存在尚未核对的服务商提交，暂时不能切换素材版本。".into());
        }
        next.resource_media_mut(&resource_id)
            .ok_or_else(invalid_target)?
            .confirmed_image_id = Some(latest_id);
        next.invalidate_dependent_frames(&resource_id);
        changed = true;
    }
    if !changed {
        return Ok(None);
    }
    next.validate()?;
    Ok(Some(next))
}

pub(super) fn confirm_latest_frames(
    project: &StoryProject,
    segment_id: &str,
    use_confirmed_last_frame_for_video: bool,
) -> Result<Option<StoryProject>, String> {
    let mut next = project.clone();
    let segment = next
        .segments
        .iter_mut()
        .find(|segment| {
            segment.id == segment_id && segment.attempt.is_none() && segment.video.is_none()
        })
        .ok_or_else(invalid_target)?;
    let mut changed = false;
    if segment.first_frames.confirmed_image_id.is_none() {
        if let Some(latest) = segment.first_frames.images.last() {
            segment.first_frames.confirmed_image_id = Some(latest.id.clone());
            segment.user_selected_first_frame_id = Some(latest.id.clone());
            segment.inherited_first_frame_source_segment_id = None;
            changed = true;
        }
    }
    if segment.last_frames.confirmed_image_id.is_none() {
        if let Some(latest) = segment.last_frames.images.last() {
            segment.last_frames.confirmed_image_id = Some(latest.id.clone());
            changed = true;
        }
    }
    if use_confirmed_last_frame_for_video
        && segment.last_frames.confirmed_image_id.is_some()
        && segment.video_guidance_mode != StoryVideoGuidanceMode::FirstAndLastFrames
    {
        segment.video_guidance_mode = StoryVideoGuidanceMode::FirstAndLastFrames;
        changed = true;
    }
    if !changed {
        return Ok(None);
    }
    next.validate()?;
    Ok(Some(next))
}

fn all_unique<'a, T: Eq + std::hash::Hash + 'a>(mut values: impl Iterator<Item = &'a T>) -> bool {
    let mut seen = HashSet::new();
    values.all(|value| seen.insert(value))
}

fn validate_profile_fields<'a>(values: impl IntoIterator<Item = &'a String>) -> Result<(), String> {
    if values
        .into_iter()
        .any(|value| value.trim().is_empty() || value.chars().count() > 600)
    {
        Err(invalid_story())
    } else {
        Ok(())
    }
}

pub(super) fn valid_storage_filename(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('.')
        && !value.contains('/')
        && !value.contains('\\')
        && value != "."
        && value != ".."
}

fn invalid_story() -> String {
    "剧情数据无效，请重新打开后再试。".into()
}

fn invalid_target() -> String {
    "剧情媒体目标已经变化，请重新选择后再试。".into()
}
