// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use crate::config::{AppConfig, StoreMode, TaskRunnerRole};
use crate::models::{
    AskUserPromptStatus, ModelPhaseStatus, TaskMcpConfig, TaskRunStatus, TaskScheduleConfig,
    TaskScheduleMode, TaskStatus, TaskToolState, TASK_PROFILE_DEFAULT,
};
use crate::services::TaskService;
use serde_json::json;

fn service_config(database_url: String) -> AppConfig {
    AppConfig {
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        otlp_endpoint: "http://127.0.0.1:4317".to_string(),
        otlp_trace_sample_ratio: 0.0,
        otlp_export_timeout: Duration::from_secs(1),
        role: TaskRunnerRole::All,
        store_mode: StoreMode::Postgres,
        database_url,
        memory_engine_base_url: None,
        memory_engine_source_id: "task".to_string(),
        memory_engine_operator_token: None,
        memory_engine_http_client: reqwest::Client::new(),
        default_tenant_id: "tenant".to_string(),
        default_subject_id: "subject".to_string(),
        default_workspace_dir: ".".to_string(),
        memory_timeout: Duration::from_secs(1),
        execution_timeout: Duration::from_secs(1),
        scheduler_poll_interval: Duration::from_secs(1),
        worker_id: "postgres-contract-worker".to_string(),
        worker_claim_ttl: Duration::from_secs(120),
        worker_concurrency: 4,
        default_task_execution_max_iterations: 1,
        default_tool_result_model_max_chars: 1_000,
        default_tool_results_model_total_max_chars: 2_000,
        chatos_callback_url: String::new(),
        chatos_callback_http_client: reqwest::Client::new(),
        chatos_internal_api_secret: None,
        mcp_management_internal_api_secret: None,
        user_service_internal_api_secret: None,
        callback_timeout: Duration::from_secs(1),
        admin_username: "admin".to_string(),
        admin_password: "admin".to_string(),
        admin_display_name: "Admin".to_string(),
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_request_timeout: Duration::from_secs(1),
    }
}

async fn test_store() -> PostgresStore {
    let database_url = std::env::var("TASK_RUNNER_TEST_DATABASE_URL")
        .expect("TASK_RUNNER_TEST_DATABASE_URL must be set");
    let config = chatos_postgres::PostgresConfig::new(database_url).expect("test config");
    let pool = chatos_postgres::connect(&config).await.expect("test pool");
    let (run_event_sender, _) = broadcast::channel(32);
    let (run_event_persist_sender, _receiver) = mpsc::sync_channel(32);
    PostgresStore {
        pool,
        user_service_model_source: UserServiceModelSource {
            base_url: "https://unused.test".to_string(),
            http_client: reqwest::Client::new(),
            signing_secret: "unused".to_string(),
        },
        run_event_persist_sender,
        cancel_requested_runs: Arc::new(RwLock::new(HashSet::new())),
        run_event_sender,
    }
}

fn task(id: &str, next_run_at: Option<&str>) -> TaskRecord {
    let now = now_rfc3339();
    TaskRecord {
        id: id.to_string(),
        title: id.to_string(),
        description: None,
        objective: "postgres contract".to_string(),
        input_payload: Some(json!({"round_trip": true})),
        status: TaskStatus::Ready,
        priority: 0,
        tags: vec!["postgres".to_string()],
        default_model_config_id: Some("model-contract".to_string()),
        memory_thread_id: format!("thread-{id}"),
        tenant_id: "tenant-contract".to_string(),
        subject_id: "subject-contract".to_string(),
        project_id: None,
        project_context: None,
        task_profile: TASK_PROFILE_DEFAULT.to_string(),
        creator_user_id: None,
        creator_username: None,
        creator_display_name: None,
        owner_user_id: None,
        owner_username: None,
        owner_display_name: None,
        result_summary: None,
        process_log: None,
        last_run_id: None,
        schedule: TaskScheduleConfig {
            mode: if next_run_at.is_some() {
                TaskScheduleMode::Interval
            } else {
                TaskScheduleMode::Manual
            },
            run_at: None,
            interval_seconds: Some(60),
            next_run_at: next_run_at.map(ToOwned::to_owned),
            last_scheduled_at: None,
        },
        parent_task_id: None,
        source_run_id: None,
        source_session_id: None,
        source_turn_id: None,
        source_user_message_id: None,
        remote_connection_id: None,
        prerequisite_task_ids: Vec::new(),
        task_tool_state: TaskToolState::default(),
        plugin_config: Default::default(),
        plugin_selection_audit: None,
        mcp_config: TaskMcpConfig::default(),
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    }
}

