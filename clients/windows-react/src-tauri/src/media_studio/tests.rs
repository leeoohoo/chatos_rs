use super::*;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use provider::{GeneratedImageAsset, ImageGenerationResult};
use std::fs;
use tempfile::tempdir;

fn model(id: &str, owner: &str, name: &str, enabled: bool) -> GatewayModelConfig {
    GatewayModelConfig {
        id: id.into(),
        owner_user_id: owner.into(),
        name: name.into(),
        provider: "openai".into(),
        model: name.into(),
        api_key: Some("must-never-cross".into()),
        base_url: Some("https://provider.example/v1".into()),
        task_thinking_level: None,
        temperature: None,
        max_output_tokens: None,
        model_capability_revision: None,
        context_window_tokens: None,
        output_reserve_tokens: None,
        protocol_overhead_estimator_id: None,
        tokenizer_id: None,
        responses_compaction_threshold: None,
        enabled,
        task_enabled: Some(false),
        has_api_key: true,
        supports_images: true,
        supports_reasoning: false,
        supports_responses: true,
    }
}

#[test]
fn stale_history_cleanup_only_removes_owned_manifest_temporaries() {
    let root = tempdir().unwrap();
    let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
    let owner = history::owner_hash("alice");
    let image_record = manager
        .history
        .temporary_cleanup_root()
        .join(&owner)
        .join(Uuid::new_v4().to_string());
    let video_record = manager
        .video_history
        .temporary_cleanup_root()
        .join(&owner)
        .join(Uuid::new_v4().to_string());
    let incomplete_record = manager
        .history
        .temporary_cleanup_root()
        .join(&owner)
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&image_record).unwrap();
    fs::create_dir_all(&video_record).unwrap();
    fs::create_dir_all(&incomplete_record).unwrap();

    let image_temporary = image_record.join("record.json.tmp");
    let image_owned_temporary = image_record.join(format!(".image-history-{}.tmp", Uuid::new_v4()));
    let video_temporary = video_record.join("record.json.tmp");
    let video_owned_temporary = video_record.join(format!(".video-history-{}.tmp", Uuid::new_v4()));
    let video_manifest = video_record.join("record.json");
    let committed = image_record.join("record.json");
    let unrelated = image_record.join("notes.tmp");
    let incomplete_asset = incomplete_record.join("00000000000000000000000000000001.png");
    for path in [
        &image_temporary,
        &image_owned_temporary,
        &video_temporary,
        &video_owned_temporary,
        &video_manifest,
        &committed,
        &unrelated,
    ] {
        fs::write(path, b"value").unwrap();
    }
    fs::write(&incomplete_asset, b"orphaned asset").unwrap();

    let invalid_owner_temporary = manager
        .history
        .temporary_cleanup_root()
        .join("not-an-owner")
        .join(Uuid::new_v4().to_string())
        .join("record.json.tmp");
    fs::create_dir_all(invalid_owner_temporary.parent().unwrap()).unwrap();
    fs::write(&invalid_owner_temporary, b"value").unwrap();
    let invalid_record_temporary = manager
        .history
        .temporary_cleanup_root()
        .join(&owner)
        .join("not-a-record")
        .join("record.json.tmp");
    fs::create_dir_all(invalid_record_temporary.parent().unwrap()).unwrap();
    fs::write(&invalid_record_temporary, b"value").unwrap();

    assert_eq!(
        cleanup_history_temporaries(
            &manager.history.temporary_cleanup_root(),
            &manager.video_history.temporary_cleanup_root(),
            UNIX_EPOCH,
        ),
        0
    );
    assert_eq!(
        cleanup_history_temporaries(
            &manager.history.temporary_cleanup_root(),
            &manager.video_history.temporary_cleanup_root(),
            SystemTime::now() + Duration::from_secs(1),
        ),
        5
    );
    assert!(!image_temporary.exists());
    assert!(!image_owned_temporary.exists());
    assert!(!video_temporary.exists());
    assert!(!video_owned_temporary.exists());
    assert!(video_manifest.exists());
    assert!(!incomplete_record.exists());
    assert!(committed.exists());
    assert!(unrelated.exists());
    assert!(invalid_owner_temporary.exists());
    assert!(invalid_record_temporary.exists());
}

