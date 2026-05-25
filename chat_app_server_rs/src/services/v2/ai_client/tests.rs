use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::test_support::{
    before_request_set_task_done_on_nth_request,
    build_test_client_with_max_iterations, ensure_memory_session, setup_sqlite_task_board,
    start_mock_provider,
    MockProviderStep,
};
use super::AiClientCallbacks;
use crate::services::task_manager::TaskDraft;

fn empty_callbacks() -> AiClientCallbacks {
    AiClientCallbacks::default()
}

#[tokio::test]
async fn task_follow_up_continues_same_turn_until_unfinished_tasks_finish() {
    let session_id = "session_v2_task_follow_up_continue";
    let turn_id = "turn_v2_task_follow_up_continue";
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
    ensure_memory_session(session_id)
        .await
        .expect("create memory session");
    let created = setup_sqlite_task_board(session_id, turn_id, tasks)
        .await
        .expect("setup board");
    let task_id = created[0].id.clone();
    let steps = vec![
        MockProviderStep::sse(vec![json!({
            "choices": [{
                "finish_reason": "stop",
                "delta": { "content": "summary" }
            }]
        })]),
        MockProviderStep::sse(vec![json!({
            "choices": [{
                "finish_reason": "stop",
                "delta": { "content": "continue work" }
            }]
        })]),
        MockProviderStep::sse(vec![json!({
            "choices": [{
                "finish_reason": "stop",
                "delta": { "content": "TASK_REVIEW: pass\nall good" }
            }]
        })]),
    ];
    let (base_url, captured, server) = start_mock_provider(steps).await;
    let mut client = build_test_client_with_max_iterations(base_url, 4);
    let callbacks = before_request_set_task_done_on_nth_request(session_id.to_string(), task_id, 2);

    let result = client
        .process_request(
            vec![json!({"role": "user", "content": "do it"})],
            Some(session_id.to_string()),
            Some(turn_id.to_string()),
            "gpt-4o".to_string(),
            0.7,
            None,
            false,
            callbacks,
            false,
            None,
            None,
            Some("chat".to_string()),
            None,
            None,
            vec![],
        )
        .await
        .expect("follow-up should continue and finish");
    server.abort();

    assert_eq!(result.get("content").and_then(Value::as_str), Some("continue work"));
    assert_eq!(
        result
            .get("task_turn_review")
            .and_then(|value| value.get("outcome"))
            .and_then(Value::as_str),
        Some("pass")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 3);
    assert!(serde_json::to_string(&requests[1])
        .map(|text| text.len() > 0)
        .unwrap_or(false));
    assert!(serde_json::to_string(&requests[2])
        .map(|text| text.len() > 0)
        .unwrap_or(false));
}

#[tokio::test]
async fn task_follow_up_reviews_same_turn_when_work_is_done() {
    let session_id = "session_v2_task_follow_up_review";
    let turn_id = "turn_v2_task_follow_up_review";
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
    ensure_memory_session(session_id)
        .await
        .expect("create memory session");
    setup_sqlite_task_board(session_id, turn_id, tasks)
        .await
        .expect("setup board");
    let steps = vec![
        MockProviderStep::sse(vec![json!({
            "choices": [{
                "finish_reason": "stop",
                "delta": { "content": "summary" }
            }]
        })]),
        MockProviderStep::sse(vec![json!({
            "choices": [{
                "finish_reason": "stop",
                "delta": { "content": "TASK_REVIEW: pass\nlooks good" }
            }]
        })]),
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

    let result = client
        .process_request(
            vec![json!({"role": "user", "content": "check work"})],
            Some(session_id.to_string()),
            Some(turn_id.to_string()),
            "gpt-4o".to_string(),
            0.7,
            None,
            false,
            callbacks,
            false,
            None,
            None,
            Some("chat".to_string()),
            None,
            None,
            vec![],
        )
        .await
        .expect("follow-up should review and finish");
    server.abort();

    assert_eq!(result.get("content").and_then(Value::as_str), Some("summary"));
    assert_eq!(
        result
            .get("task_turn_review")
            .and_then(|value| value.get("attempted"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        result
            .get("task_turn_review")
            .and_then(|value| value.get("outcome"))
            .and_then(Value::as_str),
        Some("pass")
    );

    let requests = captured.lock().await.clone();
    assert_eq!(requests.len(), 2);
    assert!(serde_json::to_string(&requests[1])
        .map(|text| text.len() > 0)
        .unwrap_or(false));
    let phases = phase_events.lock().expect("lock poisoned").clone();
    assert_eq!(phases.len(), 1);
    assert_eq!(
        phases[0]
            .get("phase")
            .and_then(Value::as_str),
        Some("review")
    );
}