async fn seed_message_tasks(
    store: &PostgresStore,
    source_session_id: &str,
    source_user_message_id: &str,
    count: usize,
) -> Vec<String> {
    let mut task_ids = Vec::with_capacity(count);
    for index in 0..count {
        let task_id = format!("contract-message-{source_session_id}-{index:04}");
        let run_id = format!("contract-message-run-{source_session_id}-{index:04}");
        let mut record = task(&task_id, None);
        record.status = TaskStatus::Queued;
        record.source_session_id = Some(source_session_id.to_string());
        record.source_user_message_id = Some(source_user_message_id.to_string());
        record.last_run_id = Some(run_id.clone());
        store.save_task(record).await.expect("save message task");
        store
            .save_run(TaskRunRecord::queued(
                run_id,
                task_id.clone(),
                "model-contract".to_string(),
                format!("thread-{task_id}"),
                json!({}),
                now_rfc3339(),
            ))
            .await
            .expect("save message run");
        task_ids.push(task_id);
    }
    task_ids
}

async fn measured_message_query_calls(
    service: &TaskService,
    store: &PostgresStore,
    source_session_id: &str,
    source_user_message_id: &str,
    expected_tasks: usize,
) -> i64 {
    sqlx::query("SELECT pg_stat_statements_reset()")
        .execute(&store.pool)
        .await
        .expect("reset query statistics");
    let tasks = service
        .list_tasks_for_chatos_source(source_session_id, Some(source_user_message_id), None)
        .await
        .expect("list message tasks");
    assert_eq!(tasks.len(), expected_tasks);
    sqlx::query_scalar::<_, i64>(
        "SELECT coalesce(sum(calls),0)::bigint FROM pg_stat_statements \
         WHERE query LIKE 'SELECT data FROM tasks WHERE %' \
         OR query LIKE 'SELECT data FROM task_runs WHERE id=ANY%' \
         OR query LIKE 'SELECT data FROM task_prerequisites WHERE task_id = ANY%'",
    )
    .fetch_one(&store.pool)
    .await
    .expect("read query statistics")
}

#[tokio::test]
#[ignore = "requires TASK_RUNNER_TEST_DATABASE_URL, migrations, and pg_stat_statements"]
async fn postgres_message_task_round_trips_stay_constant_at_one_thousand_tasks() {
    let database_url = std::env::var("TASK_RUNNER_TEST_DATABASE_URL")
        .expect("TASK_RUNNER_TEST_DATABASE_URL must be set");
    let store = test_store().await;
    let service = TaskService::new(
        service_config(database_url),
        AppStore::Postgres(store.clone()),
    );
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let single_session = format!("single-{suffix}");
    let bulk_session = format!("bulk-{suffix}");
    let source_user_message_id = format!("message-{suffix}");
    seed_message_tasks(&store, &single_session, &source_user_message_id, 1).await;
    seed_message_tasks(&store, &bulk_session, &source_user_message_id, 1_000).await;

    let single_calls = measured_message_query_calls(
        &service,
        &store,
        &single_session,
        &source_user_message_id,
        1,
    )
    .await;
    let bulk_calls = measured_message_query_calls(
        &service,
        &store,
        &bulk_session,
        &source_user_message_id,
        1_000,
    )
    .await;

    assert_eq!(single_calls, 3);
    assert_eq!(bulk_calls, single_calls);

    sqlx::query("DELETE FROM tasks WHERE source_session_id=ANY($1)")
        .bind(&[single_session, bulk_session])
        .execute(&store.pool)
        .await
        .expect("cleanup message tasks");
}

