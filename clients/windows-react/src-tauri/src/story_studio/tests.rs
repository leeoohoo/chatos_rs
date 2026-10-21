use super::domain::*;
use super::{projection::StoryProjectSnapshot, store::StoryProjectStore};
use sha2::{Digest, Sha256};
use std::fs;
use tempfile::tempdir;
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

fn projects_directory(app_data: &std::path::Path, owner: &str) -> std::path::PathBuf {
    let owner_hash = format!("{:x}", Sha256::digest(owner.as_bytes()));
    app_data
        .join("StoryStudio")
        .join("Accounts")
        .join(owner_hash)
        .join("Projects")
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
