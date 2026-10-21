use super::domain::*;
use super::{
    application::{
        apply_story_project_settings, apply_story_project_source, default_story_style,
        make_story_project, StoryAssetImageGenerationInput, StoryFrameImageGenerationInput,
        StoryFrameRoleInput, StoryImageConfirmationInput, StoryImageImportInput,
        StoryLastFrameClearInput, StoryLatestFramesInput, StoryLatestResourceImagesInput,
        StoryModelSelectionInput, StoryProjectCreationInput, StoryProjectSettingsInput,
        StoryProjectSourceInput, StoryResourcePromptInput, StorySegmentEditInput,
        StorySegmentStructureInput, StorySegmentStructureMutation, StorySegmentStructureOperation,
    },
    execution::StoryPlanningExecutionRegistry,
    media::{story_studio_protocol, StoryStudioManager},
    memory::{assemble_memory_input, bind_memory, memory_records, synced_digest},
    planning::{
        project_fingerprint, StoryAgentMessage, StoryAgentMessageRole, StoryAgentReceipt,
        StoryAgentRunPolicy, StoryAgentToolCall, StoryAgentToolOutcome, StoryPlanningRun,
        StoryPlanningStage, StoryPlanningStatus,
    },
    projection::StoryProjectSnapshot,
    store::StoryProjectStore,
    tools,
};
use crate::authentication::GatewayModelConfig;
use memory_engine_sdk::{
    ComposeContextBlock, ComposeContextMeta, ComposeContextResponse, EngineRecord,
};
use sha2::{Digest, Sha256};
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

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
fn model_admission_requires_current_owner_enabled_model_and_credential_status() {
    let input = StoryProjectCreationInput {
        title: "Story".into(),
        description: "Description".into(),
        models: StoryModelSelectionInput {
            text_model_id: "text".into(),
            image_model_id: "image".into(),
            video_model_id: "video".into(),
        },
    };
    let mut models = story_models();
    models[1].has_api_key = false;
    assert!(make_story_project(input.clone(), "alice", &models, 1).is_err());

    let mut models = story_models();
    models[1].enabled = false;
    assert!(make_story_project(input.clone(), "alice", &models, 1).is_err());

    let models = story_models();
    assert!(make_story_project(input.clone(), "bob", &models, 1).is_err());

    assert!(models.iter().all(|model| model.task_enabled == Some(false)));
    assert!(make_story_project(input, "alice", &models, 1).is_ok());
}

#[test]
fn planning_checkpoint_is_native_only_and_safe_snapshot_omits_provider_and_tool_payloads() {
    let mut canonical = project();
    canonical.source = "A complete source story".into();
    canonical.segments.clear();
    canonical.relations.clear();
    let mut captured_ideas = String::new();
    let ideas = format!("  {}  ", "想".repeat(2_020));
    let mut run = StoryPlanningRun::new(
        &canonical,
        " alice ",
        StoryPlanningStage::Outline,
        Vec::new(),
        &ideas,
        StoryAgentRunPolicy::default(),
        "private-system-prompt".into(),
        |value| {
            captured_ideas = value.to_owned();
            format!("private-goal:{value}")
        },
        100,
    )
    .unwrap();
    assert_eq!(captured_ideas.chars().count(), 2_000);
    run.checkpoint.pending_calls.push(StoryAgentToolCall {
        id: "call-1".into(),
        name: "story_private_tool".into(),
        arguments: "provider-tool-arguments-must-not-cross".into(),
    });
    run.tool_receipts.insert(
        "call-1".into(),
        StoryAgentReceipt {
            name: "story_private_tool".into(),
            arguments: "receipt-arguments-must-not-cross".into(),
            outcome: StoryAgentToolOutcome {
                content: "private-tool-outcome".into(),
                made_progress: true,
                is_error: false,
            },
        },
    );
    run.checkpoint.messages[0].response_output_json =
        Some(b"provider-response-output-must-not-cross".to_vec());

    let snapshot = serde_json::to_string(&run.snapshot("alice")).unwrap();
    for forbidden in [
        "private-system-prompt",
        "private-goal",
        "provider-tool-arguments-must-not-cross",
        "receipt-arguments-must-not-cross",
        "private-tool-outcome",
        "provider-response-output-must-not-cross",
        "pendingCalls",
        "toolReceipts",
        "messages",
    ] {
        assert!(
            !snapshot.contains(forbidden),
            "leaked {forbidden}: {snapshot}"
        );
    }
    assert!(snapshot.contains("baseProjectFingerprint"));
    assert!(snapshot.contains("usesCloudMemory"));
}