#[test]
fn stale_workflow_cleanup_only_removes_owned_manifest_temporaries() {
    let root = tempdir().unwrap();
    let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
    let accounts = manager.workflows.temporary_cleanup_root();
    let owner = history::owner_hash("alice");
    let workflow = accounts.join(&owner).join(Uuid::new_v4().to_string());
    fs::create_dir_all(workflow.join("assets")).unwrap();

    let owned_temporary = workflow.join(format!(".workflow-{}.tmp", Uuid::new_v4()));
    let malformed_temporary = workflow.join(".workflow-NOT-A-UUID.tmp");
    let committed = workflow.join("workflow.json");
    let unrelated = workflow.join("notes.tmp");
    let nested_temporary = workflow
        .join("assets")
        .join(format!(".workflow-{}.tmp", Uuid::new_v4()));
    for path in [
        &owned_temporary,
        &malformed_temporary,
        &committed,
        &unrelated,
        &nested_temporary,
    ] {
        fs::write(path, b"value").unwrap();
    }

    let hard_link_source = workflow.join("hard-link-source");
    fs::write(&hard_link_source, b"linked").unwrap();
    let hard_link_temporary = workflow.join(format!(".workflow-{}.tmp", Uuid::new_v4()));
    fs::hard_link(&hard_link_source, &hard_link_temporary).unwrap();

    let invalid_owner_temporary = accounts
        .join("not-an-owner")
        .join(Uuid::new_v4().to_string())
        .join(format!(".workflow-{}.tmp", Uuid::new_v4()));
    fs::create_dir_all(invalid_owner_temporary.parent().unwrap()).unwrap();
    fs::write(&invalid_owner_temporary, b"value").unwrap();
    let invalid_workflow_temporary = accounts
        .join(&owner)
        .join("not-a-workflow")
        .join(format!(".workflow-{}.tmp", Uuid::new_v4()));
    fs::create_dir_all(invalid_workflow_temporary.parent().unwrap()).unwrap();
    fs::write(&invalid_workflow_temporary, b"value").unwrap();

    assert_eq!(cleanup_workflow_temporaries(&accounts, UNIX_EPOCH), 0);
    assert_eq!(
        cleanup_workflow_temporaries(&accounts, SystemTime::now() + Duration::from_secs(1),),
        1
    );
    assert!(!owned_temporary.exists());
    assert!(malformed_temporary.exists());
    assert!(committed.exists());
    assert!(unrelated.exists());
    assert!(nested_temporary.exists());
    assert!(hard_link_source.exists());
    assert!(hard_link_temporary.exists());
    assert!(invalid_owner_temporary.exists());
    assert!(invalid_workflow_temporary.exists());
}

#[test]
fn catalog_matches_macos_filter_sort_and_redacts_secrets() {
    let models = present_model_catalog(
        "alice",
        vec![
            model("chat", "alice", "gpt-6.1-sol", true),
            model("image", "alice", "gpt-image-1", true),
            model("disabled", "alice", "gpt-image-2", false),
            model("other", "bob", "gpt-image-3", true),
        ],
    )
    .unwrap();
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["image", "chat"]
    );
    let encoded = serde_json::to_string(&models).unwrap();
    assert!(!encoded.contains("must-never-cross"));
    assert!(!encoded.contains("api_key"));
    assert!(encoded.contains("hasApiKey"));
    assert!(encoded.contains("supportsImageInput"));
    assert!(encoded.contains("\"workload\":\"image\""));
    assert_eq!(models[0].workload, MediaStudioModelWorkload::Image);
    assert_eq!(models[1].workload, MediaStudioModelWorkload::Chat);
}