#[tokio::test]
#[ignore = "requires TASK_RUNNER_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn postgres_batch_task_update_is_atomic() {
    let store = test_store().await;
    let suffix = uuid::Uuid::new_v4();
    let first_id = format!("contract-batch-first-{suffix}");
    let second_id = format!("contract-batch-second-{suffix}");
    let mut first = store
        .save_task(task(&first_id, None))
        .await
        .expect("save first task");
    let mut second = store
        .save_task(task(&second_id, None))
        .await
        .expect("save second task");

    first.status = TaskStatus::Failed;
    first.result_summary = Some("first failed".to_string());
    second.status = TaskStatus::Cancelled;
    second.result_summary = Some("second cancelled".to_string());
    store
        .update_tasks_batch(&[first.clone(), second.clone()])
        .await
        .expect("batch update");

    assert_eq!(
        store
            .get_task(&first_id)
            .await
            .expect("load first")
            .expect("first task")
            .result_summary,
        first.result_summary
    );
    assert_eq!(
        store
            .get_task(&second_id)
            .await
            .expect("load second")
            .expect("second task")
            .status,
        TaskStatus::Cancelled
    );

    sqlx::query("DELETE FROM tasks WHERE id=ANY($1)")
        .bind(&[first_id, second_id])
        .execute(&store.pool)
        .await
        .expect("cleanup tasks");
}

#[tokio::test]
#[ignore = "requires TASK_RUNNER_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn postgres_maintenance_lease_allows_one_owner_and_expiry_takeover() {
    let store = test_store().await;
    let app_store = AppStore::Postgres(store.clone());
    let suffix = uuid::Uuid::new_v4();
    let lease_name = format!("contract-maintenance-lease-{suffix}");
    let owner_a = format!("contract-owner-a-{suffix}");
    let owner_b = format!("contract-owner-b-{suffix}");
    assert!(app_store
        .try_acquire_maintenance_lease(&lease_name, &owner_a, Duration::from_secs(30))
        .await
        .expect("owner A acquires lease"));
    assert!(app_store
        .try_acquire_maintenance_lease(&lease_name, &owner_a, Duration::from_secs(60),)
        .await
        .expect("owner A renews lease"));
    assert!(!app_store
        .try_acquire_maintenance_lease(&lease_name, &owner_b, Duration::from_secs(60),)
        .await
        .expect("owner B cannot take an active lease"));

    sqlx::query(
        "UPDATE task_runner_maintenance_leases SET lease_until=now()-interval '1 second' \
         WHERE lease_name=$1",
    )
    .bind(&lease_name)
    .execute(&store.pool)
    .await
    .expect("expire maintenance lease");

    assert!(app_store
        .try_acquire_maintenance_lease(&lease_name, &owner_b, Duration::from_secs(60),)
        .await
        .expect("owner B takes over expired lease"));
    assert!(!app_store
        .try_acquire_maintenance_lease(&lease_name, &owner_a, Duration::from_secs(60),)
        .await
        .expect("former owner cannot reclaim an active lease"));

    sqlx::query("DELETE FROM task_runner_maintenance_leases WHERE lease_name=$1")
        .bind(&lease_name)
        .execute(&store.pool)
        .await
        .expect("cleanup maintenance lease");
}

