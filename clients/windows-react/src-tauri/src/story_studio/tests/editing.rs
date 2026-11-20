use super::*;

#[test]
fn resource_prompt_edit_matches_macos_confirmation_and_invalidation_rules() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    let mut resource_image = StoryImage::test_image("private-resource.png");
    resource_image.provider_result_id = Some("private-provider-result".into());
    canonical.props[0].media.images.push(resource_image.clone());
    canonical.props[0].media.confirmed_image_id = Some(resource_image.id.clone());
    let first = StoryImage::test_image("private-first.png");
    let last = StoryImage::test_image("private-last.png");
    canonical.segments[0]
        .first_frames
        .images
        .push(first.clone());
    canonical.segments[0].first_frames.confirmed_image_id = Some(first.id.clone());
    canonical.segments[0].user_selected_first_frame_id = Some(first.id.clone());
    canonical.segments[0].last_frames.images.push(last.clone());
    canonical.segments[0].last_frames.confirmed_image_id = Some(last.id.clone());
    store.save("alice", &canonical).unwrap();

    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    let initial = serde_json::to_value(
        manager
            .present_project("alice", 9, &presentation, &canonical.id)
            .unwrap(),
    )
    .unwrap();
    let base_fingerprint = initial["baseFingerprint"].as_str().unwrap().to_owned();
    let saved = manager
        .update_resource_prompt(
            "alice",
            9,
            &presentation,
            StoryResourcePromptInput {
                project_id: canonical.id.clone(),
                base_fingerprint,
                resource_id: "key".into(),
                prompt: "A newly polished brass key".into(),
            },
        )
        .unwrap();
    let safe = serde_json::to_value(saved).unwrap();
    assert_eq!(
        safe["props"][0]["imagePrompt"],
        "A newly polished brass key"
    );
    assert!(safe["props"][0]["media"]["confirmedImageId"].is_null());
    assert!(safe["segments"][0]["firstFrames"]["confirmedImageId"].is_null());
    assert!(safe["segments"][0]["lastFrames"]["confirmedImageId"].is_null());

    let persisted = store.load_native_project("alice", &canonical.id).unwrap();
    assert_eq!(persisted.props[0].media.images[0], resource_image);
    assert_eq!(persisted.segments[0].first_frames.images[0], first);
    assert_eq!(persisted.segments[0].last_frames.images[0], last);
    assert_eq!(
        persisted.segments[0].user_selected_first_frame_id,
        canonical.segments[0].user_selected_first_frame_id
    );
}

