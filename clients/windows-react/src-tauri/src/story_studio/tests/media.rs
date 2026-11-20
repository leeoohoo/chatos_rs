use super::*;

#[test]
fn local_story_image_import_normalizes_png_and_commits_manifest_with_asset() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let canonical = project();
    store.save("alice", &canonical).unwrap();
    let source = app_data.path().join("picked.jpg");
    image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]))
        .save_with_format(&source, image::ImageFormat::Jpeg)
        .unwrap();
    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    manager
        .present_project("alice", 12, &presentation, &canonical.id)
        .unwrap();
    let snapshot = manager
        .import_local_story_image(
            "alice",
            12,
            &presentation,
            StoryImageImportInput {
                project_id: canonical.id.clone(),
                base_fingerprint: project_fingerprint(&canonical).unwrap(),
                resource_id: Some("key".into()),
                segment_id: None,
                frame_role: None,
                confirms_imported_image: true,
            },
            &source,
        )
        .unwrap();
    let safe = serde_json::to_value(snapshot).unwrap();
    assert_eq!(
        safe["props"][0]["media"]["images"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let persisted = store.load_native_project("alice", &canonical.id).unwrap();
    let imported = &persisted.props[0].media.images[0];
    assert_eq!(
        persisted.props[0].media.confirmed_image_id,
        Some(imported.id.clone())
    );
    assert_eq!(imported.mime_type, "image/png");
    assert!(imported.filename.starts_with("image-"));
    let bytes = fs::read(
        projects_directory(app_data.path(), "alice")
            .join(&canonical.id)
            .join(&imported.filename),
    )
    .unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn generated_story_image_import_normalizes_bytes_and_cleans_up_stale_mutations() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let canonical = project();
    store.save("alice", &canonical).unwrap();
    let source = app_data.path().join("generated.jpg");
    image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]))
        .save_with_format(&source, image::ImageFormat::Jpeg)
        .unwrap();
    let generated_bytes = fs::read(&source).unwrap();
    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    manager
        .present_project("alice", 12, &presentation, &canonical.id)
        .unwrap();
    let original_fingerprint = project_fingerprint(&canonical).unwrap();
    let snapshot = manager
        .import_generated_story_image(
            "alice",
            12,
            &presentation,
            StoryImageImportInput {
                project_id: canonical.id.clone(),
                base_fingerprint: original_fingerprint.clone(),
                resource_id: None,
                segment_id: Some("s1".into()),
                frame_role: Some(StoryFrameRoleInput::First),
                confirms_imported_image: true,
            },
            &generated_bytes,
        )
        .unwrap();
    let encoded = serde_json::to_string(&snapshot).unwrap();
    assert!(!encoded.contains("provider-result"));
    assert!(!encoded.contains("provider-asset"));

    let persisted = store.load_native_project("alice", &canonical.id).unwrap();
    let imported = &persisted.segments[0].first_frames.images[0];
    assert_eq!(
        persisted.segments[0].first_frames.confirmed_image_id,
        Some(imported.id.clone())
    );
    assert_eq!(
        persisted.segments[0].user_selected_first_frame_id,
        Some(imported.id.clone())
    );
    assert_eq!(imported.mime_type, "image/png");
    assert!(imported.provider_result_id.is_none());
    assert!(imported.provider_asset_id.is_none());
    assert!(imported.generation_attempt_id.is_none());
    assert!(!encoded.contains(&imported.filename));
    let directory = projects_directory(app_data.path(), "alice").join(&canonical.id);
    let before = fs::read_dir(&directory).unwrap().count();

    let stale = manager.import_generated_story_image(
        "alice",
        12,
        &presentation,
        StoryImageImportInput {
            project_id: canonical.id.clone(),
            base_fingerprint: original_fingerprint,
            resource_id: Some("key".into()),
            segment_id: None,
            frame_role: None,
            confirms_imported_image: true,
        },
        &generated_bytes,
    );
    assert!(stale.is_err());
    assert_eq!(fs::read_dir(directory).unwrap().count(), before);
}

