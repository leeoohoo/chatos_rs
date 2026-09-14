// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use chatos_agent_profiles::{StoryDesignContextProvider, STORY_DESIGN_CAPABILITY_SNAPSHOT_REF};
use chatos_client_storage::{
    AgentRunStateRecord, ClientStorage, PutRecord, RecordMetadata, RecordQuery, RecordScope,
    SecretReference, SqliteBootstrapProfile, SqliteClientStorage, StorageEncryptionKey,
    StorageResult, StorageTransaction, StoryRecord, StoryRecordKind, TransactionRepositories,
};
use chatos_local_agent_host::{
    apply_story_design, create_story_design_run, CreateStoryDesignRunRequest,
    DurableStoryDesignRecord, StoredStoryDesignContextProvider, StoryDesignLocalToolRuntime,
};
use chatos_local_agent_protocol::{
    canonical_json_digest, ApplyStoryDesignCommand, ContextStrategy, CreateStoryDesignCommand,
    LocalAgentRunStatus, LocalStoryDesignStage, ModelProtocol, ModelRuntimeDescriptor, ToolEffect,
};
use chatos_local_agent_runtime::{LocalToolInvocation, LocalToolRuntime};
use chrono::Utc;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

fn scope() -> RecordScope {
    RecordScope {
        owner_user_id: "alice".to_string(),
    }
}

fn descriptor() -> ModelRuntimeDescriptor {
    ModelRuntimeDescriptor {
        model_config_id: "model-story-1".to_string(),
        revision: 3,
        provider: "openai".to_string(),
        model: "gpt-test".to_string(),
        protocol: ModelProtocol::Responses,
        context_window_tokens: 400_000,
        maximum_output_tokens: 32_000,
        context_strategy: ContextStrategy::ProviderNative,
        supports_streaming: true,
        supports_native_compaction: true,
        supports_input_token_count: true,
    }
}

fn base_project() -> Value {
    json!({
        "id":"project-1","version":2,"title":"Homecoming","description":"A visual short.",
        "models":{"textModelID":"text-1","imageModelID":"image-1","videoModelID":"video-1"},
        "source":"归来旧屋","style":"Natural cinematic light","ratio":"16:9","summary":"",
        "characters":[],"scenes":[],"props":[],"segments":[],"relations":[],
        "createdAt":0.0,"updatedAt":0.0
    })
}

fn completed_project() -> Value {
    json!({
        "id":"project-1","version":2,"title":"Homecoming","description":"A visual short.",
        "models":{"textModelID":"text-1","imageModelID":"image-1","videoModelID":"video-1"},
        "source":"归来旧屋","style":"Natural cinematic light","ratio":"16:9",
        "summary":"A hero returns home.","characters":[],"props":[],
        "scenes":[{
            "id":"room","name":"Old room","profile":{
                "roleInStory":"Homecoming location","setting":"Old apartment at dusk",
                "spatialLayout":"Door opposite a single window",
                "lightingAndPalette":"Blue dusk and warm lamp",
                "keyElements":"Wood table and dusty mirror","atmosphere":"Quiet tension",
                "consistencyNotes":"Door and window stay opposite"
            },
            "imagePrompt":"Old apartment at dusk\nDoor opposite a single window\nBlue dusk and warm lamp\nWood table and dusty mirror\nQuiet tension\nDoor and window stay opposite",
            "media":{"images":[],"confirmedImageID":null,"generationAttemptID":null}
        }],
        "segments":[{
            "id":"s1","title":"Return","synopsis":"The hero enters.","kind":"story",
            "seconds":4,"sourceRange":{"start":0,"end":4},"characterIDs":[],
            "sceneIDs":["room"],"propIDs":[],"detail":null,
            "firstFrames":{"images":[],"confirmedImageID":null,"generationAttemptID":null},
            "inheritedFirstFrameSourceSegmentID":null,
            "lastFrames":{"images":[],"confirmedImageID":null,"generationAttemptID":null},
            "useLastFrameForVideo":true,"attempt":null,"previousAttempts":[],"video":null,"error":null
        }],
        "relations":[],"createdAt":0.0,"updatedAt":0.0
    })
}

async fn storage() -> Arc<dyn ClientStorage> {
    let directory = tempfile::tempdir().unwrap().keep();
    Arc::new(
        SqliteClientStorage::open(
            &SqliteBootstrapProfile {
                database_path: directory.join("client.sqlite3"),
                encryption_secret: SecretReference::new("test:story-design").unwrap(),
            },
            &StorageEncryptionKey::new([71; 32]),
        )
        .await
        .unwrap(),
    )
}