#[test]
fn story_memory_contract_binds_syncs_and_composes_only_the_current_run() {
    let mut canonical = project();
    canonical.source = "A complete source story".into();
    canonical.segments.clear();
    canonical.relations.clear();
    let mut run = StoryPlanningRun::new(
        &canonical,
        "alice",
        StoryPlanningStage::Outline,
        Vec::new(),
        "ideas",
        StoryAgentRunPolicy::default(),
        "private system".into(),
        |ideas| format!("goal:{ideas}"),
        1_700_000_000_000,
    )
    .unwrap();
    bind_memory(&mut run, "alice", 1_700_000_000_000).unwrap();
    let scope = run.checkpoint.memory.as_ref().unwrap().scope.clone();
    assert_eq!(
        scope.thread_id,
        format!("client-agent:story:{}:{}", canonical.id, run.id())
    );
    assert_eq!(
        scope.subject_id,
        format!("client-agent:story:{}", canonical.id)
    );
    assert!(run.uses_cloud_memory);

    run.checkpoint.messages.push(StoryAgentMessage {
        role: StoryAgentMessageRole::Assistant,
        content: String::new(),
        tool_calls: vec![StoryAgentToolCall {
            id: "call-1".into(),
            name: "story_read_state".into(),
            arguments: "{}".into(),
        }],
        tool_call_id: None,
        response_output_json: Some(serde_json::to_vec(&vec![serde_json::json!({
            "type": "function_call", "call_id": "call-1", "name": "story_read_state", "arguments": "{}"
        })]).unwrap()),
        usage: None,
    });
    run.checkpoint.messages.push(StoryAgentMessage {
        role: StoryAgentMessageRole::Tool,
        content: "state-result".into(),
        tool_calls: Vec::new(),
        tool_call_id: Some("call-1".into()),
        response_output_json: None,
        usage: None,
    });
    let records = memory_records(&run, 0, run.checkpoint.messages.len()).unwrap();
    let end = run.checkpoint.messages.len();
    let digest = synced_digest(&run, end).unwrap();
    let memory = run.checkpoint.memory.as_mut().unwrap();
    memory.thread_created = true;
    memory.synced_message_count = end;
    memory.synced_digest = digest;
    let recent_records = records
        .into_iter()
        .map(|record| EngineRecord {
            id: record.id,
            thread_id: scope.thread_id.clone(),
            tenant_id: "alice".into(),
            source_id: scope.source_id.clone(),
            external_record_id: record.external_record_id,
            role: record.role,
            record_type: record.record_type,
            content: record.content,
            structured_payload: record.structured_payload,
            metadata: record.metadata,
            summary_status: "pending".into(),
            summary_id: None,
            summarized_at: None,
            created_at: record.created_at,
        })
        .collect::<Vec<_>>();
    let response = ComposeContextResponse {
        thread_id: scope.thread_id.clone(),
        blocks: vec![ComposeContextBlock {
            block_type: "thread_summary".into(),
            text: "summary".into(),
        }],
        meta: ComposeContextMeta {
            summary_count: 1,
            recent_record_count: recent_records.len(),
        },
        recent_records,
    };
    let input = assemble_memory_input(&run, &response).unwrap();
    assert_eq!(input[0]["role"], "system");
    assert!(input.iter().any(|item| item["type"] == "function_call"));
    assert!(input.iter().any(|item| {
        item["type"] == "function_call_output" && item["output"] == "state-result"
    }));
    assert_eq!(input.last().unwrap()["content"], "goal:ideas");
    assert!(input.iter().all(|item| item["content"] != "private system"));

    let mut foreign = response;
    foreign.recent_records[0].tenant_id = "bob".into();
    assert!(assemble_memory_input(&run, &foreign).is_err());
    assert!(run.validate(&run.owner_hash.clone(), &canonical.id).is_ok());
}