#[test]
fn asset_image_generation_uses_durable_attempt_and_merges_provider_result() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    let first = StoryImage::test_image("existing-first.png");
    let last = StoryImage::test_image("existing-last.png");
    canonical.segments[0]
        .first_frames
        .images
        .push(first.clone());
    canonical.segments[0].first_frames.confirmed_image_id = Some(first.id);
    canonical.segments[0].last_frames.images.push(last.clone());
    canonical.segments[0].last_frames.confirmed_image_id = Some(last.id);
    store.save("alice", &canonical).unwrap();
    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    manager
        .present_project("alice", 12, &presentation, &canonical.id)
        .unwrap();

    let intent = manager
        .begin_asset_image_generation(
            "alice",
            12,
            &presentation,
            StoryAssetImageGenerationInput {
                project_id: canonical.id.clone(),
                base_fingerprint: project_fingerprint(&canonical).unwrap(),
                resource_id: "key".into(),
                user_ideas: "  add soft light  ".into(),
            },
        )
        .unwrap();
    assert_eq!(intent.model_config_id, canonical.models.image_model_id);
    assert!(intent.prompt.contains("add soft light"));
    assert!(intent.prompt.contains(&canonical.style));
    assert!(intent.prompt.contains(&canonical.props[0].image_prompt));
    let pending = store.load_native_project("alice", &canonical.id).unwrap();
    assert_eq!(
        pending.props[0].media.generation_attempt_id.as_deref(),
        Some(intent.attempt_id.as_str())
    );
    assert!(manager
        .begin_asset_image_generation(
            "alice",
            12,
            &presentation,
            StoryAssetImageGenerationInput {
                project_id: pending.id.clone(),
                base_fingerprint: project_fingerprint(&pending).unwrap(),
                resource_id: "key".into(),
                user_ideas: String::new(),
            },
        )
        .is_err());

    let source = app_data.path().join("provider.jpg");
    image::RgbImage::from_pixel(2, 2, image::Rgb([90, 80, 70]))
        .save_with_format(&source, image::ImageFormat::Jpeg)
        .unwrap();
    let source = fs::read(source).unwrap();
    let directory = projects_directory(app_data.path(), "alice").join(&canonical.id);
    let before = fs::read_dir(&directory).unwrap().count();
    assert!(manager
        .complete_asset_image_generation(
            "alice",
            12,
            &presentation,
            &intent.project_id,
            &intent.resource_id,
            &Uuid::new_v4().to_string(),
            "wrong-result".into(),
            "wrong-asset".into(),
            &source,
        )
        .is_err());
    assert_eq!(fs::read_dir(&directory).unwrap().count(), before);

    let snapshot = manager
        .complete_asset_image_generation(
            "alice",
            12,
            &presentation,
            &intent.project_id,
            &intent.resource_id,
            &intent.attempt_id,
            "provider-result".into(),
            "provider-asset".into(),
            &source,
        )
        .unwrap();
    let encoded = serde_json::to_string(&snapshot).unwrap();
    assert!(!encoded.contains("provider-result"));
    assert!(!encoded.contains("provider-asset"));
    let completed = store.load_native_project("alice", &canonical.id).unwrap();
    let image = &completed.props[0].media.images[0];
    assert!(completed.props[0].media.generation_attempt_id.is_none());
    assert_eq!(
        completed.props[0].media.confirmed_image_id,
        Some(image.id.clone())
    );
    assert_eq!(image.source_resource_id.as_deref(), Some("key"));
    assert_eq!(
        image.generation_attempt_id.as_deref(),
        Some(intent.attempt_id.as_str())
    );
    assert_eq!(image.provider_result_id.as_deref(), Some("provider-result"));
    assert_eq!(image.provider_asset_id.as_deref(), Some("provider-asset"));
    assert!(completed.segments[0]
        .first_frames
        .confirmed_image_id
        .is_none());
    assert!(completed.segments[0]
        .last_frames
        .confirmed_image_id
        .is_none());
    assert!(!encoded.contains(&image.filename));
}

