use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{ComposeContextRequest, ComposeContextResponse};
use crate::services::memory_engine_client;

pub async fn compose_context(
    config: &AppConfig,
    pool: &Db,
    req: ComposeContextRequest,
) -> Result<ComposeContextResponse, String> {
    memory_engine_client::compose_context(config, pool, &req).await
}
