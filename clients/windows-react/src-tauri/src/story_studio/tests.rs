use super::domain::*;
use super::{
    application::{
        self, apply_story_project_settings, apply_story_project_source, default_story_style,
        make_story_project, StoryAssetImageGenerationInput, StoryCompletedVideoArchiveInput,
        StoryFrameImageGenerationInput, StoryFrameRoleInput, StoryImageConfirmationInput,
        StoryImageImportInput, StoryImageRetryInput, StoryLastFrameClearInput,
        StoryLatestFramesInput, StoryLatestResourceImagesInput, StoryModelSelectionInput,
        StoryProjectCreationInput, StoryProjectSettingsInput, StoryProjectSourceInput,
        StoryResourcePromptInput, StorySegmentDurationInput, StorySegmentEditInput,
        StorySegmentStructureInput, StorySegmentStructureMutation, StorySegmentStructureOperation,
        StoryVideoGenerationPollInput, StoryVideoGenerationStartInput, StoryVideoGuidanceInput,
        StoryVideoRetryInput,
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
    store::{self, StoryProjectStore},
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

#[path = "tests/editing.rs"]
mod editing;
#[path = "tests/media.rs"]
mod media;
#[path = "tests/planning.rs"]
mod planning;
#[path = "tests/project.rs"]
mod project;
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
    let owner_hash = owner_hash_for_test(owner);
    app_data
        .join("StoryStudio")
        .join("Accounts")
        .join(owner_hash)
        .join("Projects")
}

fn owner_hash_for_test(owner: &str) -> String {
    format!("{:x}", Sha256::digest(owner.as_bytes()))
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
        model_capability_revision: Some("test-capability-v1".into()),
        context_window_tokens: Some(200_000),
        output_reserve_tokens: Some(30_000),
        protocol_overhead_estimator_id: Some("responses-json-v1".into()),
        tokenizer_id: Some("o200k_base".into()),
        responses_compaction_threshold: Some(170_000),
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
