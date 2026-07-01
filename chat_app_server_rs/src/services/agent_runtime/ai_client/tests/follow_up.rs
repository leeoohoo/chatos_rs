// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn task_follow_up_continues_same_turn_until_unfinished_tasks_finish() {
    let session_id = "session_task_follow_up_continue";
    let turn_id = "turn_task_follow_up_continue";
    let tasks = vec![TaskDraft {
        title: "First unfinished task".to_string(),
        details: "keep working".to_string(),
        priority: "medium".to_string(),
        status: "doing".to_string(),
        tags: vec![],
        due_at: None,
        outcome_summary: String::new(),
        outcome_items: vec![],
        resume_hint: String::new(),
        blocker_reason: String::new(),
        blocker_needs: vec![],
        blocker_kind: String::new(),
    }];
    let created = setup_sqlite_task_board(session_id, turn_id, tasks)
        .await
        .expect("setup board");
    let task_id = created[0].id.clone();
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_summary_1",
                "status": "completed",
                "output_text": "summary"
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_continue_2",
                "status": "completed",
                "output_text": "continue work"
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_review_pass_3",
                "status": "completed",
                "output_text": "TASK_REVIEW: pass\nall good"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client_with_max_iterations(base_url, 4);
    let callbacks = before_request_set_task_done_on_nth_request(session_id.to_string(), task_id, 2);

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            callbacks,
            purpose: "chat",
            ..Default::default()
        },
    )
    .await
    .expect("follow-up should continue and finish");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("continue work")
    );
    assert_eq!(
        result
            .get("task_turn_review")
            .and_then(|value| value.get("outcome"))
            .and_then(|value| value.as_str()),
        Some("pass")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 3);
    assert!(requests
        .iter()
        .all(|request| request.get("prev_id").is_none()));
}

#[tokio::test]
async fn task_follow_up_reviews_same_turn_when_work_is_done() {
    let session_id = "session_task_follow_up_review";
    let turn_id = "turn_task_follow_up_review";
    let tasks = vec![TaskDraft {
        title: "Finished task".to_string(),
        details: "already done".to_string(),
        priority: "medium".to_string(),
        status: "done".to_string(),
        tags: vec![],
        due_at: None,
        outcome_summary: String::new(),
        outcome_items: vec![],
        resume_hint: String::new(),
        blocker_reason: String::new(),
        blocker_needs: vec![],
        blocker_kind: String::new(),
    }];
    setup_sqlite_task_board(session_id, turn_id, tasks)
        .await
        .expect("setup board");
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_summary_review",
                "status": "completed",
                "output_text": "summary"
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_review_pass",
                "status": "completed",
                "output_text": "TASK_REVIEW: pass\nlooks good"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client_with_max_iterations(base_url, 4);
    let phase_events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let callbacks = AiClientCallbacks {
        on_turn_phase: Some({
            let phase_events = phase_events.clone();
            Arc::new(move |payload: Value| {
                phase_events.lock().expect("lock poisoned").push(payload);
            })
        }),
        ..empty_callbacks()
    };

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            callbacks,
            purpose: "chat",
            ..Default::default()
        },
    )
    .await
    .expect("follow-up should review and finish");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("summary")
    );
    assert_eq!(
        result
            .get("task_turn_review")
            .and_then(|value| value.get("attempted"))
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        result
            .get("task_turn_review")
            .and_then(|value| value.get("outcome"))
            .and_then(|value| value.as_str()),
        Some("pass")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert!(requests
        .iter()
        .all(|request| request.get("prev_id").is_none()));
    let phases = phase_events.lock().expect("lock poisoned").clone();
    assert_eq!(phases.len(), 1);
    assert_eq!(
        phases[0].get("phase").and_then(|value| value.as_str()),
        Some("review")
    );
}

#[tokio::test]
async fn task_follow_up_max_rounds_comes_from_runtime_settings() {
    let session_id = "session_task_follow_up_runtime_setting";
    let turn_id = "turn_task_follow_up_runtime_setting";
    let tasks = vec![TaskDraft {
        title: "Still unfinished".to_string(),
        details: "runtime setting should cap follow-ups".to_string(),
        priority: "medium".to_string(),
        status: "doing".to_string(),
        tags: vec![],
        due_at: None,
        outcome_summary: String::new(),
        outcome_items: vec![],
        resume_hint: String::new(),
        blocker_reason: String::new(),
        blocker_needs: vec![],
        blocker_kind: String::new(),
    }];
    setup_sqlite_task_board(session_id, turn_id, tasks)
        .await
        .expect("setup board");
    let steps = vec![
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_summary_before_follow_up",
                "status": "completed",
                "output_text": "first summary"
            }),
        ),
        MockProviderStep::json(
            StatusCode::OK,
            json!({
                "id": "resp_summary_after_one_follow_up",
                "status": "completed",
                "output_text": "second summary"
            }),
        ),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client_with_max_iterations(base_url, 4);
    client.apply_settings(&json!({ "TASK_FOLLOW_UP_MAX_ROUNDS": 1 }));

    let result = run_process_with_tools(
        &mut client,
        RunProcessWithToolsArgs {
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            callbacks: empty_callbacks(),
            purpose: "chat",
            ..Default::default()
        },
    )
    .await
    .expect("follow-up cap should come from runtime settings");
    server.abort();

    assert_eq!(
        result.get("content").and_then(|value| value.as_str()),
        Some("second summary")
    );
    assert_eq!(
        result
            .get("task_turn_review")
            .and_then(|value| value.get("rounds"))
            .and_then(|value| value.as_u64()),
        Some(1)
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
}