#[test]
fn frame_image_generation_uses_ordered_native_references_and_role_scoped_attempts() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.segments[0].detail = Some(test_segment_detail(15));
    store.save("alice", &canonical).unwrap();
    let reference_path = app_data.path().join("reference.png");
    image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
        .save_with_format(&reference_path, image::ImageFormat::Png)
        .unwrap();
    let reference_bytes = fs::read(reference_path).unwrap();
    let resource_image = StoryImage {
        id: Uuid::new_v4().to_string(),
        filename: "resource.png".into(),
        mime_type: "image/png".into(),
        source_resource_id: Some("key".into()),
        generation_attempt_id: None,
        provider_result_id: None,
        provider_asset_id: None,
        derived_from_video_job_id: None,
    };
    let resource_image_id = resource_image.id.clone();
    let resource_filename = resource_image.filename.clone();
    canonical = store
        .mutate_project_with_image(
            "alice",
            &canonical.id,
            &resource_filename,
            &reference_bytes,
            |project| {
                project.props[0].media.images.push(resource_image);
                project.props[0].media.confirmed_image_id = Some(resource_image_id);
                Ok(())
            },
        )
        .unwrap();
    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    manager
        .present_project("alice", 22, &presentation, &canonical.id)
        .unwrap();

    let first = manager
        .begin_frame_image_generation(
            "alice",
            22,
            &presentation,
            StoryFrameImageGenerationInput {
                project_id: canonical.id.clone(),
                base_fingerprint: project_fingerprint(&canonical).unwrap(),
                segment_id: "s1".into(),
                frame_role: StoryFrameRoleInput::First,
                reference_resource_ids: vec!["key".into()],
                user_ideas: "keep the brass texture".into(),
            },
        )
        .unwrap();
    assert_eq!(first.references.len(), 1);
    assert_eq!(first.references[0].bytes, reference_bytes);
    assert!(first.prompt.contains("keep the brass texture"));
    assert!(first.prompt.contains("first frame"));
    assert!(first.prompt.contains("\"referenceIndex\":1"));
    let pending = store.load_native_project("alice", &canonical.id).unwrap();
    assert_eq!(
        pending.segments[0]
            .first_frames
            .generation_attempt_id
            .as_deref(),
        Some(first.attempt_id.as_str())
    );
    let first_snapshot = manager
        .complete_frame_image_generation(
            "alice",
            22,
            &presentation,
            &first.project_id,
            &first.segment_id,
            first.frame_role,
            &first.attempt_id,
            "first-result".into(),
            "first-asset".into(),
            &reference_bytes,
        )
        .unwrap();
    assert!(!serde_json::to_string(&first_snapshot)
        .unwrap()
        .contains("first-result"));
    let after_first = store.load_native_project("alice", &canonical.id).unwrap();
    let first_image = after_first.segments[0].first_frames.images[0].clone();
    assert_eq!(
        after_first.segments[0].first_frames.confirmed_image_id,
        Some(first_image.id.clone())
    );
    assert!(after_first.segments[0]
        .user_selected_first_frame_id
        .is_none());

    let last = manager
        .begin_frame_image_generation(
            "alice",
            22,
            &presentation,
            StoryFrameImageGenerationInput {
                project_id: after_first.id.clone(),
                base_fingerprint: project_fingerprint(&after_first).unwrap(),
                segment_id: "s1".into(),
                frame_role: StoryFrameRoleInput::Last,
                reference_resource_ids: vec!["key".into()],
                user_ideas: String::new(),
            },
        )
        .unwrap();
    assert_eq!(last.references.len(), 2);
    assert_eq!(last.references[1].name, "current-segment-first-frame.png");
    assert!(last
        .prompt
        .contains("\"currentSegmentFirstFrameReferenceIndex\":2"));
    manager
        .complete_frame_image_generation(
            "alice",
            22,
            &presentation,
            &last.project_id,
            &last.segment_id,
            last.frame_role,
            &last.attempt_id,
            "last-result".into(),
            "last-asset".into(),
            &reference_bytes,
        )
        .unwrap();
    let completed = store.load_native_project("alice", &canonical.id).unwrap();
    assert!(completed.segments[0]
        .last_frames
        .generation_attempt_id
        .is_none());
    assert_eq!(
        completed.segments[0].last_frames.confirmed_image_id,
        completed.segments[0]
            .last_frames
            .images
            .first()
            .map(|image| image.id.clone())
    );
}

