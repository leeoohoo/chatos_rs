// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

async fn create_summary_output(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    thread_id: &str,
    subject_id: &str,
) -> EngineSummary {
    crate::repositories::threads::upsert_thread(
        pool,
        thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.to_string(),
            source_id: source.to_string(),
            subject_id: subject_id.to_string(),
            thread_type: "client_agent".to_string(),
            external_thread_id: None,
            title: None,
            labels: None,
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create fact contract thread");
    crate::repositories::summaries::insert_test_thread_summary(
        pool,
        tenant,
        source,
        thread_id,
        subject_id,
        "fact contract summary",
        None,
        None,
        1,
        Some("project:alpha"),
    )
    .await
    .expect("create fact contract summary")
}

async fn insert_fact(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    output_id: &str,
    fact_id: &str,
    scope_kind: &str,
    scope_key: &str,
    project_partition_key: Option<&str>,
    value_hash: &str,
    searchable_text: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO engine_memory_fact_projections(tenant_id,source_id,fact_id,output_kind, \
         output_id,scope_kind,scope_key,project_partition_key,fact_key,normalized_value_hash, \
         searchable_text,status,confidence,source_refs,created_at) \
         VALUES($1,$2,$3,'thread_summary',$4,$5,$6,$7,'contract.fact',$8, \
         $9,'active','model_inference','[\"record-1\"]'::jsonb,$10::timestamptz)",
    )
    .bind(tenant)
    .bind(source)
    .bind(fact_id)
    .bind(output_id)
    .bind(scope_kind)
    .bind(scope_key)
    .bind(project_partition_key)
    .bind(value_hash)
    .bind(searchable_text)
    .bind(created_at)
    .execute(pool)
    .await
    .expect("insert fact projection");
}

#[tokio::test]
async fn fact_projection_rejects_exact_duplicate_and_retires_with_output() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let summary = create_summary_output(&pool, &tenant, &source, &thread_id, &subject).await;
    let generation = crate::repositories::generation_commits::GenerationCommit {
        tenant_id: tenant.clone(),
        source_id: source.clone(),
        generation_id: format!("generation-{suffix}"),
        job_run_id: format!("job-{suffix}"),
        generation_step_key: "fact-contract".to_string(),
        generation_kind: "thread_summary".to_string(),
        input_digest: format!("sha256:{}", "a".repeat(64)),
        input_manifest: sqlx::types::Json(json!({
            "inputs": [{"id": "record-1"}]
        })),
        config_snapshot: sqlx::types::Json(json!({})),
        project_partition_key: Some("project:alpha".to_string()),
        status: "running".to_string(),
        attempt_count: 1,
        lease_owner: None,
        lease_token: None,
        output_kind: None,
        output_id: None,
    };
    let mut duplicate_payload = crate::models::structured_payload::build_generated_payload(
        generation.generation_id.as_str(),
        crate::models::structured_payload::GeneratedPayloadScope {
            kind: "thread",
            subject_id: subject.as_str(),
            project_partition_key: Some("project:alpha"),
        },
        "same exact fact",
        &["record-1".to_string()],
        None,
    )
    .expect("build duplicate fact payload");
    let duplicate = duplicate_payload["open_items"][0].clone();
    duplicate_payload["open_items"]
        .as_array_mut()
        .expect("open items")
        .push(duplicate);
    let mut tx = pool
        .begin()
        .await
        .expect("begin duplicate fact transaction");
    let duplicate_error = crate::repositories::memory_facts::project_payload(
        &mut tx,
        &generation,
        crate::repositories::memory_facts::FactOutputScope {
            output_kind: "thread_summary",
            output_id: summary.id.as_str(),
            scope_kind: "thread",
            scope_key: thread_id.as_str(),
            subject_id: subject.as_str(),
            project_partition_key: Some("project:alpha"),
        },
        crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA,
        &duplicate_payload,
    )
    .await
    .expect_err("exact duplicate fact must fail");
    assert!(duplicate_error.contains("duplicate fact_id"));
    tx.rollback()
        .await
        .expect("rollback duplicate fact transaction");

    let payload = crate::models::structured_payload::build_generated_payload(
        generation.generation_id.as_str(),
        crate::models::structured_payload::GeneratedPayloadScope {
            kind: "thread",
            subject_id: subject.as_str(),
            project_partition_key: Some("project:alpha"),
        },
        "active fact",
        &["record-1".to_string()],
        None,
    )
    .expect("build active fact payload");
    let mut tx = pool.begin().await.expect("begin active fact transaction");
    crate::repositories::memory_facts::project_payload(
        &mut tx,
        &generation,
        crate::repositories::memory_facts::FactOutputScope {
            output_kind: "thread_summary",
            output_id: summary.id.as_str(),
            scope_kind: "thread",
            scope_key: thread_id.as_str(),
            subject_id: subject.as_str(),
            project_partition_key: Some("project:alpha"),
        },
        crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA,
        &payload,
    )
    .await
    .expect("project active fact");
    assert_eq!(
        crate::repositories::memory_facts::retire_output(
            &mut tx,
            &tenant,
            &source,
            "thread_summary",
            &summary.id,
        )
        .await
        .expect("retire output facts"),
        1
    );
    tx.commit().await.expect("commit fact retirement");
    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM engine_memory_fact_projections \
         WHERE tenant_id=$1 AND source_id=$2 AND output_id=$3",
    )
    .bind(&tenant)
    .bind(&source)
    .bind(&summary.id)
    .fetch_one(&pool)
    .await
    .expect("load retired fact");
    assert_eq!(status, "retired");
}