#[test]
fn generation_ipc_accepts_only_opaque_reference_ids() {
    let presentation_id = Uuid::new_v4().to_string();
    let valid = serde_json::json!({
        "presentationId": presentation_id,
        "modelConfigId": "image-model",
        "prompt": "quiet lake",
        "size": "1024x1024",
        "count": 1,
        "referenceImageIds": ["reference_123"],
        "clientRequestId": "request-1"
    });
    let request: MediaStudioImageGenerationRequest = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(request.reference_image_ids, vec!["reference_123"]);

    let mut raw_reference = valid;
    raw_reference["referenceImages"] = serde_json::json!([{
        "name": "secret.png",
        "mimeType": "image/png",
        "base64Data": "aW1hZ2U="
    }]);
    assert!(serde_json::from_value::<MediaStudioImageGenerationRequest>(raw_reference).is_err());
}

#[test]
fn video_generation_ipc_accepts_only_opaque_native_inputs() {
    let presentation_id = Uuid::new_v4().to_string();
    let valid = serde_json::json!({
        "presentationId": presentation_id,
        "modelConfigId": "video-model",
        "prompt": "running dog",
        "size": "768P",
        "seconds": 4,
        "ratio": "16:9",
        "inputImageId": "reference_123",
        "referenceAudioId": "video_audio_123",
        "clientRequestId": "request-1"
    });
    let request: MediaStudioVideoGenerationRequest = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(request.input_image_id.as_deref(), Some("reference_123"));
    assert_eq!(
        request.reference_audio_id.as_deref(),
        Some("video_audio_123")
    );

    let mut raw_media = valid;
    raw_media["inputImage"] = serde_json::json!({
        "mimeType": "image/png",
        "base64Data": "aW1hZ2U="
    });
    assert!(serde_json::from_value::<MediaStudioVideoGenerationRequest>(raw_media).is_err());
}

#[tokio::test]
async fn persistence_failure_preserves_generated_result_without_native_paths_or_secrets() {
    let root = tempdir().unwrap();
    let app_data = root.path().join("app-data");
    let manager = MediaStudioManager::new(app_data.clone()).unwrap();
    fs::write(
        app_data.join("MediaStudio").join("Accounts"),
        b"not-a-directory",
    )
    .unwrap();
    let preview_session_token = manager.activate_session("alice", 1).unwrap();
    let result = ImageGenerationResult {
        id: "provider-result".into(),
        model_config_id: "image-model".into(),
        model_name: "gpt-image-1".into(),
        created_at_unix_ms: 1_700_000_000_000,
        images: vec![GeneratedImageAsset {
            id: "asset-1".into(),
            mime_type: "image/png".into(),
            base64_data: Some(BASE64.encode(b"generated-image")),
            url: None,
            revised_prompt: None,
        }],
        client_request_id: "request-1".into(),
    };

    let outcome = persist_generation_result(
        &manager,
        "alice",
        "quiet lake",
        result.clone(),
        &preview_session_token,
    )
    .await;
    let ImageGenerationOutcome::GeneratedWithoutPersistence {
        result: preserved,
        persistence_error_message,
    } = &outcome
    else {
        panic!("generation result must survive a persistence failure");
    };
    assert_eq!(preserved, &result);
    assert!(!persistence_error_message.is_empty());

    let encoded = serde_json::to_string(&outcome).unwrap();
    assert!(encoded.contains("generatedWithoutPersistence"));
    assert!(!encoded.contains("api_key"));
    assert!(!encoded.contains("must-never-cross"));
    assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
}