fn command(project: &Value) -> CreateStoryDesignCommand {
    CreateStoryDesignCommand {
        run_id: "story-run-1".to_string(),
        story_record_id: "agent-run:story-1".to_string(),
        project_id: "project-1".to_string(),
        expected_project_revision: 1,
        model_config_id: "model-story-1".to_string(),
        stage: LocalStoryDesignStage::Outline,
        target_ids: Vec::new(),
        base_project_digest: canonical_json_digest(project),
    }
}

#[tokio::test]
async fn creation_atomically_freezes_the_story_and_rebuilds_provider_context() {
    let storage = storage().await;
    let project = base_project();
    seed_project(storage.as_ref(), project.clone()).await;
    let created = create_story_design_run(
        storage.as_ref(),
        CreateStoryDesignRunRequest {
            scope: scope(),
            device_id: "device-1".to_string(),
            causation_id: "request-1".to_string(),
            command: command(&project),
            model_runtime_snapshot: descriptor(),
            now: Utc::now(),
        },
    )
    .await
    .unwrap();

    assert_eq!(created.run_record.run.owner_entity_id, "agent-run:story-1");
    let provider = StoredStoryDesignContextProvider::new(storage.clone(), scope());
    let context = provider
        .load_step_context(&created.run_record.run)
        .await
        .unwrap();
    assert_eq!(context.state.draft, project);
    assert_eq!(context.model_input_items.len(), 1);
    assert_eq!(context.state.base_project_revision, 1);
}

#[tokio::test]
async fn story_tool_receipt_makes_a_committed_draft_write_exactly_replayable() {
    let storage = storage().await;
    let project = base_project();
    seed_project(storage.as_ref(), project.clone()).await;
    create_story_design_run(
        storage.as_ref(),
        CreateStoryDesignRunRequest {
            scope: scope(),
            device_id: "device-1".to_string(),
            causation_id: "request-1".to_string(),
            command: command(&project),
            model_runtime_snapshot: descriptor(),
            now: Utc::now(),
        },
    )
    .await
    .unwrap();
    let runtime = StoryDesignLocalToolRuntime::new(storage.clone(), scope(), "device-1");
    let invocation = LocalToolInvocation {
        invocation_id: "invocation-1".to_string(),
        run_id: "story-run-1".to_string(),
        batch_id: "batch-1".to_string(),
        source_turn_id: "turn-1".to_string(),
        project_id: Some("project-1".to_string()),
        capability_snapshot_ref: STORY_DESIGN_CAPABILITY_SNAPSHOT_REF.to_string(),
        tool_call_id: "call-1".to_string(),
        tool_name: "story_save_summary".to_string(),
        effect: ToolEffect::DraftWrite,
        arguments: json!({"summary":"A hero returns home."}),
    };
    let first = runtime
        .execute(invocation.clone(), CancellationToken::new())
        .await
        .unwrap();
    let first_record = load_story(storage.as_ref(), "agent-run:story-1").await;
    let replay = runtime
        .execute(invocation, CancellationToken::new())
        .await
        .unwrap();
    let replay_record = load_story(storage.as_ref(), "agent-run:story-1").await;

    assert_eq!(first, replay);
    assert_eq!(
        first_record.metadata.revision,
        replay_record.metadata.revision
    );
    let durable: DurableStoryDesignRecord = serde_json::from_value(replay_record.state).unwrap();
    assert_eq!(durable.design.draft["summary"], "A hero returns home.");
    assert_eq!(durable.tool_receipts.len(), 1);
}

#[tokio::test]
async fn successful_design_applies_once_with_project_revision_cas() {
    let storage = storage().await;
    let project = base_project();
    seed_project(storage.as_ref(), project.clone()).await;
    create_story_design_run(
        storage.as_ref(),
        CreateStoryDesignRunRequest {
            scope: scope(),
            device_id: "device-1".to_string(),
            causation_id: "request-1".to_string(),
            command: command(&project),
            model_runtime_snapshot: descriptor(),
            now: Utc::now(),
        },
    )
    .await
    .unwrap();
    let completed = completed_project();
    let story_revision = mark_design_succeeded(storage.as_ref(), completed.clone()).await;
    let apply = ApplyStoryDesignCommand {
        run_id: "story-run-1".to_string(),
        story_record_id: "agent-run:story-1".to_string(),
        project_id: "project-1".to_string(),
        expected_project_revision: 1,
        expected_story_revision: story_revision,
    };
    let applied = apply_story_design(
        storage.as_ref(),
        scope(),
        "device-1".to_string(),
        apply.clone(),
        Utc::now(),
    )
    .await
    .unwrap();
    let replay = apply_story_design(
        storage.as_ref(),
        scope(),
        "device-1".to_string(),
        apply,
        Utc::now(),
    )
    .await
    .unwrap();

    assert_eq!(applied.project.revision, 2);
    assert_eq!(applied.project.draft.state, completed);
    assert_eq!(applied.design.draft.status.as_deref(), Some("applied"));
    assert_eq!(replay, applied);
}