#[test]
fn video_generation_persists_provider_identity_and_commits_mp4_atomically() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.segments[0].detail = Some(test_segment_detail(15));
    store.save("alice", &canonical).unwrap();
    let image_path = app_data.path().join("first.png");
    image::RgbImage::from_pixel(256, 256, image::Rgb([10, 20, 30]))
        .save_with_format(&image_path, image::ImageFormat::Png)
        .unwrap();
    let image_bytes = fs::read(image_path).unwrap();
    let first = StoryImage {
        id: Uuid::new_v4().to_string(),
        filename: "video-first.png".into(),
        mime_type: "image/png".into(),
        source_resource_id: Some("s1".into()),
        generation_attempt_id: None,
        provider_result_id: None,
        provider_asset_id: None,
        derived_from_video_job_id: None,
    };
    let first_id = first.id.clone();
    canonical = store
        .mutate_project_with_image(
            "alice",
            &canonical.id,
            &first.filename.clone(),
            &image_bytes,
            |project| {
                project.segments[0].first_frames.images.push(first);
                project.segments[0].first_frames.confirmed_image_id = Some(first_id);
                Ok(())
            },
        )
        .unwrap();
    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    manager
        .present_project("alice", 31, &presentation, &canonical.id)
        .unwrap();

    let intent = manager
        .begin_video_generation(
            "alice",
            31,
            &presentation,
            StoryVideoGenerationStartInput {
                project_id: canonical.id.clone(),
                base_fingerprint: project_fingerprint(&canonical).unwrap(),
                segment_id: "s1".into(),
                size: "768P".into(),
                user_ideas: "keep the slow camera move".into(),
            },
        )
        .unwrap();
    assert_eq!(
        intent.provider_request.first_frame.unwrap().bytes,
        image_bytes
    );
    assert!(intent
        .provider_request
        .prompt
        .contains("keep the slow camera move"));
    assert!(intent.provider_request.prompt.contains("镜头时间线"));
    assert!(intent.provider_request.prompt.chars().count() <= 6_800);
    let pending = store.load_native_project("alice", &canonical.id).unwrap();
    assert_eq!(
        pending.segments[0]
            .attempt
            .as_ref()
            .map(|value| value.id.as_str()),
        Some(intent.attempt_id.as_str())
    );

    let submitted = manager
        .record_video_submission(
            "alice",
            31,
            &presentation,
            StoryVideoGenerationPollInput {
                project_id: canonical.id.clone(),
                segment_id: "s1".into(),
            },
            &intent.attempt_id,
            "provider-job-1",
            "queued",
        )
        .unwrap();
    assert!(!serde_json::to_string(&submitted)
        .unwrap()
        .contains("provider-job-1"));
    let resume = manager
        .prepare_video_poll(
            "alice",
            31,
            &presentation,
            StoryVideoGenerationPollInput {
                project_id: canonical.id.clone(),
                segment_id: "s1".into(),
            },
        )
        .unwrap();
    assert_eq!(resume.provider_job_id, "provider-job-1");
    assert_eq!(resume.attempt.id, intent.attempt_id);

    let directory = projects_directory(app_data.path(), "alice").join(&canonical.id);
    let before = fs::read_dir(&directory).unwrap().count();
    assert!(manager
        .complete_video_generation(
            "alice",
            31,
            &presentation,
            &canonical.id,
            "s1",
            &Uuid::new_v4().to_string(),
            "provider-job-1".into(),
            "MiniMax-H3".into(),
            b"video-bytes",
        )
        .is_err());
    assert_eq!(fs::read_dir(&directory).unwrap().count(), before);

    let completed = manager
        .complete_video_generation(
            "alice",
            31,
            &presentation,
            &canonical.id,
            "s1",
            &intent.attempt_id,
            "provider-job-1".into(),
            "MiniMax-H3".into(),
            b"video-bytes",
        )
        .unwrap();
    let encoded = serde_json::to_string(&completed).unwrap();
    assert!(!encoded.contains("provider-job-1"));
    assert!(!encoded.contains("video-"));
    let saved = store.load_native_project("alice", &canonical.id).unwrap();
    assert_eq!(
        saved.segments[0].attempt.as_ref().unwrap().status,
        "completed"
    );
    assert_eq!(
        saved.segments[0].video.as_ref().unwrap().job_id,
        "provider-job-1"
    );
    let stored_bytes = store
        .read_media_asset(
            &owner_hash_for_test("alice"),
            &canonical.id,
            &saved.segments[0].video.as_ref().unwrap().filename,
            "video/mp4",
            super::store::MAXIMUM_STORY_VIDEO_BYTES,
        )
        .unwrap();
    assert_eq!(stored_bytes, b"video-bytes");
}