#[tokio::test]
async fn preview_protocol_is_bound_to_the_active_authentication_generation() {
    let root = tempdir().unwrap();
    let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
    let session = manager.activate_session("alice", 7).unwrap();
    let result = ImageGenerationResult {
        id: "provider-result".into(),
        model_config_id: "image-model".into(),
        model_name: "gpt-image-1".into(),
        created_at_unix_ms: 1_700_000_000_000,
        images: vec![GeneratedImageAsset {
            id: "asset-1".into(),
            mime_type: "image/png".into(),
            base64_data: Some(BASE64.encode(b"preview-bytes")),
            url: None,
            revised_prompt: None,
        }],
        client_request_id: "request-1".into(),
    };
    let ImageGenerationOutcome::Saved { item } =
        persist_generation_result(&manager, "alice", "quiet lake", result, &session).await
    else {
        panic!("history should persist");
    };
    let path = format!("/{}/{}/{}", session, item.id, item.images[0].content_token);
    let (mime_type, bytes) = manager.protocol_body(&path).unwrap();
    assert_eq!(mime_type, "image/png");
    assert_eq!(bytes, b"preview-bytes");

    manager.reset();
    assert_eq!(
        manager.protocol_body(&path).unwrap_err(),
        http::StatusCode::UNAUTHORIZED
    );
    let replacement = manager.activate_session("alice", 8).unwrap();
    assert_ne!(session, replacement);
    assert_eq!(
        manager.protocol_body(&path).unwrap_err(),
        http::StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn story_history_bridge_reads_only_the_current_owners_opaque_asset() {
    let root = tempdir().unwrap();
    let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
    let result = ImageGenerationResult {
        id: "provider-result".into(),
        model_config_id: "image-model".into(),
        model_name: "gpt-image-1".into(),
        created_at_unix_ms: 1_700_000_000_000,
        images: vec![GeneratedImageAsset {
            id: "provider-asset".into(),
            mime_type: "image/png".into(),
            base64_data: Some(BASE64.encode(b"owner-scoped-image")),
            url: None,
            revised_prompt: None,
        }],
        client_request_id: "request-1".into(),
    };
    let item = manager
        .history
        .persist_image("alice", "quiet lake", &result)
        .await
        .unwrap();
    let asset = &item.images[0];

    assert_eq!(
        manager
            .read_story_history_image("alice", &item.id, &asset.content_token)
            .unwrap(),
        b"owner-scoped-image"
    );
    assert!(manager
        .read_story_history_image("bob", &item.id, &asset.content_token)
        .is_err());
    assert!(manager
        .read_story_history_image("alice", &item.id, &"0".repeat(32))
        .is_err());
}

#[test]
fn video_preview_uses_the_same_owner_generation_protocol_boundary() {
    use std::sync::atomic::AtomicBool;

    let root = tempdir().unwrap();
    let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
    let session = manager.activate_session("alice", 7).unwrap();
    let item = manager
        .video_history
        .persist_video(
            "alice",
            "clip",
            &video_provider::VideoGenerationResult {
                id: "provider-video".into(),
                model_config_id: "video-model".into(),
                model_name: "MiniMax-H3".into(),
                created_at_unix_ms: 1_700_000_000_000,
                mime_type: "video/mp4".into(),
                video_data: b"video-bytes".to_vec(),
            },
            &AtomicBool::new(false),
        )
        .unwrap();
    let path = format!("/{}/{}/{}", session, item.id, item.content_token);
    let (mime_type, bytes) = manager.protocol_body(&path).unwrap();
    assert_eq!(mime_type, "video/mp4");
    assert_eq!(bytes, b"video-bytes");

    manager.reset();
    assert_eq!(
        manager.protocol_body(&path).unwrap_err(),
        http::StatusCode::UNAUTHORIZED
    );
}

#[test]
fn media_preview_protocol_supports_bounded_video_ranges() {
    let request = http::Request::builder()
        .uri("/session/record/token")
        .header(http::header::RANGE, "bytes=2-5")
        .body(Vec::new())
        .unwrap();
    let response = media_preview_response(&request, "video/mp4", b"0123456789".to_vec());
    assert_eq!(response.status(), http::StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[http::header::CONTENT_TYPE], "video/mp4");
    assert_eq!(response.headers()[http::header::CONTENT_LENGTH], "4");
    assert_eq!(
        response.headers()[http::header::CONTENT_RANGE],
        "bytes 2-5/10"
    );
    assert_eq!(response.headers()[http::header::ACCEPT_RANGES], "bytes");
    assert_eq!(response.body(), b"2345");

    let suffix = http::Request::builder()
        .uri("/session/record/token")
        .header(http::header::RANGE, "bytes=6-")
        .body(Vec::new())
        .unwrap();
    let response = media_preview_response(&suffix, "video/mp4", b"0123456789".to_vec());
    assert_eq!(response.status(), http::StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.body(), b"6789");

    let tail = http::Request::builder()
        .uri("/session/record/token")
        .header(http::header::RANGE, "bytes=-3")
        .body(Vec::new())
        .unwrap();
    let response = media_preview_response(&tail, "video/mp4", b"0123456789".to_vec());
    assert_eq!(response.status(), http::StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.body(), b"789");

    let invalid = http::Request::builder()
        .uri("/session/record/token")
        .header(http::header::RANGE, "bytes=20-30")
        .body(Vec::new())
        .unwrap();
    let response = media_preview_response(&invalid, "video/mp4", b"0123456789".to_vec());
    assert_eq!(response.status(), http::StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        response.headers()[http::header::CONTENT_RANGE],
        "bytes */10"
    );

    let multiple = http::Request::builder()
        .uri("/session/record/token")
        .header(http::header::RANGE, "bytes=0-1,8-9")
        .body(Vec::new())
        .unwrap();
    let response = media_preview_response(&multiple, "video/mp4", b"0123456789".to_vec());
    assert_eq!(response.status(), http::StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        response.headers()[http::header::CONTENT_TYPE],
        "multipart/byteranges; boundary=chatos-media-range"
    );
    let body = String::from_utf8(response.body().clone()).unwrap();
    assert!(body.contains("Content-Range: bytes 0-1/10"));
    assert!(body.contains("Content-Range: bytes 8-9/10"));
    assert!(body.contains("01"));
    assert!(body.contains("89"));

    let head = http::Request::builder()
        .method(http::Method::HEAD)
        .uri("/session/record/token")
        .body(Vec::new())
        .unwrap();
    let response = media_preview_response(&head, "video/mp4", b"0123456789".to_vec());
    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(response.headers()[http::header::CONTENT_LENGTH], "10");
    assert!(response.body().is_empty());

    let options = http::Request::builder()
        .method(http::Method::OPTIONS)
        .uri("/session/record/token")
        .body(Vec::new())
        .unwrap();
    let response = media_preview_response(&options, "video/mp4", b"0123456789".to_vec());
    assert_eq!(response.status(), http::StatusCode::NO_CONTENT);
    assert_eq!(response.headers()["Access-Control-Allow-Origin"], "*");
    assert_eq!(
        response.headers()["Access-Control-Allow-Headers"],
        "Range, Content-Type"
    );
    assert!(response.body().is_empty());
}

#[test]
fn reset_clears_video_inputs_and_active_jobs() {
    let root = tempdir().unwrap();
    let manager = MediaStudioManager::new(root.path().to_path_buf()).unwrap();
    manager.activate_session("alice", 7).unwrap();
    let presentation_id = Uuid::new_v4().to_string();
    let (input_epoch, _) = manager
        .video_inputs
        .begin_scope("alice", 7, &presentation_id)
        .unwrap();
    let snapshot = manager
        .video_jobs
        .register(
            "alice",
            7,
            &presentation_id,
            VideoGenerationRequest {
                model_config_id: "video-model".into(),
                prompt: "moving clouds".into(),
                size: "768P".into(),
                seconds: 4,
                ratio: "16:9".into(),
                input_image: None,
                last_frame_image: None,
                reference_video: None,
                reference_audio: None,
                reference_purpose: video_provider::VideoReferencePurpose::Reference,
                client_request_id: "request-1".into(),
            },
            video_provider::ResolvedVideoModel {
                model_config_id: "video-model".into(),
                model_name: "MiniMax-H3".into(),
                base_url: reqwest::Url::parse("https://api.example.com/v1/").unwrap(),
                api_key: "must-never-cross".into(),
            },
            video_provider::ProviderVideoJob {
                id: "provider-job".into(),
                status: "queued".into(),
                progress: None,
                model_name: Some("MiniMax-H3".into()),
                error_message: None,
                content_url: None,
            },
        )
        .unwrap();

    manager.reset();

    let (replacement_epoch, _) = manager
        .video_inputs
        .begin_scope("alice", 7, &presentation_id)
        .unwrap();
    assert_ne!(input_epoch, replacement_epoch);
    assert!(manager
        .video_jobs
        .begin_poll("alice", 7, &presentation_id, &snapshot.job_id)
        .is_err());
}
