// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{default_active, default_idle};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkUpsertThreadRequest {
    pub tenant_id: String,
    pub subject_id: String,
    pub thread_type: String,
    pub external_thread_id: Option<String>,
    pub title: Option<String>,
    pub labels: Option<Vec<String>>,
    pub metadata: Option<Value>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineThread {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub subject_id: String,
    pub thread_type: String,
    pub external_thread_id: Option<String>,
    pub title: Option<String>,
    pub labels: Option<Vec<String>>,
    pub metadata: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default = "default_idle")]
    pub summary_status: String,
    pub summary_job_run_id: Option<String>,
    pub summary_locked_at: Option<String>,
    pub summary_lock_expires_at: Option<String>,
    #[serde(default)]
    pub pending_record_count: i64,
    #[serde(default)]
    pub pending_summary_tokens: i64,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkGetThreadRequest {
    pub tenant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkListThreadsRequest {
    pub tenant_id: String,
    pub subject_id: Option<String>,
    pub external_thread_id: Option<String>,
    pub session_id: Option<String>,
    pub contact_id: Option<String>,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub mapping_source: Option<String>,
    pub mapping_version: Option<String>,
    pub thread_label: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteThreadResponse {
    pub deleted_thread: bool,
    pub deleted_records: i64,
    pub deleted_summaries: i64,
    pub deleted_snapshots: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetThreadResponse {
    pub item: Option<EngineThread>,
}

#[cfg(test)]
mod tests {
    use super::{DeleteThreadResponse, EngineThread};

    #[test]
    fn engine_thread_deserializes_summary_state_fields() {
        let thread: EngineThread = serde_json::from_value(serde_json::json!({
            "id": "thread-1",
            "tenant_id": "tenant-1",
            "source_id": "source-1",
            "subject_id": "subject-1",
            "thread_type": "chat",
            "external_thread_id": null,
            "title": "Demo",
            "labels": ["a"],
            "metadata": null,
            "status": "active",
            "summary_status": "running",
            "summary_job_run_id": "job-1",
            "summary_locked_at": "2026-05-21T00:00:00Z",
            "summary_lock_expires_at": "2026-05-21T00:05:00Z",
            "pending_record_count": 3,
            "pending_summary_tokens": 128,
            "created_at": "2026-05-21T00:00:00Z",
            "updated_at": "2026-05-21T00:00:00Z",
            "archived_at": null
        }))
        .expect("thread");

        assert_eq!(thread.summary_status, "running");
        assert_eq!(thread.pending_record_count, 3);
        assert_eq!(thread.pending_summary_tokens, 128);
        assert_eq!(thread.summary_job_run_id.as_deref(), Some("job-1"));
    }

    #[test]
    fn delete_thread_response_deserializes_snapshot_count() {
        let resp: DeleteThreadResponse = serde_json::from_value(serde_json::json!({
            "deleted_thread": true,
            "deleted_records": 12,
            "deleted_summaries": 3,
            "deleted_snapshots": 4
        }))
        .expect("delete response");

        assert!(resp.deleted_thread);
        assert_eq!(resp.deleted_records, 12);
        assert_eq!(resp.deleted_summaries, 3);
        assert_eq!(resp.deleted_snapshots, 4);
    }

    #[test]
    fn engine_thread_defaults_status_fields() {
        let thread: EngineThread = serde_json::from_value(serde_json::json!({
            "id": "thread-1",
            "tenant_id": "tenant-1",
            "source_id": "source-1",
            "subject_id": "subject-1",
            "thread_type": "chat",
            "external_thread_id": null,
            "title": null,
            "labels": null,
            "metadata": null,
            "summary_job_run_id": null,
            "summary_locked_at": null,
            "summary_lock_expires_at": null,
            "pending_record_count": 0,
            "pending_summary_tokens": 0,
            "created_at": "2026-05-21T00:00:00Z",
            "updated_at": "2026-05-21T00:00:00Z",
            "archived_at": null
        }))
        .expect("thread");

        assert_eq!(thread.status, "active");
        assert_eq!(thread.summary_status, "idle");
    }
}
