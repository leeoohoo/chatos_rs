use super::domain::*;
use uuid::Uuid;

#[test]
fn schema_v2_rejects_unsafe_native_filenames() {
    let mut project = project();
    project.props[0]
        .media
        .images
        .push(StoryImage::test_image("../escape.png"));

    assert!(project.validate().is_err());
}

#[test]
fn resource_attachment_uses_the_macos_confirmation_and_invalidation_rules() {
    let mut project = project();
    let first = StoryImage::test_image("first.png");
    let last = StoryImage::test_image("last.png");
    project.segments[0].first_frames.images.push(first.clone());
    project.segments[0].first_frames.confirmed_image_id = Some(first.id);
    project.segments[0].last_frames.images.push(last.clone());
    project.segments[0].last_frames.confirmed_image_id = Some(last.id);
    let imported = StoryImage::test_image("imported.png");

    let result = attach_imported_image(
        &project,
        &StoryImageTarget::Resource {
            resource_id: "key".into(),
        },
        imported.clone(),
        true,
    )
    .expect("resource attachment");

    assert_eq!(result.props[0].media.images, vec![imported.clone()]);
    assert_eq!(
        result.props[0].media.confirmed_image_id.as_deref(),
        Some(imported.id.as_str())
    );
    assert_eq!(result.segments[0].first_frames.confirmed_image_id, None);
    assert_eq!(result.segments[0].last_frames.confirmed_image_id, None);
}

#[test]
fn first_frame_attachment_records_an_explicit_user_choice() {
    let mut project = project();
    project.segments[0].inherited_first_frame_source_segment_id = Some("previous".into());
    let image = StoryImage::test_image("first.png");

    let result = attach_imported_image(
        &project,
        &StoryImageTarget::Frame {
            segment_id: "s1".into(),
            role: StoryFrameRole::First,
        },
        image.clone(),
        true,
    )
    .expect("frame attachment");

    assert_eq!(
        result.segments[0].first_frames.confirmed_image_id,
        Some(image.id.clone())
    );
    assert_eq!(
        result.segments[0].user_selected_first_frame_id,
        Some(image.id)
    );
    assert_eq!(
        result.segments[0].inherited_first_frame_source_segment_id,
        None
    );
}

#[test]
fn last_frame_attachment_confirms_without_marking_a_user_first_frame() {
    let project = project();
    let image = StoryImage::test_image("last.png");

    let result = attach_imported_image(
        &project,
        &StoryImageTarget::Frame {
            segment_id: "s1".into(),
            role: StoryFrameRole::Last,
        },
        image.clone(),
        true,
    )
    .expect("last-frame attachment");

    assert_eq!(
        result.segments[0].last_frames.confirmed_image_id,
        Some(image.id)
    );
    assert_eq!(result.segments[0].user_selected_first_frame_id, None);
}

#[test]
fn resource_confirmation_fails_closed_during_an_unresolved_video_submission() {
    let mut project = project();
    let image = StoryImage::test_image("resource.png");
    project.props[0].media.images.push(image.clone());
    project.segments[0].attempt = Some(StoryVideoAttempt {
        id: Uuid::new_v4().to_string(),
        model_config_id: "video".into(),
        prompt: "prompt".into(),
        size: "768P".into(),
        ratio: "16:9".into(),
        seconds: 15,
        job_id: None,
        status: "submitting".into(),
        created_at_unix_ms: 1,
    });

    let result = confirm_image(
        &project,
        &StoryImageTarget::Resource {
            resource_id: "key".into(),
        },
        &image.id,
    );

    assert!(result.is_err());
}

#[test]
fn persisted_project_deserialization_rejects_unknown_fields() {
    let project = project();
    let mut value = serde_json::to_value(project).expect("serialize project");
    value
        .as_object_mut()
        .expect("project object")
        .insert("absolutePath".into(), serde_json::json!("C:\\secret"));

    assert!(serde_json::from_value::<StoryProject>(value).is_err());
}

fn project() -> StoryProject {
    StoryProject {
        id: Uuid::new_v4().to_string(),
        version: STORY_SCHEMA_VERSION,
        title: "Story".into(),
        description: "Description".into(),
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