#[tokio::test]
#[ignore = "requires TASK_RUNNER_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn postgres_store_enforces_atomic_scheduler_dependency_and_active_run_guards() {
    let store = test_store().await;
    let suffix = uuid::Uuid::new_v4();
    let scheduled_id = format!("contract-scheduled-{suffix}");
    let prerequisite_id = format!("contract-prerequisite-{suffix}");
    let run_task_id = format!("contract-run-task-{suffix}");
    let initial_due = "2026-09-17T01:00:00Z";
    store
        .save_task(task(&scheduled_id, Some(initial_due)))
        .await
        .expect("scheduled task");
    store
        .save_task(task(&prerequisite_id, None))
        .await
        .expect("prerequisite task");
    store
        .save_task(task(&run_task_id, None))
        .await
        .expect("run task");

    let stored = store
        .get_task(&scheduled_id)
        .await
        .expect("get task")
        .expect("stored task");
    assert_eq!(stored.input_payload, Some(json!({"round_trip": true})));
    let task_filters = TaskListFilters {
        keyword: Some("postgres".to_string()),
        tag: Some("postgres".to_string()),
        limit: Some(1),
        ..TaskListFilters::default()
    };
    let task_page = store
        .list_tasks_page(&task_filters)
        .await
        .expect("task page");
    assert_eq!(task_page.items.len(), 1);
    assert!(task_page.total >= 3);
    assert!(task_page.has_more);
    let task_summaries = store
        .list_task_summaries_filtered(&task_filters)
        .await
        .expect("task summaries");
    assert_eq!(task_summaries.len(), 1);
    let task_stats = store
        .task_stats_filtered(&TaskListFilters {
            tag: Some("postgres".to_string()),
            ..TaskListFilters::default()
        })
        .await
        .expect("task stats");
    assert!(task_stats.total >= 3);
    assert!(task_stats.ready >= 3);

    let mut schedule_jobs = Vec::new();
    for index in 0..32 {
        let store = store.clone();
        let task_id = scheduled_id.clone();
        schedule_jobs.push(tokio::spawn(async move {
            store
                .update_task_schedule_if_next_run_at(
                    &task_id,
                    initial_due,
                    TaskScheduleConfig {
                        mode: TaskScheduleMode::Interval,
                        run_at: None,
                        interval_seconds: Some(60),
                        next_run_at: Some(format!("2026-09-17T02:{index:02}:00Z")),
                        last_scheduled_at: Some(initial_due.to_string()),
                    },
                    &now_rfc3339(),
                )
                .await
                .expect("schedule CAS")
                .is_some()
        }));
    }
    let mut schedule_winners = 0;
    for job in schedule_jobs {
        schedule_winners += usize::from(job.await.expect("schedule task"));
    }
    assert_eq!(schedule_winners, 1);

    let revision = store
        .dependency_graph_revision()
        .await
        .expect("dependency revision");
    let mut revision_jobs = Vec::new();
    for _ in 0..32 {
        let store = store.clone();
        let task_id = scheduled_id.clone();
        let prerequisite_id = prerequisite_id.clone();
        revision_jobs.push(tokio::spawn(async move {
            store
                .set_task_prerequisites_if_revision(&task_id, vec![prerequisite_id], revision)
                .await
                .expect("dependency CAS")
                .is_some()
        }));
    }
    let mut revision_winners = 0;
    for job in revision_jobs {
        revision_winners += usize::from(job.await.expect("revision task"));
    }
    assert_eq!(revision_winners, 1);
    assert_eq!(
        store
            .get_task(&scheduled_id)
            .await
            .expect("task after dependency CAS")
            .expect("stored task")
            .prerequisite_task_ids,
        vec![prerequisite_id.clone()]
    );

    let mut run_jobs = Vec::new();
    for index in 0..32 {
        let store = store.clone();
        let task_id = run_task_id.clone();
        run_jobs.push(tokio::spawn(async move {
            let run = TaskRunRecord::queued(
                format!("contract-run-{suffix}-{index}"),
                task_id,
                "model-contract".to_string(),
                "thread-contract".to_string(),
                json!({}),
                now_rfc3339(),
            );
            store.save_run(run).await.is_ok()
        }));
    }
    let mut run_winners = 0;
    for job in run_jobs {
        run_winners += usize::from(job.await.expect("run task"));
    }
    assert_eq!(run_winners, 1);
    let run_filters = RunListFilters {
        task_id: Some(run_task_id.clone()),
        keyword: Some("contract".to_string()),
        limit: Some(1),
        ..RunListFilters::default()
    };
    let run_page = store.list_runs_page(&run_filters).await.expect("run page");
    assert_eq!(run_page.total, 1);
    assert_eq!(run_page.items.len(), 1);
    assert_eq!(
        store
            .list_run_summaries_filtered(&run_filters)
            .await
            .expect("run summaries")
            .len(),
        1
    );
    let run_stats = store.run_execution_stats().await.expect("run stats");
    assert!(run_stats.total >= 1);
    assert!(run_stats.active >= 1);

    for task_id in [&scheduled_id, &prerequisite_id, &run_task_id] {
        store.delete_task(task_id).await.expect("cleanup task");
    }
}