async fn seed_project(storage: &dyn ClientStorage, state: Value) {
    let mut operation = SeedProject { state: Some(state) };
    storage.transaction(&mut operation).await.unwrap();
}

struct SeedProject {
    state: Option<Value>,
}

#[async_trait]
impl StorageTransaction for SeedProject {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let now = Utc::now();
        repositories
            .stories()
            .put(PutRecord {
                record: StoryRecord {
                    metadata: RecordMetadata {
                        id: "project:project-1".to_string(),
                        scope: scope(),
                        origin_device_id: "device-1".to_string(),
                        revision: 0,
                        created_at: now,
                        updated_at: now,
                    },
                    project_id: "project-1".to_string(),
                    kind: StoryRecordKind::Project,
                    status: Some("draft".to_string()),
                    state: self.state.take().unwrap(),
                },
                expected_revision: None,
            })
            .await?;
        Ok(())
    }
}

async fn load_story(storage: &dyn ClientStorage, id: &str) -> StoryRecord {
    let mut operation = LoadStory {
        id: id.to_string(),
        result: None,
    };
    storage.transaction(&mut operation).await.unwrap();
    operation.result.unwrap()
}

struct LoadStory {
    id: String,
    result: Option<StoryRecord>,
}

#[async_trait]
impl StorageTransaction for LoadStory {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        self.result = repositories
            .stories()
            .get(&RecordQuery {
                scope: scope(),
                id: self.id.clone(),
            })
            .await?;
        Ok(())
    }
}

async fn mark_design_succeeded(storage: &dyn ClientStorage, draft: Value) -> u64 {
    let mut operation = CompleteDesign {
        draft: Some(draft),
        story_revision: None,
    };
    storage.transaction(&mut operation).await.unwrap();
    operation.story_revision.unwrap()
}

struct CompleteDesign {
    draft: Option<Value>,
    story_revision: Option<u64>,
}

#[async_trait]
impl StorageTransaction for CompleteDesign {
    async fn execute(
        &mut self,
        repositories: &mut dyn TransactionRepositories,
    ) -> StorageResult<()> {
        let now = Utc::now();
        let mut story = repositories
            .stories()
            .get(&RecordQuery {
                scope: scope(),
                id: "agent-run:story-1".to_string(),
            })
            .await?
            .unwrap();
        let mut durable: DurableStoryDesignRecord =
            serde_json::from_value(story.state.clone()).unwrap();
        durable.design.draft = self.draft.take().unwrap();
        durable.design.read_through = 4;
        let story_revision = story.metadata.revision;
        story.state = serde_json::to_value(&durable).unwrap();
        story.metadata.updated_at = now;
        let story = repositories
            .stories()
            .put(PutRecord {
                record: story,
                expected_revision: Some(story_revision),
            })
            .await?;
        self.story_revision = Some(story.metadata.revision);

        let mut run: AgentRunStateRecord = repositories
            .agent_runs()
            .get(&RecordQuery {
                scope: scope(),
                id: "story-run-1".to_string(),
            })
            .await?
            .unwrap();
        let run_revision = run.metadata.revision;
        run.run.version = run_revision + 1;
        run.run.status = LocalAgentRunStatus::Succeeded;
        run.run.terminal_outcome = Some(json!({
            "kind":"story_design","story_record_id":"agent-run:story-1",
            "project_id":"project-1","base_project_revision":1,
            "base_project_digest":run_design_digest(&durable, true),
            "stage":"outline","draft_digest":canonical_json_digest(&durable.design.draft)
        }));
        run.run.updated_at = now;
        run.metadata.updated_at = now;
        repositories
            .agent_runs()
            .put(PutRecord {
                record: run,
                expected_revision: Some(run_revision),
            })
            .await?;
        Ok(())
    }
}

fn run_design_digest(durable: &DurableStoryDesignRecord, base: bool) -> String {
    if base {
        durable.design.base_project_digest.clone()
    } else {
        canonical_json_digest(&durable.design.draft)
    }
}
