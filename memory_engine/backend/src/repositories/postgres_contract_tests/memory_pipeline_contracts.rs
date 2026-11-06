// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn configured_threshold_arms_summary_dispatch_without_a_runtime_default() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let agent_subject_id = format!("agent:{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject_id.clone(),
            thread_type: "task".to_string(),
            external_thread_id: None,
            title: None,
            labels: Some(vec!["memory-schema:v2".to_string(), agent_subject_id]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert summary threshold thread");
    crate::repositories::threads::apply_summary_queue_state_delta(
        &pool, &tenant, &source, &thread_id, 1, 899,
    )
    .await
    .expect("add pending tokens below configured threshold");

    let below = crate::repositories::threads::rearm_summary_dispatch_if_eligible(
        &pool, &tenant, &source, &thread_id, 900,
    )
    .await
    .expect("check below configured threshold");
    assert!(below.is_none());
    let eligible =
        crate::repositories::threads::list_eligible_summary_dispatches(&pool, 900, 10_000)
            .await
            .expect("scan summary candidates below configured threshold");
    assert!(!eligible
        .iter()
        .any(|candidate| candidate.tenant_id == tenant && candidate.thread_id == thread_id));

    crate::repositories::threads::apply_summary_queue_state_delta(
        &pool, &tenant, &source, &thread_id, 0, 1,
    )
    .await
    .expect("reach configured threshold");
    let armed = crate::repositories::threads::rearm_summary_dispatch_if_eligible(
        &pool, &tenant, &source, &thread_id, 900,
    )
    .await
    .expect("arm at configured threshold")
    .expect("summary dispatch is armed exactly at threshold");
    assert_eq!(armed.summary_dispatch_version, 1);
    assert!(!armed.forced());
}

#[tokio::test]
async fn thread_summary_dispatches_rollup_and_both_identity_memories() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("client-agent:todo:{suffix}");
    let agent_subject_id = format!("agent:{suffix}");
    let project_subject_id = format!("project:{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject_id.clone(),
            thread_type: "conversation".to_string(),
            external_thread_id: Some(thread_id.clone()),
            title: None,
            labels: Some(vec![
                "memory-schema:v2".to_string(),
                agent_subject_id.clone(),
                project_subject_id.clone(),
                "thread-kind:todo_execution".to_string(),
            ]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert project-bound todo thread");
    let summary = crate::repositories::summaries::create_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &agent_subject_id,
        "todo result",
        None,
        None,
        1,
        Some(&project_subject_id),
    )
    .await
    .expect("create thread summary");

    assert!(crate::repositories::summaries::get_pending_rollup_dispatch(
        &pool,
        &tenant,
        &source,
        &summary.id,
    )
    .await
    .expect("load rollup dispatch")
    .is_some());
    assert!(
        crate::repositories::summaries::get_pending_subject_memory_source_dispatch(
            &pool,
            &tenant,
            &source,
            &summary.id,
        )
        .await
        .expect("load subject-memory source dispatch")
        .is_some()
    );

    let labels = vec![agent_subject_id.clone(), project_subject_id.clone()];
    let scopes =
        crate::repositories::subject_memory_scopes::list_matching_active_subject_memory_scopes(
            &pool,
            &tenant,
            &source,
            labels.as_slice(),
            "thread_incremental",
            10,
        )
        .await
        .expect("match Agent and Project identity scopes");
    let typed_scopes = scopes
        .iter()
        .map(|scope| (scope.subject_id.as_str(), scope.memory_type.as_str()))
        .collect::<HashSet<_>>();
    assert_eq!(
        typed_scopes,
        HashSet::from([
            (agent_subject_id.as_str(), "agent_memory"),
            (project_subject_id.as_str(), "project_summary"),
        ])
    );

    for (scope_key, label) in [
        (
            format!("identity:{agent_subject_id}"),
            agent_subject_id.as_str(),
        ),
        (
            format!("identity:{project_subject_id}"),
            project_subject_id.as_str(),
        ),
    ] {
        let candidates = crate::repositories::summaries::list_summaries_by_thread_label_for_subject_memory_scope(
            &pool,
            &tenant,
            &source,
            label,
            "thread_incremental",
            scope_key.as_str(),
            10,
        )
        .await
        .expect("list identity-scoped summary candidates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, summary.id);
    }
}

#[tokio::test]
async fn stable_agent_thread_routes_each_summary_to_its_dynamic_project() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("client-agent:agent:{suffix}");
    let agent_subject_id = format!("agent:{suffix}");
    let project_subject_id = format!("project:{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject_id.clone(),
            thread_type: "conversation".to_string(),
            external_thread_id: Some(thread_id.clone()),
            title: None,
            labels: Some(vec![
                "memory-schema:v2".to_string(),
                agent_subject_id.clone(),
                "thread-kind:agent_conversation".to_string(),
            ]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert stable Agent thread");
    crate::repositories::subject_memory_scopes::ensure_project_identity_scope(
        &pool,
        &tenant,
        &source,
        &project_subject_id,
    )
    .await
    .expect("ensure dynamic Project identity scope");
    let summary = crate::repositories::summaries::create_thread_summary(
        &pool,
        &tenant,
        &source,
        &thread_id,
        &agent_subject_id,
        "project-scoped conversation",
        None,
        None,
        1,
        Some(&project_subject_id),
    )
    .await
    .expect("create project-scoped Agent summary");

    let candidates =
        crate::repositories::summaries::list_summaries_by_thread_label_for_subject_memory_scope(
            &pool,
            &tenant,
            &source,
            &project_subject_id,
            "thread_incremental",
            format!("identity:{project_subject_id}").as_str(),
            10,
        )
        .await
        .expect("select summary by its dynamic Project metadata");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, summary.id);
}
