use serde_json::json;

use crate::config::Config;

use super::{chatos_memory_engine::CHATOS_COMPAT_SOURCE_ID, memory_engine_client};

const CHATOS_SOURCE_TYPE: &str = "chat_app";
const CHATOS_SOURCE_NAME: &str = "Chatos";

#[derive(Debug, Clone)]
pub struct EnsureChatosMemoryEngineSourceReport {
    pub source_id: String,
    pub source_type: String,
    pub sdk_enabled: bool,
    pub status: String,
}

pub async fn ensure_chatos_memory_engine_source()
-> Result<EnsureChatosMemoryEngineSourceReport, String> {
    let _cfg = Config::try_get()?;

    let source = memory_engine_client::upsert_source(
        CHATOS_COMPAT_SOURCE_ID,
        &memory_engine_client::UpsertEngineSourceRequestDto {
            tenant_id: None,
            source_type: CHATOS_SOURCE_TYPE.to_string(),
            name: CHATOS_SOURCE_NAME.to_string(),
            description: Some(
                "Chatos integration managed by chat_app_server_rs for threads, records, summaries, snapshots, review repair, and context compose.".to_string(),
            ),
            config: Some(json!({
                "platform_managed": true,
                "owner_service": "chat_app_server_rs",
                "mapping_version": "chatos_sdk.v1",
                "mapping_mode": "session_subject_with_contact_project_labels",
                "capabilities": [
                    "threads",
                    "records",
                    "summaries",
                    "snapshots",
                    "subject_memories",
                    "context_compose",
                    "review_repair"
                ],
            })),
            sdk_enabled: Some(true),
            status: Some("active".to_string()),
        },
    )
    .await?;

    Ok(EnsureChatosMemoryEngineSourceReport {
        source_id: source.source_id,
        source_type: source.source_type,
        sdk_enabled: source.sdk_enabled,
        status: source.status,
    })
}