#[test]
fn planning_fingerprint_ignores_revision_and_video_capability_snapshot() {
    let mut original = project();
    original.models.supported_video_durations = Some(vec![4, 8, 12]);
    let mut refreshed = original.clone();
    refreshed.updated_at_unix_ms = 9_999;
    refreshed.models.supported_video_durations = Some((4..=15).collect());

    assert_eq!(
        project_fingerprint(&original).unwrap(),
        project_fingerprint(&refreshed).unwrap()
    );
}

#[test]
fn refine_planning_archives_completed_video_only_inside_the_run_draft() {
    let mut canonical = project();
    canonical.source = "x".into();
    canonical.segments[0].video = Some(StoryVideo {
        filename: "completed.mp4".into(),
        job_id: "provider-job".into(),
        model_name: "video".into(),
    });
    canonical.segments[0].attempt = Some(StoryVideoAttempt {
        id: Uuid::new_v4().to_string(),
        model_config_id: "video".into(),
        prompt: "prompt".into(),
        size: "768P".into(),
        ratio: "16:9".into(),
        seconds: 15,
        job_id: Some("provider-job".into()),
        status: "completed".into(),
        created_at_unix_ms: 1,
    });
    let run = StoryPlanningRun::new(
        &canonical,
        "alice",
        StoryPlanningStage::Refine,
        vec!["s1".into()],
        "",
        StoryAgentRunPolicy::default(),
        "system".into(),
        |_| "goal".into(),
        100,
    )
    .unwrap();

    assert!(canonical.segments[0].video.is_some());
    assert!(canonical.segments[0].archived_videos.is_empty());
    assert!(run.draft.segments[0].video.is_none());
    assert!(run.draft.segments[0].attempt.is_none());
    assert_eq!(run.draft.segments[0].archived_videos.len(), 1);
    assert_eq!(run.draft.segments[0].previous_attempts.len(), 1);
    assert_eq!(run.id(), run.checkpoint.id);
}

#[test]
fn planning_checkpoint_store_is_atomic_owner_scoped_and_strictly_decoded() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.source = "A complete source story".into();
    canonical.segments.clear();
    canonical.relations.clear();
    store.save("alice", &canonical).unwrap();
    let run = StoryPlanningRun::new(
        &canonical,
        "alice",
        StoryPlanningStage::Outline,
        Vec::new(),
        "ideas",
        StoryAgentRunPolicy::default(),
        "system".into(),
        |ideas| format!("goal:{ideas}"),
        100,
    )
    .unwrap();

    store.save_planning_run("alice", &run).unwrap();
    let loaded = store
        .load_planning_run("alice", &canonical.id, run.id())
        .unwrap();
    assert_eq!(loaded, run);
    assert!(store
        .load_planning_run("bob", &canonical.id, run.id())
        .is_err());

    let path = projects_directory(app_data.path(), "alice")
        .join(&canonical.id)
        .join("runs")
        .join(format!("{}.json", run.id()));
    assert!(!path.with_extension("json.tmp").exists());
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("providerSecret".into(), serde_json::json!("must-not-cross"));
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(store
        .load_planning_run("alice", &canonical.id, run.id())
        .is_err());
    assert!(path.exists(), "an unreadable checkpoint must be preserved");
}

