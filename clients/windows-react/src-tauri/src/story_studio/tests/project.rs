use super::*;

#[test]
fn schema_v2_rejects_unsafe_native_filenames() {
    let mut unsafe_image_project = project();
    unsafe_image_project.props[0]
        .media
        .images
        .push(StoryImage::test_image("../escape.png"));

    assert!(unsafe_image_project.validate().is_err());

    let mut unsafe_video_project = project();
    unsafe_video_project.segments[0].video = Some(StoryVideo {
        filename: "../escape.mp4".into(),
        job_id: "provider-job".into(),
        model_name: "video".into(),
    });
    assert!(unsafe_video_project.validate().is_err());
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
fn decoded_video_tail_preserves_guidance_and_becomes_the_next_inherited_first_frame() {
    let mut canonical = project();
    canonical.source = "xy".into();
    canonical.segments[0].video = Some(StoryVideo {
        filename: "video.mp4".into(),
        job_id: "provider-job".into(),
        model_name: "MiniMax-H3".into(),
    });
    let guidance = StoryImage::test_image("guidance.png");
    canonical.segments[0]
        .last_frames
        .images
        .push(guidance.clone());
    canonical.segments[0].last_frames.confirmed_image_id = Some(guidance.id.clone());
    let mut second = canonical.segments[0].clone();
    second.id = "s2".into();
    second.source_range = StorySourceRange { start: 1, end: 2 };
    second.first_frames = StoryImageCollection::default();
    second.last_frames = StoryImageCollection::default();
    second.video = None;
    canonical.segments.push(second);

    super::application::apply_actual_video_last_frame(
        &mut canonical,
        super::application::StoryActualVideoLastFrame {
            segment_id: "s1",
            video_job_id: "provider-job",
            filename: "actual-tail.png".into(),
        },
        2,
    )
    .unwrap();

    let actual_id = canonical.segments[0]
        .actual_video_last_frame_id
        .clone()
        .expect("decoded tail identity");
    assert_eq!(
        canonical.segments[0].last_frames.confirmed_image_id,
        Some(guidance.id)
    );
    assert_eq!(canonical.segments[0].last_frames.images.len(), 2);
    assert_eq!(
        canonical.segments[1].first_frames.confirmed_image_id,
        Some(actual_id.clone())
    );
    assert_eq!(
        canonical.segments[1]
            .inherited_first_frame_source_segment_id
            .as_deref(),
        Some("s1")
    );

    super::application::apply_actual_video_last_frame(
        &mut canonical,
        super::application::StoryActualVideoLastFrame {
            segment_id: "s1",
            video_job_id: "provider-job",
            filename: "duplicate-must-not-be-used.png".into(),
        },
        3,
    )
    .unwrap();
    assert_eq!(canonical.segments[0].last_frames.images.len(), 2);
    assert_eq!(
        canonical.segments[0].actual_video_last_frame_id,
        Some(actual_id)
    );
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

#[test]
fn native_store_is_owner_scoped_atomic_and_round_trips_schema_v2() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut saved = project();

    store.save(" alice ", &saved).expect("first save");
    saved.title = "Updated Story".into();
    saved.updated_at_unix_ms = 2;
    store.save("alice", &saved).expect("atomic replacement");

    let page = store.load_page("alice", None, 0).expect("owner page");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.unreadable_count, 0);
    assert_eq!(page.next_cursor, None);
    assert!(store.load_page("bob", None, 40).unwrap().items.is_empty());

    let snapshot = store
        .load_project("alice", &saved.id)
        .expect("stored project");
    let value = serde_json::to_value(snapshot).unwrap();
    assert_eq!(value["title"], "Updated Story");

    let projects = projects_directory(app_data.path(), "alice");
    assert!(projects.join(&saved.id).join("project.json").is_file());
    assert!(!projects.join(&saved.id).join("project.json.tmp").exists());
}

