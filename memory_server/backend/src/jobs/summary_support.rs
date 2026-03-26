use crate::db::Db;
use crate::models::AiModelConfig;

use super::job_support;

pub(crate) async fn finish_failed_job_run(pool: &Db, job_run_id: &str, error_message: &str) {
    job_support::finish_failed_job_run(pool, job_run_id, error_message, "[MEMORY-SUMMARY-L0]")
        .await;
}

pub(crate) async fn resolve_model_config(
    pool: &Db,
    user_id: &str,
    model_config_id: Option<&str>,
) -> Result<Option<AiModelConfig>, String> {
    job_support::resolve_model_config(pool, user_id, model_config_id).await
}