#[tokio::test]
async fn fact_supersession_rejects_cross_scope_self_loop_cycle_and_older_to_newer() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let summary = create_summary_output(&pool, &tenant, &source, &thread_id, &subject).await;
    for (id, scope_key, hash, created_at) in [
        ("a", thread_id.as_str(), "hash-a", "2026-11-09T00:00:01Z"),
        ("b", thread_id.as_str(), "hash-b", "2026-11-09T00:00:02Z"),
        ("c", thread_id.as_str(), "hash-c", "2026-11-09T00:00:03Z"),
        (
            "other",
            "thread:other",
            "hash-other",
            "2026-11-09T00:00:04Z",
        ),
    ] {
        insert_fact(
            &pool,
            &tenant,
            &source,
            &summary.id,
            format!("fact-{suffix}-{id}").as_str(),
            "thread",
            scope_key,
            Some("project:alpha"),
            hash,
            hash,
            created_at,
        )
        .await;
    }
    let a = format!("fact-{suffix}-a");
    let b = format!("fact-{suffix}-b");
    let c = format!("fact-{suffix}-c");
    let other = format!("fact-{suffix}-other");
    let insert_edge = |newer: &str, older: &str| {
        sqlx::query(
            "INSERT INTO engine_memory_fact_supersessions(tenant_id,source_id,newer_fact_id, \
             superseded_fact_id) VALUES($1,$2,$3,$4)",
        )
        .bind(&tenant)
        .bind(&source)
        .bind(newer.to_string())
        .bind(older.to_string())
    };
    let self_loop = insert_edge(&a, &a)
        .execute(&pool)
        .await
        .expect_err("self loop rejected");
    assert!(self_loop.to_string().contains("check constraint"));
    let cross_scope = insert_edge(&other, &a)
        .execute(&pool)
        .await
        .expect_err("cross scope rejected");
    assert!(cross_scope.to_string().contains("crosses scope"));
    let older_to_newer = insert_edge(&a, &b)
        .execute(&pool)
        .await
        .expect_err("older to newer rejected");
    assert!(older_to_newer.to_string().contains("older memory fact"));
    insert_edge(&b, &a)
        .execute(&pool)
        .await
        .expect("insert b supersedes a");
    insert_edge(&c, &b)
        .execute(&pool)
        .await
        .expect("insert c supersedes b");
    let cycle = insert_edge(&a, &c)
        .execute(&pool)
        .await
        .expect_err("cycle rejected");
    assert!(cycle.to_string().contains("cycle") || cycle.to_string().contains("older memory fact"));
}

#[tokio::test]
async fn lexical_retrieval_handles_chinese_and_enforces_thread_project_agent_scope() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let summary = create_summary_output(&pool, &tenant, &source, &thread_id, &subject).await;
    for (id, scope_kind, scope_key, partition, text) in [
        (
            "thread",
            "thread",
            thread_id.as_str(),
            Some("project:alpha"),
            "open_items storage.database 当前数据库使用 sqlite",
        ),
        (
            "project-alpha",
            "project",
            "project:alpha",
            Some("project:alpha"),
            "decisions storage.database 项目数据库采用 sqlite",
        ),
        (
            "project-beta",
            "project",
            "project:beta",
            Some("project:beta"),
            "decisions storage.database 另一个项目数据库采用 sqlite",
        ),
        (
            "agent",
            "agent",
            subject.as_str(),
            None,
            "stable_preferences storage.database agent 偏好 sqlite",
        ),
    ] {
        insert_fact(
            &pool,
            &tenant,
            &source,
            &summary.id,
            format!("fact-{suffix}-{id}").as_str(),
            scope_kind,
            scope_key,
            partition,
            format!("hash-{id}").as_str(),
            text,
            "2026-11-09T00:00:01Z",
        )
        .await;
    }
    let facts = crate::repositories::memory_facts::search_relevant(
        &pool,
        &tenant,
        &source,
        &thread_id,
        "project:alpha",
        std::slice::from_ref(&subject),
        "数据库 sqlite",
        20,
    )
    .await
    .expect("search Chinese memory facts");
    assert_eq!(facts.len(), 3);
    assert!(facts.iter().any(|fact| fact.scope_kind == "thread"));
    assert!(facts
        .iter()
        .any(|fact| fact.scope_kind == "project" && fact.scope_key == "project:alpha"));
    assert!(facts.iter().any(|fact| fact.scope_kind == "agent"));
    assert!(!facts.iter().any(|fact| fact.scope_key == "project:beta"));
}