#[test]
fn stale_story_cleanup_only_removes_canonical_uncommitted_manifests() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut saved = project();
    let referenced_asset = format!("image-{}.png", Uuid::new_v4().simple());
    saved.props[0]
        .media
        .images
        .push(StoryImage::test_image(&referenced_asset));
    store.save("alice", &saved).unwrap();
    let project_directory = projects_directory(app_data.path(), "alice").join(&saved.id);
    let runs_directory = project_directory.join("runs");
    fs::create_dir(&runs_directory).unwrap();

    let project_temporary = project_directory.join("project.json.tmp");
    let batch_temporary = project_directory.join(format!(
        "media-batch-{}.json.tmp",
        Uuid::new_v4().to_string().to_lowercase()
    ));
    let run_temporary = runs_directory.join(format!(
        "{}.json.tmp",
        Uuid::new_v4().to_string().to_lowercase()
    ));
    let project_shared_temporary = project_directory.join(format!(
        ".story-json-{}.tmp",
        Uuid::new_v4().to_string().to_lowercase()
    ));
    let run_shared_temporary = runs_directory.join(format!(
        ".story-json-{}.tmp",
        Uuid::new_v4().to_string().to_lowercase()
    ));
    let unrelated = project_directory.join("notes.json.tmp");
    let invalid_batch = project_directory.join("media-batch-not-a-uuid.json.tmp");
    let invalid_run = runs_directory.join("not-a-uuid.json.tmp");
    let orphan_asset = project_directory.join(format!("image-{}.png", Uuid::new_v4().simple()));
    let referenced_asset = project_directory.join(&referenced_asset);
    for path in [
        &project_temporary,
        &batch_temporary,
        &run_temporary,
        &project_shared_temporary,
        &run_shared_temporary,
        &unrelated,
        &invalid_batch,
        &invalid_run,
        &orphan_asset,
        &referenced_asset,
    ] {
        fs::write(path, b"value").unwrap();
    }

    assert_eq!(
        store.cleanup_stale_temporaries_before(std::time::UNIX_EPOCH),
        0
    );
    assert_eq!(
        store.cleanup_stale_temporaries_before(
            std::time::SystemTime::now() + std::time::Duration::from_secs(1),
        ),
        6
    );
    assert!(!project_temporary.exists());
    assert!(!batch_temporary.exists());
    assert!(!run_temporary.exists());
    assert!(!project_shared_temporary.exists());
    assert!(!run_shared_temporary.exists());
    assert!(!orphan_asset.exists());
    assert!(referenced_asset.exists());
    assert!(project_directory.join("project.json").exists());
    assert!(unrelated.exists());
    assert!(invalid_batch.exists());
    assert!(invalid_run.exists());
}

#[test]
fn catalog_page_is_stable_newest_first_and_counts_unreadable_manifests() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut older = project();
    older.id = "00000000-0000-4000-8000-000000000001".into();
    older.title = "Older".into();
    older.updated_at_unix_ms = 10;
    store.save("alice", &older).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut newer = project();
    newer.id = "00000000-0000-4000-8000-000000000002".into();
    newer.title = "Newer".into();
    newer.updated_at_unix_ms = 20;
    store.save("alice", &newer).unwrap();

    let first = store.load_page("alice", None, 1).unwrap();
    assert_eq!(first.items.len(), 1);
    assert!(first.next_cursor.is_some());
    let first_json = serde_json::to_value(&first.items[0]).unwrap();
    assert_eq!(first_json["id"], newer.id);
    let second = store
        .load_page("alice", first.next_cursor.clone(), 1)
        .unwrap();
    assert_eq!(second.items.len(), 1);
    let second_json = serde_json::to_value(&second.items[0]).unwrap();
    assert_eq!(second_json["id"], older.id);

    let broken_id = "00000000-0000-4000-8000-000000000003";
    let broken = projects_directory(app_data.path(), "alice").join(broken_id);
    fs::create_dir(&broken).unwrap();
    fs::write(broken.join("project.json"), b"not-json").unwrap();
    let all = store.load_page("alice", None, 200).unwrap();
    assert_eq!(all.items.len(), 2);
    assert_eq!(all.unreadable_count, 1);
}