#[test]
fn video_failure_only_clears_attempt_when_submission_is_definitely_absent() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.segments[0].detail = Some(test_segment_detail(15));
    let first = StoryImage::test_image("first.png");
    canonical.segments[0]
        .first_frames
        .images
        .push(first.clone());
    canonical.segments[0].first_frames.confirmed_image_id = Some(first.id);
    store.save("alice", &canonical).unwrap();
    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    manager
        .present_project("alice", 44, &presentation, &canonical.id)
        .unwrap();

    // A missing local file fails before begin_video_generation persists an attempt.
    assert!(manager
        .begin_video_generation(
            "alice",
            44,
            &presentation,
            StoryVideoGenerationStartInput {
                project_id: canonical.id.clone(),
                base_fingerprint: project_fingerprint(&canonical).unwrap(),
                segment_id: "s1".into(),
                size: "768P".into(),
                user_ideas: String::new(),
            },
        )
        .is_err());
    assert!(store
        .load_native_project("alice", &canonical.id)
        .unwrap()
        .segments[0]
        .attempt
        .is_none());
}

#[test]
fn verified_video_retry_archives_the_attempt_without_deleting_history() {
    let mut canonical = project();
    let attempt = StoryVideoAttempt {
        id: Uuid::new_v4().to_string(),
        model_config_id: "video".into(),
        prompt: "private prompt".into(),
        size: "768P".into(),
        ratio: "16:9".into(),
        seconds: 15,
        job_id: None,
        status: "unknown".into(),
        created_at_unix_ms: 1,
    };
    canonical.segments[0].attempt = Some(attempt.clone());
    canonical.segments[0].error = Some("unknown submission".into());
    let project_id = canonical.id.clone();
    let fingerprint = project_fingerprint(&canonical).unwrap();

    super::application::allow_story_video_retry(
        &mut canonical,
        StoryVideoRetryInput {
            project_id,
            base_fingerprint: fingerprint,
            segment_id: "s1".into(),
        },
        2,
    )
    .unwrap();

    assert!(canonical.segments[0].attempt.is_none());
    assert!(canonical.segments[0].error.is_none());
    assert_eq!(canonical.segments[0].previous_attempts, vec![attempt]);
}

#[test]
fn completed_video_archive_preserves_history_and_reopens_typed_guidance() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.segments[0].detail = Some(test_segment_detail(15));
    canonical.segments[0].attempt = Some(StoryVideoAttempt {
        id: Uuid::new_v4().to_string(),
        model_config_id: "video".into(),
        prompt: "prompt".into(),
        size: "768P".into(),
        ratio: "16:9".into(),
        seconds: 15,
        job_id: Some("provider-job".into()),
        status: "completed".into(),
        created_at_unix_ms: 2,
    });
    canonical.segments[0].video = Some(StoryVideo {
        filename: "finished.mp4".into(),
        job_id: "provider-job".into(),
        model_name: "MiniMax-H3".into(),
    });
    store.save("alice", &canonical).unwrap();
    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    manager
        .present_project("alice", 55, &presentation, &canonical.id)
        .unwrap();

    let archived = manager
        .archive_completed_video(
            "alice",
            55,
            &presentation,
            StoryCompletedVideoArchiveInput {
                project_id: canonical.id.clone(),
                base_fingerprint: project_fingerprint(&canonical).unwrap(),
                segment_id: "s1".into(),
                use_source_video: true,
            },
        )
        .unwrap();
    let encoded = serde_json::to_string(&archived).unwrap();
    assert!(!encoded.contains("finished.mp4"));
    assert!(!encoded.contains("provider-job"));
    let archived_native = store.load_native_project("alice", &canonical.id).unwrap();
    assert!(archived_native.segments[0].video.is_none());
    assert!(archived_native.segments[0].attempt.is_none());
    assert_eq!(archived_native.segments[0].archived_videos.len(), 1);
    assert_eq!(archived_native.segments[0].previous_attempts.len(), 1);
    assert_eq!(
        archived_native.segments[0].video_guidance_mode,
        StoryVideoGuidanceMode::SourceVideo
    );

    manager
        .update_video_guidance(
            "alice",
            55,
            &presentation,
            StoryVideoGuidanceInput {
                project_id: archived_native.id.clone(),
                base_fingerprint: project_fingerprint(&archived_native).unwrap(),
                segment_id: "s1".into(),
                mode: StoryVideoGuidanceMode::FirstFrame,
            },
        )
        .unwrap();
    let updated = store.load_native_project("alice", &canonical.id).unwrap();
    assert_eq!(
        updated.segments[0].video_guidance_mode,
        StoryVideoGuidanceMode::FirstFrame
    );
}
