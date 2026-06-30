use reqwest::Method;

use crate::models::{
    EngineSubjectMemory, EngineSubjectMemoryScope, ListResponse, QuerySubjectMemoriesRequest,
    SdkQuerySubjectMemoriesRequest, SdkUpsertSubjectMemoryScopeRequest,
    SystemQuerySubjectMemoriesRequest, SystemUpsertSubjectMemoryScopeRequest,
    UpsertSubjectMemoryScopeRequest,
};

use super::{require_direct_source_id, AuthMode, MemoryEngineClient};

impl MemoryEngineClient {
    pub async fn upsert_subject_memory_scope(
        &self,
        scope_key: &str,
        req: &UpsertSubjectMemoryScopeRequest,
    ) -> Result<EngineSubjectMemoryScope, String> {
        match &self.auth {
            AuthMode::Direct { .. } => {
                self.send_json(
                    Method::PUT,
                    &format!("/subject-memory-scopes/{}", urlencoding::encode(scope_key)),
                    Some(req),
                )
                .await
            }
            AuthMode::SystemKey { .. } => {
                let direct = SdkUpsertSubjectMemoryScopeRequest {
                    tenant_id: req.tenant_id.clone(),
                    subject_id: req.subject_id.clone(),
                    memory_type: req.memory_type.clone(),
                    source_thread_label: req.source_thread_label.clone(),
                    relation_subject_id: req.relation_subject_id.clone(),
                    source_summary_type: req.source_summary_type.clone(),
                    prompt_title: req.prompt_title.clone(),
                    memory_metadata: req.memory_metadata.clone(),
                    status: req.status.clone(),
                };
                self.send_json(
                    Method::PUT,
                    &format!(
                        "/sdk/subject-memory-scopes/{}",
                        urlencoding::encode(scope_key)
                    ),
                    Some(&direct),
                )
                .await
            }
        }
    }

    pub async fn upsert_subject_memory_scope_system(
        &self,
        scope_key: &str,
        req: &SystemUpsertSubjectMemoryScopeRequest,
    ) -> Result<EngineSubjectMemoryScope, String> {
        match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id =
                    require_direct_source_id(source_id, "upsert_subject_memory_scope_system")?;
                let direct = UpsertSubjectMemoryScopeRequest {
                    tenant_id: req.tenant_id.clone(),
                    source_id: source_id.to_string(),
                    subject_id: req.subject_id.clone(),
                    memory_type: req.memory_type.clone(),
                    source_thread_label: req.source_thread_label.clone(),
                    relation_subject_id: req.relation_subject_id.clone(),
                    source_summary_type: req.source_summary_type.clone(),
                    prompt_title: req.prompt_title.clone(),
                    memory_metadata: req.memory_metadata.clone(),
                    status: req.status.clone(),
                };
                self.upsert_subject_memory_scope(scope_key, &direct).await
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(
                    Method::PUT,
                    &format!(
                        "/sdk/subject-memory-scopes/{}",
                        urlencoding::encode(scope_key)
                    ),
                    Some(req),
                )
                .await
            }
        }
    }

    pub async fn query_subject_memories(
        &self,
        req: &QuerySubjectMemoriesRequest,
    ) -> Result<Vec<EngineSubjectMemory>, String> {
        let resp: ListResponse<EngineSubjectMemory> = match &self.auth {
            AuthMode::Direct { .. } => {
                self.send_json(Method::POST, "/subject-memories/query", Some(req))
                    .await?
            }
            AuthMode::SystemKey { .. } => {
                let direct = SdkQuerySubjectMemoriesRequest {
                    tenant_id: req.tenant_id.clone(),
                    subject_id: req.subject_id.clone(),
                    memory_type: req.memory_type.clone(),
                    level: req.level,
                    max_level_exclusive: req.max_level_exclusive,
                    rollup_status: req.rollup_status.clone(),
                    relation_subject_id: req.relation_subject_id.clone(),
                    source_digest: req.source_digest.clone(),
                    limit: req.limit,
                    offset: req.offset,
                };
                self.send_json(Method::POST, "/sdk/subject-memories/query", Some(&direct))
                    .await?
            }
        };
        Ok(resp.items)
    }

    pub async fn query_subject_memories_system(
        &self,
        req: &SystemQuerySubjectMemoriesRequest,
    ) -> Result<Vec<EngineSubjectMemory>, String> {
        let resp: ListResponse<EngineSubjectMemory> = match &self.auth {
            AuthMode::Direct { source_id } => {
                let source_id =
                    require_direct_source_id(source_id, "query_subject_memories_system")?;
                let direct = QuerySubjectMemoriesRequest {
                    tenant_id: req.tenant_id.clone(),
                    source_id: source_id.to_string(),
                    subject_id: req.subject_id.clone(),
                    memory_type: req.memory_type.clone(),
                    level: req.level,
                    max_level_exclusive: req.max_level_exclusive,
                    rollup_status: req.rollup_status.clone(),
                    relation_subject_id: req.relation_subject_id.clone(),
                    source_digest: req.source_digest.clone(),
                    limit: req.limit,
                    offset: req.offset,
                };
                self.send_json(Method::POST, "/subject-memories/query", Some(&direct))
                    .await?
            }
            AuthMode::SystemKey { .. } => {
                self.send_json(Method::POST, "/sdk/subject-memories/query", Some(req))
                    .await?
            }
        };
        Ok(resp.items)
    }
}