#[test]
fn safe_projection_never_serializes_native_or_provider_media_identity() {
    let mut value = project();
    let mut image = StoryImage::test_image("private-image.png");
    image.provider_result_id = Some("provider-result-secret".into());
    image.provider_asset_id = Some("provider-asset-secret".into());
    image.derived_from_video_job_id = Some("provider-tail-job-secret".into());
    value.props[0].media.images.push(image);
    value.segments[0].attempt = Some(StoryVideoAttempt {
        id: Uuid::new_v4().to_string(),
        model_config_id: "video".into(),
        prompt: "prompt".into(),
        size: "768P".into(),
        ratio: "16:9".into(),
        seconds: 15,
        job_id: Some("provider-job-secret".into()),
        status: "processing".into(),
        created_at_unix_ms: 1,
    });
    value.segments[0].video = Some(StoryVideo {
        filename: "private-video.mp4".into(),
        job_id: "completed-provider-job-secret".into(),
        model_name: "video-model".into(),
    });

    let json = serde_json::to_string(&StoryProjectSnapshot::from_project(&value)).unwrap();
    for forbidden in [
        "filename",
        "providerResultID",
        "providerAssetID",
        "jobID",
        "base64",
        "private-image.png",
        "private-video.mp4",
        "provider-result-secret",
        "provider-asset-secret",
        "provider-job-secret",
        "completed-provider-job-secret",
    ] {
        assert!(!json.contains(forbidden), "leaked {forbidden}: {json}");
    }
    assert!(json.contains("\"contentToken\":null"));
    assert!(json.contains("\"previewUrl\":null"));
}

#[cfg(unix)]
#[test]
fn native_store_rejects_symlinked_account_hierarchy() {
    use std::os::unix::fs::symlink;

    let app_data = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    symlink(
        outside.path(),
        app_data.path().join("StoryStudio").join("Accounts"),
    )
    .unwrap();

    assert!(store.save("alice", &project()).is_err());
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

#[test]
fn story_media_tokens_are_owner_generation_and_presentation_scoped() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut saved = project();
    let image = StoryImage::test_image("private-image.png");
    saved.props[0].media.images.push(image.clone());
    store.save("alice", &saved).unwrap();
    let asset = projects_directory(app_data.path(), "alice")
        .join(&saved.id)
        .join(&image.filename);
    fs::write(&asset, b"private-image-bytes").unwrap();

    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    let snapshot = manager
        .present_project("alice", 7, &presentation, &saved.id)
        .unwrap();
    let value = serde_json::to_value(snapshot).unwrap();
    let media = &value["props"][0]["media"]["images"][0];
    let token = media["contentToken"].as_str().unwrap();
    let preview_url = media["previewUrl"].as_str().unwrap();
    assert_eq!(token.len(), 32);
    assert!(!preview_url.contains(&image.filename));
    assert!(!preview_url.contains("alice"));

    let response = story_studio_protocol(
        manager.clone(),
        tauri::http::Request::builder()
            .uri(preview_url)
            .body(Vec::new())
            .unwrap(),
    );
    assert_eq!(response.status(), tauri::http::StatusCode::OK);
    assert_eq!(response.body(), b"private-image-bytes");
    assert_eq!(response.headers()["Cache-Control"], "no-store");
    assert_eq!(response.headers()["X-Content-Type-Options"], "nosniff");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = app_data.path().join("outside-image");
        fs::write(&outside, b"must-not-cross-story-protocol").unwrap();
        fs::remove_file(&asset).unwrap();
        symlink(&outside, &asset).unwrap();
        let response = story_studio_protocol(
            manager.clone(),
            tauri::http::Request::builder()
                .uri(preview_url)
                .body(Vec::new())
                .unwrap(),
        );
        assert_eq!(response.status(), tauri::http::StatusCode::NOT_FOUND);
    }

    let wrong_presentation = preview_url.replace(&presentation, &Uuid::new_v4().to_string());
    let response = story_studio_protocol(
        manager.clone(),
        tauri::http::Request::builder()
            .uri(wrong_presentation)
            .body(Vec::new())
            .unwrap(),
    );
    assert_eq!(response.status(), tauri::http::StatusCode::NOT_FOUND);

    manager
        .discard_presentation("alice", 7, &presentation)
        .unwrap();
    let response = story_studio_protocol(
        manager.clone(),
        tauri::http::Request::builder()
            .uri(preview_url)
            .body(Vec::new())
            .unwrap(),
    );
    assert_eq!(response.status(), tauri::http::StatusCode::NOT_FOUND);

    let replacement_presentation = Uuid::new_v4().to_string();
    manager
        .present_page("alice", 8, &replacement_presentation, None, 40)
        .unwrap();
    let response = story_studio_protocol(
        manager,
        tauri::http::Request::builder()
            .uri(preview_url)
            .body(Vec::new())
            .unwrap(),
    );
    assert_eq!(response.status(), tauri::http::StatusCode::UNAUTHORIZED);
}

