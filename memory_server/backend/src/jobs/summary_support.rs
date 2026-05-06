use crate::db::Db;
use crate::models::AiModelConfig;

use super::job_support;

pub(crate) async fn finish_failed_job_run(pool: &Db, job_run_id: &str, error_message: &str) {
    job_support::finish_failed_job_run(pool, job_run_id, error_message, "[MEMORY-SUMMARY-L0]")
        .await;
}

pub(crate) async fn update_failed_job_run_diagnostics(
    pool: &Db,
    job_run_id: &str,
    pending_before_count: Option<i64>,
    selected_count: Option<i64>,
    marked_count: Option<i64>,
    pending_after_count: Option<i64>,
) {
    job_support::update_failed_job_run_diagnostics(
        pool,
        job_run_id,
        pending_before_count,
        selected_count,
        marked_count,
        pending_after_count,
        "[MEMORY-SUMMARY-L0]",
    )
    .await;
}

pub(crate) async fn resolve_model_config(
    pool: &Db,
    user_id: &str,
    model_config_id: Option<&str>,
) -> Result<Option<AiModelConfig>, String> {
    job_support::resolve_model_config(pool, user_id, model_config_id).await
}
