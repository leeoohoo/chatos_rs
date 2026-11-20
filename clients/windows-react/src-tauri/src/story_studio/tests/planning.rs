use super::*;

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
        format!("agent:story-studio:{}", canonical.id)
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
            ..Default::default()
        }],
        meta: ComposeContextMeta {
            summary_count: 1,
            recent_record_count: recent_records.len(),
            ..Default::default()
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