#[test]
fn safe_creation_input_builds_the_canonical_macos_project_without_provider_work() {
    let input: StoryProjectCreationInput = serde_json::from_value(serde_json::json!({
        "title": "  New Story  ",
        "description": "  Description  ",
        "models": {
            "textModelId": "text",
            "imageModelId": "image",
            "videoModelId": "video"
        }
    }))
    .unwrap();
    let models = story_models();
    let value = make_story_project(input, "alice", &models, 42).unwrap();

    assert_eq!(value.title, "New Story");
    assert_eq!(value.description, "Description");
    assert_eq!(value.style, default_story_style());
    assert_eq!(value.ratio, "16:9");
    assert_eq!(
        value.models.supported_video_durations,
        Some((4..=15).collect())
    );
    assert!(value.source.is_empty());
    assert!(value.segments.is_empty());
    assert_eq!(value.created_at_unix_ms, 42);
    assert_eq!(value.updated_at_unix_ms, 42);

    let encoded = serde_json::to_string(&StoryProjectSnapshot::from_project(&value)).unwrap();
    assert!(!encoded.contains("must-not-cross"));
    assert!(!encoded.contains("apiKey"));

    let unsafe_shape = serde_json::from_value::<StoryProjectCreationInput>(serde_json::json!({
        "title": "Story",
        "description": "Description",
        "source": "React must not submit persisted state",
        "models": {
            "textModelId": "text",
            "imageModelId": "image",
            "videoModelId": "video"
        }
    }));
    assert!(unsafe_shape.is_err());
}

#[test]
fn settings_rebuild_from_canonical_project_and_freeze_unresolved_model_identity() {
    let models = story_models();
    let mut current = project();
    current.source = "canonical source".into();
    current.style = "canonical style".into();
    current.summary = "canonical summary".into();
    let project_id = current.id.clone();
    apply_story_project_settings(
        &mut current,
        StoryProjectSettingsInput {
            project_id: project_id.clone(),
            title: "  Updated Story  ".into(),
            description: "updated description".into(),
            models: StoryModelSelectionInput {
                text_model_id: "video".into(),
                image_model_id: "image".into(),
                video_model_id: "video".into(),
            },
        },
        "alice",
        &models,
        10,
    )
    .unwrap();
    assert_eq!(current.title, "Updated Story");
    assert_eq!(current.source, "canonical source");
    assert_eq!(current.style, "canonical style");
    assert_eq!(current.summary, "canonical summary");
    assert_eq!(current.id, project_id);

    current.props[0].media.generation_attempt_id = Some(Uuid::new_v4().to_string());
    let result = apply_story_project_settings(
        &mut current,
        StoryProjectSettingsInput {
            project_id,
            title: "Updated Story".into(),
            description: "updated description".into(),
            models: StoryModelSelectionInput {
                text_model_id: "text".into(),
                image_model_id: "text".into(),
                video_model_id: "video".into(),
            },
        },
        "alice",
        &models,
        11,
    );
    assert!(result.is_err());
    assert_eq!(current.models.image_model_id, "image");
}