#[tokio::test]
#[ignore = "requires TASK_RUNNER_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn postgres_task_cursor_and_literal_search_preserve_contracts() {
    let store = test_store().await;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let scope_tag = format!("cursor-scope-{suffix}");
    let model_config_id = format!("cursor-model-{suffix}");
    let ids = (0..6)
        .map(|index| format!("cursor-task-{suffix}-{index:02}"))
        .collect::<Vec<_>>();

    for (index, id) in ids.iter().enumerate() {
        let mut record = task(id, None);
        record.updated_at = "2026-09-18T00:00:00Z".to_string();
        record.default_model_config_id = Some(model_config_id.clone());
        record.title = if index == 0 {
            "literal 100% safe".to_string()
        } else {
            "literal 1000 safe".to_string()
        };
        record.tags = if index == 4 {
            vec![scope_tag.clone(), "tag%needle".to_string()]
        } else {
            vec![scope_tag.clone()]
        };
        store.save_task(record).await.expect("insert cursor task");
    }

    let first = store
        .list_tasks_page(&TaskListFilters {
            tag: Some(scope_tag.clone()),
            limit: Some(2),
            ..TaskListFilters::default()
        })
        .await
        .expect("first cursor page");
    assert_eq!(first.total, ids.len());
    assert!(first.has_more);
    assert_eq!(
        first.items.iter().map(|item| &item.id).collect::<Vec<_>>(),
        vec![&ids[0], &ids[1]]
    );

    let offset_page = store
        .list_tasks_page(&TaskListFilters {
            tag: Some(scope_tag.clone()),
            limit: Some(2),
            offset: Some(2),
            ..TaskListFilters::default()
        })
        .await
        .expect("legacy offset page");
    assert_eq!(offset_page.items[0].id, ids[2]);
    assert_eq!(offset_page.items[1].id, ids[3]);

    let inserted_before_cursor = format!("cursor-task-{suffix}-new");
    let mut newer = task(&inserted_before_cursor, None);
    newer.updated_at = "2026-09-18T00:00:01Z".to_string();
    newer.default_model_config_id = Some(model_config_id.clone());
    newer.tags = vec![scope_tag.clone()];
    store
        .save_task(newer)
        .await
        .expect("insert task before cursor");

    let mut seen = first
        .items
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let mut cursor = first.items.last().expect("first page cursor").clone();
    loop {
        let page = store
            .list_tasks_page(&TaskListFilters {
                tag: Some(scope_tag.clone()),
                after_updated_at: Some(cursor.updated_at.clone()),
                after_id: Some(cursor.id.clone()),
                limit: Some(2),
                ..TaskListFilters::default()
            })
            .await
            .expect("next cursor page");
        if page.items.is_empty() {
            break;
        }
        cursor = page.items.last().expect("page cursor").clone();
        seen.extend(page.items.into_iter().map(|item| item.id));
        if !page.has_more {
            break;
        }
    }
    assert_eq!(seen, ids);
    assert!(!seen.contains(&inserted_before_cursor));

    for (keyword, expected_id) in [("100%", &ids[0]), ("tag%needle", &ids[4])] {
        let page = store
            .list_tasks_page(&TaskListFilters {
                keyword: Some(keyword.to_string()),
                model_config_id: Some(model_config_id.clone()),
                limit: Some(10),
                ..TaskListFilters::default()
            })
            .await
            .expect("literal task search");
        assert_eq!(page.total, 1);
        assert_eq!(&page.items[0].id, expected_id);
    }

    for id in ids.iter().chain(std::iter::once(&inserted_before_cursor)) {
        store.delete_task(id).await.expect("delete cursor task");
    }
}

