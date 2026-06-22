use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{default_active, default_pending};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSubjectMemory {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub memory_key: String,
    pub memory_type: String,
    pub text: String,
    pub level: i64,
    pub source_digest: Option<String>,
    pub confidence: Option<f64>,
    pub last_seen_at: Option<String>,
    pub metadata: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default = "default_pending")]
    pub rollup_status: String,
    pub rollup_memory_key: Option<String>,
    pub rolled_up_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSubjectMemoryScope {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub scope_key: String,
    pub subject_id: String,
    pub memory_type: String,
    pub source_thread_label: String,
    pub relation_subject_id: Option<String>,
    pub source_summary_type: Option<String>,
    pub prompt_title: Option<String>,
    pub memory_metadata: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSubjectMemoryScopeRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub memory_type: String,
    pub source_thread_label: String,
    pub relation_subject_id: Option<String>,
    pub source_summary_type: Option<String>,
    pub prompt_title: Option<String>,
    pub memory_metadata: Option<Value>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySubjectMemoriesRequest {
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub memory_type: Option<String>,
    pub level: Option<i64>,
    pub max_level_exclusive: Option<i64>,
    pub rollup_status: Option<String>,
    pub relation_subject_id: Option<String>,
    pub source_digest: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkRunSubjectMemoryScopesRequest {
    pub tenant_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkUpsertSubjectMemoryScopeRequest {
    pub tenant_id: String,
    pub subject_id: String,
    pub memory_type: String,
    pub source_thread_label: String,
    pub relation_subject_id: Option<String>,
    pub source_summary_type: Option<String>,
    pub prompt_title: Option<String>,
    pub memory_metadata: Option<Value>,
    pub status: Option<String>,
}

pub type SystemUpsertSubjectMemoryScopeRequest = SdkUpsertSubjectMemoryScopeRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkQuerySubjectMemoriesRequest {
    pub tenant_id: String,
    pub subject_id: String,
    pub memory_type: Option<String>,
    pub level: Option<i64>,
    pub max_level_exclusive: Option<i64>,
    pub rollup_status: Option<String>,
    pub relation_subject_id: Option<String>,
    pub source_digest: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub type SystemQuerySubjectMemoriesRequest = SdkQuerySubjectMemoriesRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSubjectMemoryScopesResponse {
    pub processed_scopes: usize,
    pub generated_scopes: usize,
    pub generated_memories: usize,
    pub marked_source_summaries: usize,
    pub marked_source_memories: usize,
    pub failed_scopes: usize,
}

#[cfg(test)]
mod tests {
    use super::{EngineSubjectMemory, EngineSubjectMemoryScope};

    #[test]
    fn engine_subject_memory_defaults_status_fields() {
        let memory: EngineSubjectMemory = serde_json::from_value(serde_json::json!({
            "id": "mem-1",
            "tenant_id": "tenant-1",
            "source_id": "source-1",
            "subject_id": "subject-1",
            "memory_key": "profile:name",
            "memory_type": "profile",
            "text": "Alice",
            "level": 0,
            "source_digest": null,
            "confidence": null,
            "last_seen_at": null,
            "metadata": null,
            "rollup_memory_key": null,
            "rolled_up_at": null,
            "created_at": "2026-05-21T00:00:00Z",
            "updated_at": "2026-05-21T00:00:00Z"
        }))
        .expect("memory");

        assert_eq!(memory.status, "active");
        assert_eq!(memory.rollup_status, "pending");
    }

    #[test]
    fn engine_subject_memory_scope_defaults_status_to_active() {
        let scope: EngineSubjectMemoryScope = serde_json::from_value(serde_json::json!({
            "id": "scope-1",
            "tenant_id": "tenant-1",
            "source_id": "source-1",
            "scope_key": "scope-1",
            "subject_id": "subject-1",
            "memory_type": "profile",
            "source_thread_label": "support",
            "relation_subject_id": null,
            "source_summary_type": null,
            "prompt_title": null,
            "memory_metadata": null,
            "created_at": "2026-05-21T00:00:00Z",
            "updated_at": "2026-05-21T00:00:00Z",
            "last_run_at": null
        }))
        .expect("scope");

        assert_eq!(scope.status, "active");
    }
}