#[test]
fn source_edit_is_fingerprint_guarded_and_preserves_native_only_state() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.segments.clear();
    let mut image = StoryImage::test_image("private-image.png");
    image.provider_result_id = Some("provider-result-secret".into());
    image.provider_asset_id = Some("provider-asset-secret".into());
    canonical.props[0].media.images.push(image.clone());
    store.save("alice", &canonical).unwrap();

    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    let initial = manager
        .present_project("alice", 7, &presentation, &canonical.id)
        .unwrap();
    let initial_json = serde_json::to_value(initial).unwrap();
    let base_fingerprint = initial_json["baseFingerprint"]
        .as_str()
        .expect("live project fingerprint")
        .to_owned();

    let saved = manager
        .update_project_source(
            "alice",
            7,
            &presentation,
            StoryProjectSourceInput {
                project_id: canonical.id.clone(),
                base_fingerprint: base_fingerprint.clone(),
                source: "replacement source".into(),
                style: "replacement style".into(),
                ratio: "9:16".into(),
            },
        )
        .unwrap();
    let saved_json = serde_json::to_value(saved).unwrap();
    assert_eq!(saved_json["source"], "replacement source");
    assert_eq!(saved_json["style"], "replacement style");
    assert_eq!(saved_json["ratio"], "9:16");
    assert_ne!(saved_json["baseFingerprint"], base_fingerprint);

    let persisted = store.load_native_project("alice", &canonical.id).unwrap();
    assert_eq!(persisted.props[0].media.images[0], image);
    assert_eq!(persisted.title, canonical.title);
    assert_eq!(persisted.models, canonical.models);
    assert_eq!(persisted.created_at_unix_ms, canonical.created_at_unix_ms);

    let stale = manager.update_project_source(
        "alice",
        7,
        &presentation,
        StoryProjectSourceInput {
            project_id: canonical.id.clone(),
            base_fingerprint,
            source: "must not win".into(),
            style: "must not win".into(),
            ratio: "1:1".into(),
        },
    );
    assert!(stale.is_err());
    assert_eq!(
        store
            .load_native_project("alice", &canonical.id)
            .unwrap()
            .source,
        "replacement source"
    );

    assert!(
        serde_json::from_value::<StoryProjectSourceInput>(serde_json::json!({
            "projectId": canonical.id,
            "baseFingerprint": saved_json["baseFingerprint"],
            "source": "source",
            "style": "style",
            "ratio": "16:9",
            "characters": [],
            "providerResultId": "React must not submit native state"
        }))
        .is_err()
    );
}

#[test]
fn source_edit_rejects_existing_outline_and_retired_presentation_generation() {
    let mut outlined = project();
    let fingerprint = project_fingerprint(&outlined).unwrap();
    let outlined_id = outlined.id.clone();
    let result = apply_story_project_source(
        &mut outlined,
        StoryProjectSourceInput {
            project_id: outlined_id,
            base_fingerprint: fingerprint,
            source: "must not replace segment ranges".into(),
            style: "style".into(),
            ratio: "16:9".into(),
        },
        2,
    );
    assert!(result.is_err());
    assert_eq!(outlined.source, "x");

    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.segments.clear();
    store.save("alice", &canonical).unwrap();
    let manager = StoryStudioManager::new(app_data.path().to_path_buf()).unwrap();
    let presentation = Uuid::new_v4().to_string();
    manager
        .present_project("alice", 7, &presentation, &canonical.id)
        .unwrap();
    manager.reset_session_if_matches("alice", 7);
    let result = manager.update_project_source(
        "alice",
        7,
        &presentation,
        StoryProjectSourceInput {
            project_id: canonical.id.clone(),
            base_fingerprint: project_fingerprint(&canonical).unwrap(),
            source: "must not survive stale generation".into(),
            style: canonical.style.clone(),
            ratio: canonical.ratio.clone(),
        },
    );
    assert!(result.is_err());
    assert_eq!(
        store
            .load_native_project("alice", &canonical.id)
            .unwrap()
            .source,
        canonical.source
    );
}