#[test]
fn resource_prompt_edit_rejects_unresolved_dependent_video_and_unsafe_input_shape() {
    let mut canonical = project();
    canonical.segments[0].attempt = Some(StoryVideoAttempt {
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
    let original = canonical.clone();
    let fingerprint = project_fingerprint(&canonical).unwrap();
    let result = super::application::apply_story_resource_prompt(
        &mut canonical,
        StoryResourcePromptInput {
            project_id: original.id.clone(),
            base_fingerprint: fingerprint,
            resource_id: "key".into(),
            prompt: "must not replace".into(),
        },
        2,
    );
    assert!(result.is_err());
    assert_eq!(canonical, original);

    assert!(
        serde_json::from_value::<StoryResourcePromptInput>(serde_json::json!({
            "projectId": original.id,
            "baseFingerprint": "fingerprint",
            "resourceId": "key",
            "prompt": "prompt",
            "media": {"confirmedImageId": null}
        }))
        .is_err()
    );
}

#[test]
fn segment_edit_changes_only_macos_authorized_fields_and_preserves_native_state() {
    let mut canonical = project();
    let first = StoryImage::test_image("private-first.png");
    let last = StoryImage::test_image("private-last.png");
    canonical.segments[0]
        .first_frames
        .images
        .push(first.clone());
    canonical.segments[0].first_frames.confirmed_image_id = Some(first.id.clone());
    canonical.segments[0].user_selected_first_frame_id = Some(first.id.clone());
    canonical.segments[0].last_frames.images.push(last.clone());
    canonical.segments[0].last_frames.confirmed_image_id = Some(last.id.clone());
    canonical.segments[0].actual_video_last_frame_id = Some(last.id.clone());
    canonical.segments[0]
        .previous_attempts
        .push(StoryVideoAttempt {
            id: Uuid::new_v4().to_string(),
            model_config_id: "video".into(),
            prompt: "private provider prompt".into(),
            size: "768P".into(),
            ratio: "16:9".into(),
            seconds: 15,
            job_id: Some("private-provider-job".into()),
            status: "completed".into(),
            created_at_unix_ms: 1,
        });
    canonical.segments[0].archived_videos.push(StoryVideo {
        filename: "private-archive.mp4".into(),
        job_id: "private-archive-job".into(),
        model_name: "private-model".into(),
    });
    canonical.segments[0].error = Some("private historical error".into());
    let original = canonical.segments[0].clone();
    let project_id = canonical.id.clone();
    let fingerprint = project_fingerprint(&canonical).unwrap();
    super::application::apply_story_segment_edit(
        &mut canonical,
        StorySegmentEditInput {
            project_id,
            base_fingerprint: fingerprint,
            segment: StorySegmentEdit {
                segment_id: "s1".into(),
                title: "Edited title".into(),
                synopsis: "Edited synopsis".into(),
                kind: StorySegmentKind::Story,
                seconds: 15,
                detail: Some(test_segment_detail(15)),
                character_ids: vec![],
                scene_ids: vec![],
                prop_ids: vec!["key".into()],
                video_guidance_mode: StoryVideoGuidanceMode::FirstAndLastFrames,
                relations: vec![],
            },
        },
        2,
    )
    .unwrap();

    let edited = &canonical.segments[0];
    assert_eq!(edited.title, "Edited title");
    assert_eq!(edited.synopsis, "Edited synopsis");
    assert_eq!(edited.source_range, original.source_range);
    assert_eq!(edited.first_frames.images, original.first_frames.images);
    assert_eq!(edited.last_frames.images, original.last_frames.images);
    assert_eq!(
        edited.user_selected_first_frame_id,
        original.user_selected_first_frame_id
    );
    assert_eq!(
        edited.actual_video_last_frame_id,
        original.actual_video_last_frame_id
    );
    assert_eq!(edited.previous_attempts, original.previous_attempts);
    assert_eq!(edited.archived_videos, original.archived_videos);
    assert_eq!(edited.error, original.error);
    assert_eq!(edited.first_frames.confirmed_image_id, None);
    assert_eq!(edited.last_frames.confirmed_image_id, None);
}

#[test]
fn segment_duration_intent_clamps_relations_and_invalidates_plans_like_macos_editor() {
    let mut canonical = project();
    canonical.segments[0].detail = Some(test_segment_detail(15));
    canonical.relations.push(StorySegmentRelation {
        id: "relation-a".into(),
        segment_id: "s1".into(),
        character_id: "character-a".into(),
        scene_id: "scene-a".into(),
        action: "walk".into(),
        position: "left".into(),
        start_second: 12,
        end_second: 15,
    });
    canonical.characters.push(StoryCharacter {
        id: "character-a".into(),
        name: "Character".into(),
        profile: StoryCharacterProfile {
            is_protagonist: true,
            role_in_story: "Lead".into(),
            appearance: "Coat".into(),
            personality: "Calm".into(),
            motivation: "Leave".into(),
            relationships: "Alone".into(),
            costume: "Blue".into(),
            consistency_notes: "Stable".into(),
        },
        image_prompt: "Character in a blue coat".into(),
        media: StoryImageCollection::default(),
    });
    canonical.scenes.push(StoryScene {
        id: "scene-a".into(),
        name: "Scene".into(),
        profile: StorySceneProfile {
            role_in_story: "Setting".into(),
            setting: "Street".into(),
            spatial_layout: "Long".into(),
            lighting_and_palette: "Blue".into(),
            key_elements: "Rain".into(),
            atmosphere: "Quiet".into(),
            consistency_notes: "Stable".into(),
        },
        image_prompt: "Rainy street".into(),
        media: StoryImageCollection::default(),
    });
    canonical.segments[0]
        .character_ids
        .push("character-a".into());
    canonical.segments[0].scene_ids.push("scene-a".into());
    let project_id = canonical.id.clone();
    let fingerprint = project_fingerprint(&canonical).unwrap();

    super::application::apply_story_segment_duration(
        &mut canonical,
        StorySegmentDurationInput {
            project_id,
            base_fingerprint: fingerprint,
            segment_id: "s1".into(),
            seconds: 5,
        },
        2,
    )
    .unwrap();

    assert_eq!(canonical.segments[0].seconds, 5);
    assert!(canonical.segments[0].detail.is_none());
    assert_eq!(canonical.relations[0].start_second, 4);
    assert_eq!(canonical.relations[0].end_second, 5);

    let fingerprint = project_fingerprint(&canonical).unwrap();
    let project_id = canonical.id.clone();
    let error = super::application::apply_story_segment_duration(
        &mut canonical,
        StorySegmentDurationInput {
            project_id,
            base_fingerprint: fingerprint,
            segment_id: "s1".into(),
            seconds: 7,
        },
        3,
    )
    .unwrap_err();
    assert_eq!(error, "当前视频模型不支持这个分段时长。");
}

#[test]
fn segment_continuity_edit_invalidates_only_editable_neighbors_and_rejects_native_fields() {
    let mut canonical = project();
    canonical.source = "abc".into();
    let mut first = canonical.segments[0].clone();
    first.id = "s0".into();
    first.source_range = StorySourceRange { start: 0, end: 1 };
    let mut middle = canonical.segments[0].clone();
    middle.id = "s1".into();
    middle.source_range = StorySourceRange { start: 1, end: 2 };
    let mut last = canonical.segments[0].clone();
    last.id = "s2".into();
    last.source_range = StorySourceRange { start: 2, end: 3 };
    for segment in [&mut first, &mut middle, &mut last] {
        segment.detail = Some(test_segment_detail(15));
        let image = StoryImage::test_image(&format!("{}.png", segment.id));
        segment.first_frames.images.push(image.clone());
        segment.first_frames.confirmed_image_id = Some(image.id);
    }
    last.attempt = Some(StoryVideoAttempt {
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
    canonical.segments = vec![first, middle, last.clone()];
    let result = apply_segment_edit(
        &canonical,
        StorySegmentEdit {
            segment_id: "s1".into(),
            title: "Middle".into(),
            synopsis: "continuity changed".into(),
            kind: StorySegmentKind::Story,
            seconds: 15,
            detail: Some(test_segment_detail(15)),
            character_ids: vec![],
            scene_ids: vec![],
            prop_ids: vec!["key".into()],
            video_guidance_mode: StoryVideoGuidanceMode::FirstFrame,
            relations: vec![],
        },
    )
    .unwrap();
    assert!(result.segments[0].detail.is_none());
    assert!(result.segments[0].first_frames.confirmed_image_id.is_none());
    assert_eq!(result.segments[2], last);
    assert!(result.segments[1].first_frames.confirmed_image_id.is_none());

    assert!(
        serde_json::from_value::<StorySegmentEditInput>(serde_json::json!({
            "projectId": canonical.id,
            "baseFingerprint": "fingerprint",
            "segment": {
                "segmentId": "s1", "title": "title", "synopsis": "synopsis",
                "kind": "story", "seconds": 15, "detail": null,
                "characterIds": [], "sceneIds": [], "propIds": ["key"],
                "videoGuidanceMode": "first_frame", "relations": [],
                "video": {"filename": "private.mp4"}
            }
        }))
        .is_err()
    );
}

#[test]
fn segment_structure_intents_add_move_delete_and_invalidate_all_plans() {
    let mut canonical = project();
    canonical.segments[0].detail = Some(test_segment_detail(15));
    let original_id = canonical.segments[0].id.clone();
    let project_id = canonical.id.clone();
    let add_fingerprint = project_fingerprint(&canonical).unwrap();
    super::application::apply_story_segment_structure(
        &mut canonical,
        StorySegmentStructureInput {
            project_id: project_id.clone(),
            base_fingerprint: add_fingerprint,
            mutation: StorySegmentStructureMutation {
                operation: StorySegmentStructureOperation::Add,
                segment_id: None,
                offset: None,
            },
        },
        2,
    )
    .unwrap();
    assert_eq!(canonical.segments.len(), 2);
    assert!(canonical
        .segments
        .iter()
        .all(|segment| segment.detail.is_none()));
    let added_id = canonical.segments[1].id.clone();

    let move_fingerprint = project_fingerprint(&canonical).unwrap();
    super::application::apply_story_segment_structure(
        &mut canonical,
        StorySegmentStructureInput {
            project_id: project_id.clone(),
            base_fingerprint: move_fingerprint,
            mutation: StorySegmentStructureMutation {
                operation: StorySegmentStructureOperation::Move,
                segment_id: Some(added_id.clone()),
                offset: Some(-1),
            },
        },
        3,
    )
    .unwrap();
    assert_eq!(canonical.segments[0].id, added_id);

    let delete_fingerprint = project_fingerprint(&canonical).unwrap();
    super::application::apply_story_segment_structure(
        &mut canonical,
        StorySegmentStructureInput {
            project_id,
            base_fingerprint: delete_fingerprint,
            mutation: StorySegmentStructureMutation {
                operation: StorySegmentStructureOperation::Delete,
                segment_id: Some(original_id),
                offset: None,
            },
        },
        4,
    )
    .unwrap();
    assert_eq!(canonical.segments.len(), 1);
    assert_eq!(canonical.segments[0].id, added_id);
}

#[test]
fn segment_structure_intent_rejects_media_jobs_completed_video_and_extra_fields() {
    let mut canonical = project();
    canonical.segments[0].video = Some(StoryVideo {
        filename: "completed.mp4".into(),
        job_id: "completed-job".into(),
        model_name: "video".into(),
    });
    let original = canonical.clone();
    let fingerprint = project_fingerprint(&canonical).unwrap();
    let result = super::application::apply_story_segment_structure(
        &mut canonical,
        StorySegmentStructureInput {
            project_id: original.id.clone(),
            base_fingerprint: fingerprint,
            mutation: StorySegmentStructureMutation {
                operation: StorySegmentStructureOperation::Delete,
                segment_id: Some("s1".into()),
                offset: None,
            },
        },
        2,
    );
    assert!(result.is_err());
    assert_eq!(canonical, original);
    assert!(
        serde_json::from_value::<StorySegmentStructureInput>(serde_json::json!({
            "projectId": original.id,
            "baseFingerprint": "fingerprint",
            "mutation": {"operation": "add", "segment": {"id": "forbidden"}}
        }))
        .is_err()
    );
}

#[test]
fn typed_image_confirmation_and_last_frame_clear_match_macos_rules() {
    let mut canonical = project();
    let first_resource = StoryImage::test_image("resource-first.png");
    let second_resource = StoryImage::test_image("resource-second.png");
    canonical.props[0].media.images = vec![first_resource.clone(), second_resource.clone()];
    canonical.props[0].media.confirmed_image_id = Some(first_resource.id);
    let frame = StoryImage::test_image("frame.png");
    canonical.segments[0]
        .first_frames
        .images
        .push(frame.clone());
    canonical.segments[0].first_frames.confirmed_image_id = Some(frame.id);
    let project_id = canonical.id.clone();
    let fingerprint = project_fingerprint(&canonical).unwrap();
    super::application::apply_story_image_confirmation(
        &mut canonical,
        StoryImageConfirmationInput {
            project_id: project_id.clone(),
            base_fingerprint: fingerprint,
            resource_id: Some("key".into()),
            segment_id: None,
            frame_role: None,
            image_id: second_resource.id.clone(),
        },
        2,
    )
    .unwrap();
    assert_eq!(
        canonical.props[0].media.confirmed_image_id,
        Some(second_resource.id.clone())
    );
    assert!(canonical.segments[0]
        .first_frames
        .confirmed_image_id
        .is_none());

    let last = StoryImage::test_image("last.png");
    canonical.segments[0].last_frames.images.push(last.clone());
    canonical.segments[0].last_frames.confirmed_image_id = Some(last.id);
    canonical.segments[0].video_guidance_mode = StoryVideoGuidanceMode::FirstAndLastFrames;
    let fingerprint = project_fingerprint(&canonical).unwrap();
    super::application::apply_story_last_frame_clear(
        &mut canonical,
        StoryLastFrameClearInput {
            project_id,
            base_fingerprint: fingerprint,
            segment_id: "s1".into(),
        },
        3,
    )
    .unwrap();
    assert!(canonical.segments[0]
        .last_frames
        .confirmed_image_id
        .is_none());
    assert_eq!(
        canonical.segments[0].video_guidance_mode,
        StoryVideoGuidanceMode::FirstFrame
    );

    let invalid = StoryImageConfirmationInput {
        project_id: canonical.id.clone(),
        base_fingerprint: project_fingerprint(&canonical).unwrap(),
        resource_id: Some("key".into()),
        segment_id: Some("s1".into()),
        frame_role: Some(StoryFrameRoleInput::First),
        image_id: second_resource.id,
    };
    assert!(
        super::application::apply_story_image_confirmation(&mut canonical, invalid, 4).is_err()
    );
}

#[test]
fn latest_resource_and_frame_confirmation_matches_macos_batch_rules() {
    let mut canonical = project();
    let older = StoryImage::test_image("older-resource.png");
    let latest = StoryImage::test_image("latest-resource.png");
    canonical.props[0].media.images = vec![older, latest.clone()];
    let first = StoryImage::test_image("latest-first.png");
    let last = StoryImage::test_image("latest-last.png");
    canonical.segments[0]
        .first_frames
        .images
        .push(first.clone());
    canonical.segments[0].last_frames.images.push(last.clone());
    let project_id = canonical.id.clone();
    let fingerprint = project_fingerprint(&canonical).unwrap();
    super::application::apply_story_latest_resource_images(
        &mut canonical,
        StoryLatestResourceImagesInput {
            project_id: project_id.clone(),
            base_fingerprint: fingerprint,
            resource_ids: vec!["key".into(), "key".into(), "missing".into()],
        },
        2,
    )
    .unwrap();
    assert_eq!(canonical.props[0].media.confirmed_image_id, Some(latest.id));

    let fingerprint = project_fingerprint(&canonical).unwrap();
    super::application::apply_story_latest_frames(
        &mut canonical,
        StoryLatestFramesInput {
            project_id,
            base_fingerprint: fingerprint,
            segment_id: "s1".into(),
            use_confirmed_last_frame_for_video: true,
        },
        3,
    )
    .unwrap();
    assert_eq!(
        canonical.segments[0].first_frames.confirmed_image_id,
        Some(first.id.clone())
    );
    assert_eq!(
        canonical.segments[0].user_selected_first_frame_id,
        Some(first.id)
    );
    assert_eq!(
        canonical.segments[0].last_frames.confirmed_image_id,
        Some(last.id)
    );
    assert_eq!(
        canonical.segments[0].video_guidance_mode,
        StoryVideoGuidanceMode::FirstAndLastFrames
    );
}

#[test]
fn image_retry_unlock_requires_an_explicit_attempt_and_exact_target() {
    let mut canonical = project();
    canonical.props[0].media.generation_attempt_id = Some(Uuid::new_v4().to_string());
    let project_id = canonical.id.clone();
    let fingerprint = project_fingerprint(&canonical).unwrap();

    super::application::allow_story_image_retry(
        &mut canonical,
        StoryImageRetryInput {
            project_id,
            base_fingerprint: fingerprint,
            resource_id: Some("key".into()),
            segment_id: None,
            frame_role: None,
        },
        2,
    )
    .unwrap();
    assert!(canonical.props[0].media.generation_attempt_id.is_none());

    let project_id = canonical.id.clone();
    let fingerprint = project_fingerprint(&canonical).unwrap();
    let error = super::application::allow_story_image_retry(
        &mut canonical,
        StoryImageRetryInput {
            project_id,
            base_fingerprint: fingerprint,
            resource_id: Some("key".into()),
            segment_id: None,
            frame_role: None,
        },
        3,
    )
    .unwrap_err();
    assert_eq!(error, "当前图片没有需要核对的提交。");
}
