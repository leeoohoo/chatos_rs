// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn canonical_project_scope_deactivates_its_legacy_duplicate() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let subject = format!("project:{suffix}");
    let legacy_key = format!("project_memory:{subject}");
    crate::repositories::subject_memory_scopes::upsert_subject_memory_scope(
        &pool,
        legacy_key.as_str(),
        crate::models::UpsertSubjectMemoryScopeRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: subject.clone(),
            memory_type: "project_memory".to_string(),
            source_thread_label: subject.clone(),
            relation_subject_id: Some(legacy_key.clone()),
            source_summary_type: Some("thread_incremental".to_string()),
            prompt_title: None,
            memory_metadata: None,
            status: Some("active".to_string()),
        },
    )
    .await
    .expect("create legacy project scope");
    assert!(
        crate::repositories::subject_memory_scopes::try_acquire_subject_memory_scope_slot(
            &pool,
            &tenant,
            &source,
            &legacy_key,
            "legacy-owner",
            300,
        )
        .await
        .expect("lock legacy project scope")
    );

    crate::repositories::subject_memory_scopes::ensure_project_identity_scope(
        &pool, &tenant, &source, &subject,
    )
    .await
    .expect("ensure canonical project scope");

    let legacy = crate::repositories::subject_memory_scopes::get_subject_memory_scope(
        &pool,
        &tenant,
        &source,
        &legacy_key,
    )
    .await
    .expect("load legacy project scope")
    .expect("legacy project scope remains auditable");
    let state = sqlx::query_as::<_, (String, bool, Option<String>)>(
        "SELECT subject_memory_status,subject_memory_dispatch_pending,lock_owner \
         FROM engine_subject_memory_scopes WHERE tenant_id=$1 AND source_id=$2 AND scope_key=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&legacy_key)
    .fetch_one(&pool)
    .await
    .expect("load legacy coordination state");

    assert_eq!(legacy.status, "inactive");
    assert_eq!(state, ("idle".to_string(), false, None));
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
    let summary = crate::repositories::summaries::insert_test_thread_summary(
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
    let summary = crate::repositories::summaries::insert_test_thread_summary(
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