#[tokio::test]
#[ignore = "requires TASK_RUNNER_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn postgres_store_preserves_event_prompt_subscription_and_cancel_contracts() {
    let store = test_store().await;
    let suffix = uuid::Uuid::new_v4();
    let terminal_task_id = format!("contract-terminal-task-{suffix}");
    let terminal_run_id = format!("contract-terminal-run-{suffix}");
    let cancel_task_id = format!("contract-cancel-task-{suffix}");
    let cancel_run_id = format!("contract-cancel-run-{suffix}");
    store
        .save_task(task(&terminal_task_id, None))
        .await
        .expect("terminal task");
    store
        .save_task(task(&cancel_task_id, None))
        .await
        .expect("cancel task");

    let mut terminal_run = TaskRunRecord::queued(
        terminal_run_id.clone(),
        terminal_task_id.clone(),
        "model-contract".to_string(),
        "thread-contract-terminal".to_string(),
        json!({"contract": "terminal"}),
        now_rfc3339(),
    );
    terminal_run = store.save_run(terminal_run).await.expect("queued run");
    let subscription = RunTerminalSubscriptionRecord::new(
        &terminal_run_id,
        "parent-contract-run",
        "worker-contract",
    );
    let subscribed_run = store
        .subscribe_run_terminal(subscription.clone())
        .await
        .expect("subscribe terminal run");
    assert_eq!(subscribed_run.status, TaskRunStatus::Queued);

    terminal_run.status = TaskRunStatus::Succeeded;
    terminal_run.model_phase_status = ModelPhaseStatus::Succeeded;
    terminal_run.finished_at = Some(now_rfc3339());
    terminal_run.updated_at = now_rfc3339();
    store.save_run(terminal_run).await.expect("terminal run");
    let pending_subscriptions = store
        .list_pending_run_terminal_subscriptions(10)
        .await
        .expect("pending terminal subscriptions");
    assert!(pending_subscriptions
        .iter()
        .any(|(_, pending)| pending.id == subscription.id));
    assert!(store
        .acknowledge_run_terminal_subscription(&subscription.id)
        .await
        .expect("ack terminal subscription"));
    assert!(!store
        .acknowledge_run_terminal_subscription(&subscription.id)
        .await
        .expect("idempotent terminal subscription ack"));

    let old_event = TaskRunEventRecord {
        id: format!("contract-event-old-{suffix}"),
        run_id: terminal_run_id.clone(),
        event_type: "contract_old".to_string(),
        message: Some("old".to_string()),
        payload: Some(json!({"sequence": 1})),
        created_at: "2020-01-01T00:00:00Z".to_string(),
    };
    let new_event = TaskRunEventRecord {
        id: format!("contract-event-new-{suffix}"),
        run_id: terminal_run_id.clone(),
        event_type: "contract_new".to_string(),
        message: Some("new".to_string()),
        payload: Some(json!({"sequence": 2})),
        created_at: "2030-01-01T00:00:00Z".to_string(),
    };
    store
        .append_run_event(old_event.clone())
        .await
        .expect("old event");
    store
        .append_run_event(new_event.clone())
        .await
        .expect("new event");
    let outbox_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM task_run_event_outbox WHERE event_id=ANY($1)")
            .bind(vec![old_event.id.clone(), new_event.id.clone()])
            .fetch_one(&store.pool)
            .await
            .expect("run event outbox rows");
    assert_eq!(outbox_count, 2);
    assert!(store
        .has_run_event_type(&terminal_run_id, "contract_old")
        .await
        .expect("event type"));
    assert_eq!(
        store
            .get_run_event(&terminal_run_id, &new_event.id)
            .await
            .expect("get event")
            .expect("stored event")
            .payload,
        new_event.payload
    );
    let (event_page, event_total) = store
        .list_run_events_page(&terminal_run_id, 0, 1)
        .await
        .expect("event page");
    assert_eq!(event_total, 2);
    assert_eq!(event_page[0].id, old_event.id);
    let event_suffix = store
        .list_run_events_after(
            &terminal_run_id,
            Some(&old_event.created_at),
            Some(&old_event.id),
            10,
        )
        .await
        .expect("event cursor suffix");
    assert_eq!(event_suffix.len(), 1);
    assert_eq!(event_suffix[0].id, new_event.id);
    let cursor = store
        .latest_run_event_cursor(&terminal_run_id)
        .await
        .expect("latest event cursor")
        .expect("event cursor");
    assert_eq!(cursor.1, new_event.id);
    let event_prune = store
        .prune_terminal_run_events_before("2025-01-01T00:00:00Z", 10)
        .await
        .expect("prune old events");
    assert_eq!(event_prune.eligible_runs, 1);
    assert_eq!(event_prune.deleted_events, 1);
    let retained_events = store
        .list_run_events(&terminal_run_id)
        .await
        .expect("retained events");
    assert_eq!(retained_events.len(), 1);
    assert_eq!(retained_events[0].id, new_event.id);

    let prompt_id = format!("contract-prompt-{suffix}");
    let prompt = AskUserPromptRecord {
        id: prompt_id.clone(),
        task_id: Some(terminal_task_id.clone()),
        run_id: Some(terminal_run_id.clone()),
        conversation_id: "contract-conversation".to_string(),
        conversation_turn_id: "contract-turn".to_string(),
        tool_call_id: Some("contract-tool-call".to_string()),
        kind: "text".to_string(),
        title: "Contract".to_string(),
        message: "Contract prompt".to_string(),
        allow_cancel: true,
        timeout_ms: 30_000,
        payload: json!({"contract": true}),
        response: None,
        status: AskUserPromptStatus::Submitted,
        resolution_event_pending: true,
        created_at: "2020-01-01T00:00:00Z".to_string(),
        updated_at: "2020-01-01T00:00:00Z".to_string(),
        expires_at: None,
    };
    store
        .save_ask_user_prompt(prompt.clone())
        .await
        .expect("prompt");
    let prompt_page = store
        .list_ask_user_prompts_page(&PromptListFilters {
            task_id: Some(terminal_task_id.clone()),
            run_id: Some(terminal_run_id.clone()),
            status: Some(AskUserPromptStatus::Submitted),
            limit: Some(1),
            offset: Some(0),
        })
        .await
        .expect("prompt page");
    assert_eq!(prompt_page.total, 1);
    assert_eq!(prompt_page.items[0].id, prompt_id);
    let pending_prompts = store
        .list_pending_ask_user_resolution_events(10)
        .await
        .expect("pending prompt outbox");
    assert!(pending_prompts.iter().any(|item| item.id == prompt_id));
    assert!(store
        .acknowledge_ask_user_resolution_event(&prompt_id)
        .await
        .expect("ack prompt outbox"));
    assert!(!store
        .acknowledge_ask_user_resolution_event(&prompt_id)
        .await
        .expect("idempotent prompt outbox ack"));
    let prompt_prune = store
        .prune_terminal_ask_user_prompts_before("2025-01-01T00:00:00Z", 10)
        .await
        .expect("prune terminal prompt");
    assert_eq!(prompt_prune.eligible_prompts, 1);
    assert_eq!(prompt_prune.deleted_prompts, 1);
    assert!(store
        .get_ask_user_prompt(&prompt_id)
        .await
        .expect("get pruned prompt")
        .is_none());

    let mut cancel_run = TaskRunRecord::queued(
        cancel_run_id.clone(),
        cancel_task_id.clone(),
        "model-contract".to_string(),
        "thread-contract-cancel".to_string(),
        json!({"contract": "cancel"}),
        now_rfc3339(),
    );
    cancel_run.status = TaskRunStatus::Running;
    cancel_run.model_phase_status = ModelPhaseStatus::Running;
    cancel_run.worker_id = Some("worker-contract".to_string());
    store.save_run(cancel_run).await.expect("running run");
    let cancelled = store
        .mark_cancel_requested(&cancel_run_id)
        .await
        .expect("request cancel")
        .expect("cancelled run");
    assert!(cancelled.cancel_requested);
    assert!(cancelled.cancel_event_pending);
    assert!(store.is_cancel_requested(&cancel_run_id));
    let pending_cancels = store
        .list_pending_run_cancel_events(10)
        .await
        .expect("pending cancel outbox");
    assert!(pending_cancels.iter().any(|run| run.id == cancel_run_id));
    assert!(store
        .acknowledge_run_cancel_event(&cancel_run_id)
        .await
        .expect("ack cancel outbox"));
    assert!(!store
        .acknowledge_run_cancel_event(&cancel_run_id)
        .await
        .expect("idempotent cancel outbox ack"));

    store
        .delete_task(&terminal_task_id)
        .await
        .expect("cleanup terminal task");
    store
        .delete_task(&cancel_task_id)
        .await
        .expect("cleanup cancel task");
}
