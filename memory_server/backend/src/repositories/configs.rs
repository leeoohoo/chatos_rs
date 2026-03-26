mod job_configs;
mod model_configs;

use crate::db::Db;

pub use self::job_configs::{
    get_agent_memory_job_config, get_effective_agent_memory_job_config,
    get_effective_summary_job_config, get_effective_summary_rollup_job_config,
    get_summary_job_config, get_summary_rollup_job_config, upsert_agent_memory_job_config,
    upsert_summary_job_config, upsert_summary_rollup_job_config,
};
pub use self::model_configs::{
    create_model_config, delete_model_config, get_model_config_by_id, list_model_configs,
    update_model_config,
};

pub async fn delete_user_configs(db: &Db, user_id: &str) -> Result<(), String> {
    model_configs::delete_user_model_configs(db, user_id).await?;
    job_configs::delete_user_job_configs(db, user_id).await
}
