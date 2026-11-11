// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use uuid::Uuid;

use super::{verify_and_consume, ExpectedRuntimeContextScope};
use crate::models::UpsertThreadRequest;

async fn pool() -> Option<crate::db::Db> {
    let url = std::env::var("MEMORY_ENGINE_DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()
}

fn test_config(database_url: String) -> crate::config::AppConfig {
    crate::config::AppConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        database_url,
        ai_request_timeout_secs: 5,
        api_enabled: true,
        worker_enabled: false,
        worker_interval_secs: 30,
        worker_max_threads_per_tick: 1,
        worker_summary_concurrency: 1,
        worker_rollup_concurrency: 1,
        worker_subject_memory_concurrency: 1,
        worker_reconcile_concurrency: 1,
        cloud_agent_outbox_reconcile_interval: Duration::from_secs(1),
        cloud_agent_outbox_batch_size: 10,
        summary_retry_delay: Duration::from_millis(100),
        summary_outbox_reconcile_interval: Duration::from_secs(1),
        summary_outbox_batch_size: 10,
        rollup_retry_delay: Duration::from_millis(100),
        rollup_outbox_reconcile_interval: Duration::from_secs(1),
        rollup_outbox_batch_size: 10,
        subject_memory_retry_delay: Duration::from_millis(100),
        subject_memory_outbox_reconcile_interval: Duration::from_secs(1),
        subject_memory_outbox_batch_size: 10,
        subject_memory_lock_timeout_secs: 300,
        record_sync_lease_timeout_secs: 300,
        rollup_lock_timeout_secs: 300,
        internal_api_secrets: HashMap::new(),
        require_signed_internal_requests: true,
        runtime_context_assertion_keys: crate::config::test_runtime_context_assertion_keys(),
        model_capabilities: crate::config::test_model_capabilities(),
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_internal_base_url: "https://127.0.0.1:39192".to_string(),
        user_service_internal_http: reqwest::Client::new(),
        user_service_request_timeout_ms: 300,
    }
}

const KEY_ID: &str = "test-runtime-context-key";
const ISSUER: &str = "test-runtime";
const SECRET: &str = "test-runtime-context-assertion-secret-long-enough";

async fn create_thread(
    pool: &crate::db::Db,
    tenant: &str,
    source: &str,
    thread_id: &str,
    subject_id: &str,
) {
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
            labels: Some(vec!["memory-schema:v2".to_string(), subject_id.to_string()]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("create assertion contract thread");
}

fn issue(
    tenant: &str,
    source: &str,
    subject: &str,
    thread_id: &str,
    partition: &str,
    operations: &[&str],
    key_id: &str,
) -> String {
    memory_engine_sdk::issue_runtime_context_assertion(
        memory_engine_sdk::IssueRuntimeContextAssertion {
            secret: SECRET,
            key_id,
            issuer: ISSUER,
            principal_id: tenant,
            tenant_id: tenant,
            source_id: source,
            canonical_agent_subject_id: subject,
            thread_id,
            project_partition_key: partition,
            allowed_operations: operations,
            runtime_identity_revision: "runtime-identity-contract-v1",
            ttl_seconds: 60,
        },
    )
    .expect("issue contract assertion")
}

#[tokio::test]
async fn runtime_context_assertion_is_scope_bound_fail_closed_and_single_use() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let thread_id = format!("thread-{suffix}");
    let subject = format!("agent:{suffix}");
    let partition = format!("project:{suffix}");
    create_thread(&pool, &tenant, &source, &thread_id, &subject).await;
    let config = test_config("unused".to_string());
    let expected = || ExpectedRuntimeContextScope {
        tenant_id: tenant.as_str(),
        source_id: source.as_str(),
        thread_id: thread_id.as_str(),
        project_partition_key: partition.as_str(),
        operation: "compose",
    };

    let valid = issue(
        &tenant,
        &source,
        &subject,
        &thread_id,
        &partition,
        &["compose"],
        KEY_ID,
    );
    let claims = verify_and_consume(&pool, &config, &valid, expected())
        .await
        .expect("consume valid assertion");
    assert_eq!(claims.canonical_agent_subject_id, subject);
    let replay = verify_and_consume(&pool, &config, &valid, expected())
        .await
        .expect_err("replayed assertion must fail");
    assert!(replay.contains("replay"));

    for (label, token, scope) in [
        (
            "tenant",
            issue(
                "other-tenant",
                &source,
                &subject,
                &thread_id,
                &partition,
                &["compose"],
                KEY_ID,
            ),
            expected(),
        ),
        (
            "source",
            issue(
                &tenant,
                "other-source",
                &subject,
                &thread_id,
                &partition,
                &["compose"],
                KEY_ID,
            ),
            expected(),
        ),
        (
            "thread",
            issue(
                &tenant,
                &source,
                &subject,
                "other-thread",
                &partition,
                &["compose"],
                KEY_ID,
            ),
            expected(),
        ),
        (
            "project",
            issue(
                &tenant,
                &source,
                &subject,
                &thread_id,
                "project:other",
                &["compose"],
                KEY_ID,
            ),
            expected(),
        ),
        (
            "operation",
            issue(
                &tenant,
                &source,
                &subject,
                &thread_id,
                &partition,
                &["record_ingest"],
                KEY_ID,
            ),
            expected(),
        ),
    ] {
        let error = verify_and_consume(&pool, &config, &token, scope)
            .await
            .expect_err("mismatched assertion must fail");
        assert!(!error.is_empty(), "{label} mismatch returned no error");
    }

    let unknown_key = issue(
        &tenant,
        &source,
        &subject,
        &thread_id,
        &partition,
        &["compose"],
        "unknown-key",
    );
    assert!(
        verify_and_consume(&pool, &config, &unknown_key, expected(),)
            .await
            .expect_err("unknown key must fail")
            .contains("not trusted")
    );

    let wrong_agent = issue(
        &tenant,
        &source,
        "agent:other",
        &thread_id,
        &partition,
        &["compose"],
        KEY_ID,
    );
    assert!(
        verify_and_consume(&pool, &config, &wrong_agent, expected(),)
            .await
            .expect_err("wrong agent must fail")
            .contains("agent does not match")
    );

    let mut no_keys = test_config("unused".to_string());
    no_keys.runtime_context_assertion_keys.clear();
    let no_verifier = issue(
        &tenant,
        &source,
        &subject,
        &thread_id,
        &partition,
        &["compose"],
        KEY_ID,
    );
    assert!(
        verify_and_consume(&pool, &no_keys, &no_verifier, expected(),)
            .await
            .is_err()
    );
}
