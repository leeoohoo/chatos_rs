// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::{stream, StreamExt};
use tracing::info;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    EngineSubjectMemoryScope, RunSubjectMemoryJobRequest, RunSubjectMemoryJobResponse,
    RunSubjectMemoryScopesResponse,
};
use crate::repositories::subject_memory_scopes;

use super::job::{run_subject_memory_job_internal, subject_memory_job_has_work};

enum ScopeExecutionOutcome {
    Success(RunSubjectMemoryJobResponse),
    Failed {
        tenant_id: String,
        source_id: String,
        scope_key: String,
        error: String,
        completed_result: Option<RunSubjectMemoryJobResponse>,
    },
}

pub async fn run_registered_subject_memory_scopes(
    config: &AppConfig,
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<RunSubjectMemoryScopesResponse, String> {
    run_registered_subject_memory_scopes_internal(config, db, tenant_id, source_id, limit).await
}

async fn run_registered_subject_memory_scopes_internal(
    config: &AppConfig,
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<RunSubjectMemoryScopesResponse, String> {
    let scopes =
        subject_memory_scopes::list_active_subject_memory_scopes(db, tenant_id, source_id, limit)
            .await?;
    if scopes.is_empty() {
        return Ok(RunSubjectMemoryScopesResponse {
            processed_scopes: 0,
            generated_scopes: 0,
            generated_memories: 0,
            marked_source_summaries: 0,
            marked_source_memories: 0,
            failed_scopes: 0,
        });
    }

    let mut out = RunSubjectMemoryScopesResponse {
        processed_scopes: scopes.len(),
        generated_scopes: 0,
        generated_memories: 0,
        marked_source_summaries: 0,
        marked_source_memories: 0,
        failed_scopes: 0,
    };

    let concurrency = subject_memory_scope_concurrency(config, limit);
    let db = db.clone();
    let config = config.clone();
    let execution_results = stream::iter(scopes.into_iter().map(|scope| {
        let db = db.clone();
        let config = config.clone();
        async move {
            let tenant_id = scope.tenant_id.clone();
            let source_id = scope.source_id.clone();
            let scope_key = scope.scope_key.clone();
            match run_scope_once(&config, &db, &scope).await {
                Ok(result) => match scope_has_pending_work(&db, &scope).await {
                    Ok(true) => match subject_memory_scopes::rearm_subject_memory_dispatch(
                        &db,
                        scope.tenant_id.as_str(),
                        scope.source_id.as_str(),
                        scope.scope_key.as_str(),
                    )
                    .await
                    {
                        Ok(Some(event)) if event.subject_memory_dispatch_pending => {
                            if let Err(err) = crate::subject_memory_queue::publish_pending_scope(
                                &config,
                                &db,
                                scope.tenant_id.as_str(),
                                scope.source_id.as_str(),
                                scope.scope_key.as_str(),
                            )
                            .await
                            {
                                tracing::warn!(
                                    scope_key = scope.scope_key.as_str(),
                                    error = err.as_str(),
                                    "Memory Engine left manually rearmed subject memory scope event in Outbox"
                                );
                            }
                            ScopeExecutionOutcome::Success(result)
                        }
                        Ok(_) => ScopeExecutionOutcome::Success(result),
                        Err(error) => ScopeExecutionOutcome::Failed {
                            tenant_id,
                            source_id,
                            scope_key,
                            error: format!("rearm subject memory scope failed: {error}"),
                            completed_result: Some(result),
                        },
                    },
                    Ok(false) => ScopeExecutionOutcome::Success(result),
                    Err(error) => ScopeExecutionOutcome::Failed {
                        tenant_id,
                        source_id,
                        scope_key,
                        error: format!("check pending subject memory scope work failed: {error}"),
                        completed_result: Some(result),
                    },
                },
                Err(error) => ScopeExecutionOutcome::Failed {
                    tenant_id,
                    source_id,
                    scope_key,
                    error,
                    completed_result: None,
                },
            }
        }
    }))
    .buffer_unordered(concurrency)
    .collect::<Vec<_>>()
    .await;

    for outcome in execution_results {
        match outcome {
            ScopeExecutionOutcome::Success(result) => {
                if result.generated_memories > 0 {
                    out.generated_scopes += 1;
                }
                out.generated_memories += result.generated_memories;
                out.marked_source_summaries += result.marked_source_summaries;
                out.marked_source_memories += result.marked_source_memories;
            }
            ScopeExecutionOutcome::Failed {
                tenant_id,
                source_id,
                scope_key,
                error,
                completed_result,
            } => {
                if let Some(result) = completed_result {
                    if result.generated_memories > 0 {
                        out.generated_scopes += 1;
                    }
                    out.generated_memories += result.generated_memories;
                    out.marked_source_summaries += result.marked_source_summaries;
                    out.marked_source_memories += result.marked_source_memories;
                }
                out.failed_scopes += 1;
                info!(
                    "[MEMORY-ENGINE-SUBJECT] scope run failed tenant_id={} source_id={} scope_key={} error={}",
                    tenant_id, source_id, scope_key, error
                );
            }
        }
    }

    Ok(out)
}

pub(crate) async fn run_scope_once(
    config: &AppConfig,
    db: &Db,
    scope: &EngineSubjectMemoryScope,
) -> Result<RunSubjectMemoryJobResponse, String> {
    let lock_owner = format!("subject-memory:{}", Uuid::new_v4());
    let acquired = subject_memory_scopes::try_acquire_subject_memory_scope_slot(
        db,
        scope.tenant_id.as_str(),
        scope.source_id.as_str(),
        scope.scope_key.as_str(),
        lock_owner.as_str(),
        config.subject_memory_lock_timeout_secs,
    )
    .await?;
    if !acquired {
        return Err("subject memory scope slot already occupied".to_string());
    }

    let result = async {
        subject_memory_scopes::touch_subject_memory_scope_run(
            db,
            scope.tenant_id.as_str(),
            scope.source_id.as_str(),
            scope.scope_key.as_str(),
        )
        .await?;
        run_subject_memory_job_internal(config, db, scope_job_request(scope), true).await
    }
    .await;
    let release_result = subject_memory_scopes::release_subject_memory_scope_slot(
        db,
        scope.tenant_id.as_str(),
        scope.source_id.as_str(),
        scope.scope_key.as_str(),
        lock_owner.as_str(),
    )
    .await;
    match (result, release_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(format!("release subject memory scope slot failed: {err}")),
    }
}

pub(crate) async fn scope_has_pending_work(
    db: &Db,
    scope: &EngineSubjectMemoryScope,
) -> Result<bool, String> {
    subject_memory_job_has_work(db, &scope_job_request(scope)).await
}

pub(crate) fn scope_job_request(scope: &EngineSubjectMemoryScope) -> RunSubjectMemoryJobRequest {
    RunSubjectMemoryJobRequest {
        tenant_id: scope.tenant_id.clone(),
        source_id: scope.source_id.clone(),
        subject_id: scope.subject_id.clone(),
        memory_type: scope.memory_type.clone(),
        source_thread_label: scope.source_thread_label.clone(),
        relation_subject_id: scope.relation_subject_id.clone(),
        source_summary_type: scope.source_summary_type.clone(),
        summary_prompt: None,
        rollup_summary_prompt: None,
        prompt_title: scope.prompt_title.clone(),
        token_limit: None,
        target_summary_tokens: None,
        count_limit: None,
        keep_level0_count: None,
        max_level: None,
        memory_metadata: scope.memory_metadata.clone(),
        scope_key: Some(scope.scope_key.clone()),
    }
}

fn subject_memory_scope_concurrency(config: &AppConfig, limit: i64) -> usize {
    limit
        .max(1)
        .min(config.worker_subject_memory_concurrency.max(1) as i64) as usize
}
