// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn postgres_repositories_round_trip_memory_and_coordination_state() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject_id = format!("subject-{suffix}");

    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject_id.clone(),
            thread_type: "chat".to_string(),
            external_thread_id: Some(format!("ext-{suffix}")),
            title: Some("contract".to_string()),
            labels: Some(vec!["contract".to_string()]),
            metadata: Some(json!({"mapping_source":"contract"})),
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert thread");
    let listed =
        crate::repositories::threads::list_threads_by_label(&pool, &tenant, &source, "contract", None, 10, 0)
            .await
            .expect("list threads");
    assert_eq!(listed.len(), 1);

    assert!(crate::repositories::threads::try_acquire_summary_slot(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "summary-job",
    )
    .await
    .expect("claim summary slot")
    .is_some());
    assert!(!crate::repositories::threads::refresh_summary_slot(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "wrong-job",
        300,
    )
    .await
    .expect("reject wrong summary owner"));
    crate::repositories::threads::release_summary_slot(&pool, &tenant, &source, &thread_id, "summary-job", 0, 0)
        .await
        .expect("release summary slot")
        .expect("owned slot");

    let record = EngineRecord {
        id: format!("record-{suffix}"),
        thread_id: thread_id.clone(),
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        external_record_id: None,
        role: "user".to_string(),
        record_type: "message".to_string(),
        content: "hello".to_string(),
        structured_payload: None,
        metadata: Some(json!({"conversation_turn_id":"turn-1"})),
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: crate::models::now_rfc3339(),
    };
    sqlx::query("INSERT INTO engine_records(id,thread_id,tenant_id,source_id,role,record_type,summary_status,created_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(&record.id).bind(&record.thread_id).bind(&record.tenant_id).bind(&record.source_id)
        .bind(&record.role).bind(&record.record_type).bind(&record.summary_status)
        .bind(timestamp(&record.created_at).expect("record time")).bind(json(&record).expect("record json"))
        .execute(&pool).await.expect("insert record");
    assert_eq!(
        crate::repositories::records::claim_records_for_summary(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&record.id),
            "record-job",
        )
        .await
        .expect("claim records"),
        1
    );

    let summary = crate::repositories::summaries::create_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &subject_id,
        "summary",
        None,
        None,
        0,
    )
    .await
    .expect("create summary");
    assert!(
        crate::repositories::summaries::get_pending_rollup_dispatch(&pool, &tenant, &source, &summary.id)
            .await
            .expect("rollup outbox")
            .is_some()
    );
    assert_eq!(
        crate::repositories::records::mark_claimed_records_summarized(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&record.id),
            "record-job",
            &summary.id,
        )
        .await
        .expect("summarize records"),
        1
    );
    let rollup_event = crate::repositories::summaries::claim_pending_rollup_dispatches(&pool, 10)
        .await
        .expect("claim rollup event")
        .into_iter()
        .find(|event| event.id == summary.id)
        .expect("pending rollup event");
    assert!(
        crate::repositories::summaries::mark_rollup_dispatch_consumed(&pool, &rollup_event)
            .await
            .expect("consume rollup")
    );
    assert!(crate::repositories::summaries::rearm_rollup_dispatch_if_eligible(
        &pool, &tenant, &source, &thread_id, 8,
    )
    .await
    .expect("rearm rollup")
    .is_some());

    let memory = crate::repositories::subject_memories::upsert_subject_memory(
        &pool,
        &subject_id,
        "profile:name",
        UpsertSubjectMemoryRequest {
            id: None,
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            memory_type: "profile".to_string(),
            text: "Alice".to_string(),
            level: Some(0),
            source_digest: None,
            confidence: Some(1.0),
            last_seen_at: None,
            metadata: Some(json!({"relation_subject_id":"agent-1"})),
            rollup_status: None,
            rollup_memory_key: None,
            rolled_up_at: None,
            status: None,
            created_at: None,
            updated_at: None,
        },
    )
    .await
    .expect("upsert subject memory");
    assert_eq!(memory.text, "Alice");
    assert_eq!(
        crate::repositories::subject_memories::list_subject_memories(
            &pool,
            &tenant,
            &source,
            &subject_id,
            None,
            None,
            10,
            0
        )
        .await
        .expect("list memories")
        .len(),
        1
    );
    assert_eq!(
        crate::repositories::subject_memories::mark_subject_memories_rolled_up(
            &pool,
            &tenant,
            &source,
            &subject_id,
            std::slice::from_ref(&memory.id),
            "profile:rollup",
        )
        .await
        .expect("roll up memories"),
        1
    );
    assert_eq!(
        crate::repositories::summaries::mark_summaries_subject_memory_summarized_for_scope(
            &pool,
            &tenant,
            &source,
            &thread_id,
            std::slice::from_ref(&summary.id),
            "scope-1",
        )
        .await
        .expect("mark summary scope"),
        1
    );

    let cloud_agent_repository = crate::repositories::cloud_agent::CloudAgentPostgresStore::new(pool.clone());
    let store = CloudAgentStateStore::from_repository(cloud_agent_repository.clone());
    let run_id = format!("memory-run-{suffix}");
    let run = create_cloud_agent_run(
        &store,
        NewCloudAgentRun {
            ordering_lane_key: format!("memory-lane-{suffix}"),
            agent_run_id: run_id.clone(),
            owner_service: "memory-engine".to_string(),
            owner_entity_type: "summary".to_string(),
            owner_entity_id: summary.id.clone(),
            owner_user_id: tenant.clone(),
            agent_key: "summary".to_string(),
            input: json!({"conversation":"stored only in Memory Engine"}),
            model_config_ref: "model".to_string(),
            model_runtime_snapshot_ref: "model".to_string(),
            agent_prompt_revision: "1".to_string(),
            agent_prompt_checksum: "checksum".to_string(),
            capability_policy_revision: "none".to_string(),
            mcp_runtime_session_ref: None,
            current_input_items_ref: "input".to_string(),
            max_iterations: 4,
            deadline_at: None,
            runtime_routing_key: "memory.contract".to_string(),
            start_causation_id: summary.id.clone(),
            start_payload: json!({"summary_id":summary.id}),
        },
    )
    .await
    .expect("create cloud run");
    assert_eq!(
        store
            .load_run(&run_id)
            .await
            .expect("load run")
            .expect("run")
            .input,
        run.input
    );

    let outbox_event_id = format!("cloud_agent_run_started_{run_id}_1_1");
    let publisher_a = format!("publisher-a:{suffix}");
    let publisher_b = format!("publisher-b:{suffix}");
    let claim_until = chrono::Utc::now() + chrono::Duration::seconds(30);
    let (claimed_a, claimed_b) = tokio::join!(
        cloud_agent_repository.claim_ready_outbox_with_attempts(10, &publisher_a, claim_until),
        cloud_agent_repository.claim_ready_outbox_with_attempts(10, &publisher_b, claim_until),
    );
    let claimed_a = claimed_a.expect("publisher A claim");
    let claimed_b = claimed_b.expect("publisher B claim");
    assert_eq!(claimed_a.len() + claimed_b.len(), 1);
    let winning_token = if claimed_a.is_empty() {
        publisher_b
    } else {
        publisher_a
    };
    assert!(!cloud_agent_repository
        .mark_claimed_outbox_published(&outbox_event_id, "publisher-wrong")
        .await
        .expect("wrong publisher token"));
    assert!(cloud_agent_repository
        .claim_ready_outbox_with_attempts(
            10,
            "publisher-blocked",
            chrono::Utc::now() + chrono::Duration::seconds(30),
        )
        .await
        .expect("active claim blocks a second publisher")
        .is_empty());

    sqlx::query(
        "UPDATE cloud_agent_outbox SET claim_until=now()-interval '1 second' WHERE event_id=$1",
    )
    .bind(&outbox_event_id)
    .execute(&pool)
    .await
    .expect("expire publisher lease");
    let recovery_token = format!("publisher-recovery:{suffix}");
    assert_eq!(
        cloud_agent_repository
            .claim_ready_outbox_with_attempts(
                10,
                &recovery_token,
                chrono::Utc::now() + chrono::Duration::seconds(30),
            )
            .await
            .expect("recover expired publisher lease")
            .len(),
        1
    );
    assert!(!cloud_agent_repository
        .mark_claimed_outbox_published(&outbox_event_id, &winning_token)
        .await
        .expect("stale publisher token"));
    assert!(cloud_agent_repository
        .mark_claimed_outbox_published(&outbox_event_id, &recovery_token)
        .await
        .expect("publish outbox"));

    let coordination_has_data: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='cloud_agent_runs' AND column_name='data')",
    ).fetch_one(&pool).await.expect("inspect coordination schema");
    assert!(!coordination_has_data);

    crate::repositories::threads::delete_thread(&pool, &tenant, &source, &thread_id)
        .await
        .expect("cleanup thread");
}