#[test]
fn planning_history_is_owner_scoped_bounded_and_safe() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.source = "A complete source story".into();
    canonical.segments.clear();
    canonical.relations.clear();
    store.save("alice", &canonical).unwrap();
    for index in 0..3 {
        let mut run = StoryPlanningRun::new(
            &canonical,
            "alice",
            StoryPlanningStage::Outline,
            Vec::new(),
            "ideas",
            StoryAgentRunPolicy::default(),
            format!("private-system-{index}"),
            |_| format!("private-goal-{index}"),
            100 + index,
        )
        .unwrap();
        run.updated_at_unix_ms = 100 + index;
        store.save_planning_run("alice", &run).unwrap();
    }

    let first = store
        .load_planning_history("alice", &canonical.id, None, 2)
        .unwrap();
    assert_eq!(first.runs.len(), 2);
    assert!(first.next_cursor.is_some());
    let second = store
        .load_planning_history("alice", &canonical.id, first.next_cursor.clone(), 2)
        .unwrap();
    assert_eq!(second.runs.len(), 1);
    assert!(second.next_cursor.is_none());
    let ids = [&first, &second]
        .into_iter()
        .flat_map(|page| {
            serde_json::to_value(page).unwrap()["runs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|run| run["id"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 3);
    let safe = serde_json::to_string(&first).unwrap();
    assert!(!safe.contains("private-system"));
    assert!(!safe.contains("private-goal"));
    assert!(!safe.contains("messages"));
    assert!(store
        .load_planning_history("bob", &canonical.id, None, 20)
        .is_err());
}

#[test]
fn native_story_tool_registry_is_stage_scoped_and_never_exposes_media_generation() {
    let outline = tools::definitions(StoryPlanningStage::Outline);
    let refine = tools::definitions(StoryPlanningStage::Refine);
    let outline_names = outline.iter().map(|value| value.name).collect::<Vec<_>>();
    let refine_names = refine.iter().map(|value| value.name).collect::<Vec<_>>();

    for required in [
        "story_read_source",
        "story_read_text",
        "story_read_assets",
        "story_read_asset",
        "story_read_asset_prompt",
        "story_read_segments",
        "story_read_segment",
        "story_read_graph",
        "story_save_segment_relations",
        "story_finish",
    ] {
        assert!(outline_names.contains(&required));
        assert!(refine_names.contains(&required));
    }
    assert!(outline_names.contains(&"story_save_character_profile"));
    assert!(outline_names.contains(&"story_save_scene_profile"));
    assert!(outline_names.contains(&"story_append_segments"));
    assert!(!outline_names.contains(&"story_update_segment"));
    assert!(refine_names.contains(&"story_update_segment"));
    assert!(!refine_names.contains(&"story_append_segments"));
    assert!(outline_names
        .iter()
        .chain(refine_names.iter())
        .all(|name| !name.contains("image_generation") && !name.contains("video_generation")));
    assert!(outline
        .iter()
        .chain(refine.iter())
        .all(
            |definition| definition.provider_value()["parameters"]["additionalProperties"] == false
        ));
}

#[test]
fn outline_tools_enforce_sequential_source_coverage_and_explicit_relations() {
    let mut canonical = project();
    canonical.source = "甲乙丙".into();
    canonical.segments.clear();
    canonical.props.clear();
    let mut run = StoryPlanningRun::new(
        &canonical,
        "alice",
        StoryPlanningStage::Outline,
        Vec::new(),
        "",
        StoryAgentRunPolicy::default(),
        "system".into(),
        |_| "goal".into(),
        1,
    )
    .unwrap();

    let skipped = execute_story_tool(
        &mut run,
        "skip",
        "story_read_source",
        serde_json::json!({"offset": 1, "limit": 2}),
    );
    assert!(skipped.is_error);
    assert_eq!(run.read_through, 0);

    let source_outcome = execute_story_tool(
        &mut run,
        "read",
        "story_read_source",
        serde_json::json!({"offset": 0, "limit": 3}),
    );
    assert!(!source_outcome.is_error, "{}", source_outcome.content);
    assert_eq!(run.read_through, 3);
    let summary_outcome = execute_story_tool(
        &mut run,
        "summary",
        "story_save_summary",
        serde_json::json!({"summary": "完整摘要"}),
    );
    assert!(!summary_outcome.is_error, "{}", summary_outcome.content);
    assert!(
        !execute_story_tool(
            &mut run,
            "scene",
            "story_save_scene_profile",
            serde_json::json!({
                "id": "room", "name": "房间",
                "profile": {
                    "roleInStory": "主要场景", "setting": "室内", "spatialLayout": "方形房间",
                    "lightingAndPalette": "暖光", "keyElements": "木桌", "atmosphere": "安静",
                    "consistencyNotes": "布局固定"
                }
            }),
        )
        .is_error
    );
    assert!(
        !execute_story_tool(
            &mut run,
            "segments",
            "story_append_segments",
            serde_json::json!({"segments": [{
                "id": "segment-1", "title": "第一段", "synopsis": "覆盖全文",
                "kind": "story", "seconds": 5, "sourceStart": 0, "sourceEnd": 3
            }]}),
        )
        .is_error
    );
    let finish_before_relations = execute_story_tool(
        &mut run,
        "finish-before",
        "story_finish",
        serde_json::json!({}),
    );
    assert!(finish_before_relations.is_error);
    let relations_outcome = execute_story_tool(
        &mut run,
        "relations",
        "story_save_segment_relations",
        serde_json::json!({
            "segmentID": "segment-1", "characterIDs": [], "sceneIDs": ["room"],
            "propIDs": [], "relations": []
        }),
    );
    assert!(!relations_outcome.is_error, "{}", relations_outcome.content);
    assert!(
        !execute_story_tool(&mut run, "finish", "story_finish", serde_json::json!({}),).is_error
    );
}

#[test]
fn refine_tool_requires_a_durable_segment_read_receipt() {
    let mut canonical = project();
    canonical.scenes.push(StoryScene {
        id: "room".into(),
        name: "Room".into(),
        profile: StorySceneProfile {
            role_in_story: "Setting".into(),
            setting: "Room".into(),
            spatial_layout: "Square".into(),
            lighting_and_palette: "Warm".into(),
            key_elements: "Table".into(),
            atmosphere: "Quiet".into(),
            consistency_notes: "Stable".into(),
        },
        image_prompt: "Room\nSquare\nWarm\nTable\nQuiet\nStable".into(),
        media: StoryImageCollection::default(),
    });
    canonical.segments[0].scene_ids = vec!["room".into()];
    let mut run = StoryPlanningRun::new(
        &canonical,
        "alice",
        StoryPlanningStage::Refine,
        vec!["s1".into()],
        "",
        StoryAgentRunPolicy::default(),
        "system".into(),
        |_| "goal".into(),
        1,
    )
    .unwrap();
    let detail = serde_json::json!({
        "segmentID": "s1",
        "detail": {
            "firstFramePrompt": "房间中的静态开场", "lastFramePrompt": "房间中的结束状态",
            "shots": [{"start": 0, "end": 15, "prompt": "全景连续动作"}],
            "continuityIn": "开场", "continuityOut": "结束", "audio": "环境声", "constraints": "保持布局"
        }
    });
    assert!(
        execute_story_tool(
            &mut run,
            "update-before",
            "story_update_segment",
            detail.clone()
        )
        .is_error
    );
    let read_outcome = execute_story_tool(
        &mut run,
        "read-segment",
        "story_read_segment",
        serde_json::json!({"segmentID": "s1"}),
    );
    assert!(!read_outcome.is_error, "{}", read_outcome.content);
    assert!(!execute_story_tool(&mut run, "update", "story_update_segment", detail).is_error);
    assert!(tools::validate_completion(&run).is_ok());
}

#[test]
fn planning_execution_registry_is_single_flight_and_cancels_page_teardown() {
    let registry = StoryPlanningExecutionRegistry::default();
    let presentation = Uuid::new_v4().to_string();
    let project = Uuid::new_v4().to_string();
    let run = Uuid::new_v4().to_string();
    let lease = registry
        .begin("alice", 7, &presentation, &project, &run)
        .unwrap();
    assert!(registry
        .begin("alice", 7, &presentation, &project, &run)
        .is_err());
    assert!(registry.accepts(&lease));
    registry.cancel_presentation("alice", 7, &presentation);
    assert!(!registry.accepts(&lease));
    registry.finish(&lease);
    assert!(registry
        .begin(
            "alice",
            7,
            &presentation,
            &project,
            &Uuid::new_v4().to_string()
        )
        .is_ok());
}

#[test]
fn completed_planning_run_applies_only_over_its_unchanged_canonical_base() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.source = "x".into();
    store.save("alice", &canonical).unwrap();
    let mut run = StoryPlanningRun::new(
        &canonical,
        "alice",
        StoryPlanningStage::Refine,
        vec!["s1".into()],
        "",
        StoryAgentRunPolicy::default(),
        "system".into(),
        |_| "goal".into(),
        1,
    )
    .unwrap();
    run.draft.title = "Applied Draft".into();
    run.checkpoint.status = StoryPlanningStatus::Completed;
    run.checkpoint.result = Some("done".into());
    store.save_planning_run("alice", &run).unwrap();

    let (applied, saved) = store.apply_planning_run("alice", &run, 9).unwrap();
    assert!(applied.applied);
    assert_eq!(saved.title, "Applied Draft");
    assert_eq!(
        store.load_native_project("alice", &canonical.id).unwrap(),
        saved
    );

    let mut changed = canonical.clone();
    changed.title = "Concurrent User Edit".into();
    changed.updated_at_unix_ms = 10;
    store.save("alice", &changed).unwrap();
    let mut second = run.clone();
    second.checkpoint.id = Uuid::new_v4().to_string();
    second.applied = false;
    assert!(store.apply_planning_run("alice", &second, 11).is_err());
    assert_eq!(
        store
            .load_native_project("alice", &canonical.id)
            .unwrap()
            .title,
        "Concurrent User Edit"
    );
}

#[test]
fn abandoned_planning_run_rejects_a_late_runtime_checkpoint() {
    let app_data = tempdir().unwrap();
    let store = StoryProjectStore::for_tests(app_data.path().to_path_buf()).unwrap();
    let mut canonical = project();
    canonical.source = "x".into();
    store.save("alice", &canonical).unwrap();
    let run = StoryPlanningRun::new(
        &canonical,
        "alice",
        StoryPlanningStage::Refine,
        vec!["s1".into()],
        "",
        StoryAgentRunPolicy::default(),
        "system".into(),
        |_| "goal".into(),
        1,
    )
    .unwrap();
    store.save_planning_run("alice", &run).unwrap();
    let stale = run.clone();
    let mut abandoned = run;
    abandoned.abandoned_at_unix_ms = Some(2);
    abandoned.checkpoint.stop_reason = Some("abandoned".into());
    store.save_planning_run("alice", &abandoned).unwrap();

    assert!(store.save_planning_run("alice", &stale).is_err());
    assert_eq!(
        store
            .load_planning_run("alice", &canonical.id, abandoned.id())
            .unwrap()
            .abandoned_at_unix_ms,
        Some(2)
    );
}

fn execute_story_tool(
    run: &mut StoryPlanningRun,
    id: &str,
    name: &str,
    arguments: serde_json::Value,
) -> StoryAgentToolOutcome {
    let call = StoryAgentToolCall {
        id: id.into(),
        name: name.into(),
        arguments: serde_json::to_string(&arguments).unwrap(),
    };
    let outcome = tools::execute(&call, run);
    run.tool_receipts.insert(
        id.into(),
        StoryAgentReceipt {
            name: name.into(),
            arguments: call.arguments,
            outcome: outcome.clone(),
        },
    );
    outcome
}

fn projects_directory(app_data: &std::path::Path, owner: &str) -> std::path::PathBuf {
    let owner_hash = format!("{:x}", Sha256::digest(owner.as_bytes()));
    app_data
        .join("StoryStudio")
        .join("Accounts")
        .join(owner_hash)
        .join("Projects")
}

fn story_models() -> Vec<GatewayModelConfig> {
    [
        ("text", "gpt-5"),
        ("image", "gpt-image-1"),
        ("video", "MiniMax-H3"),
    ]
    .into_iter()
    .map(|(id, model)| GatewayModelConfig {
        id: id.into(),
        owner_user_id: "alice".into(),
        name: id.into(),
        provider: "openai".into(),
        model: model.into(),
        api_key: Some("must-not-cross".into()),
        base_url: Some("https://provider.example/v1".into()),
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

fn test_segment_detail(seconds: u8) -> StorySegmentDetail {
    StorySegmentDetail {
        first_frame_prompt: "first frame".into(),
        last_frame_prompt: Some("last frame".into()),
        shots: vec![StoryShot {
            start: 0,
            end: seconds,
            prompt: "continuous shot".into(),
        }],
        continuity_in: "continuity in".into(),
        continuity_out: "continuity out".into(),
        audio: "room tone".into(),
        constraints: "keep continuity".into(),
    }
}
