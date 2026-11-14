// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_active;

fn default_enabled() -> bool {
    true
}

fn default_model_request_max_retries() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineModelProfile {
    pub id: String,
    pub owner_user_id: Option<String>,
    pub owner_username: Option<String>,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub supports_images: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub supports_responses: bool,
    pub model_capability_revision: String,
    pub context_window_tokens: i64,
    pub output_reserve_tokens: i64,
    pub protocol_overhead_estimator_id: String,
    pub tokenizer_id: Option<String>,
    pub temperature: Option<f64>,
    pub thinking_level: Option<String>,
    #[serde(default = "default_model_request_max_retries")]
    pub model_request_max_retries: usize,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobPolicy {
    pub job_type: String,
    pub model_capability_revision: String,
    pub context_window_tokens: i64,
    pub output_reserve_tokens: i64,
    pub max_model_input_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSource {
    pub id: String,
    pub tenant_id: Option<String>,
    pub source_id: String,
    pub source_type: String,
    pub name: String,
    pub description: Option<String>,
    pub config: Option<Value>,
    #[serde(default = "default_active")]
    pub status: String,
    #[serde(default)]
    pub sdk_enabled: bool,
    pub secret_key_hint: Option<String>,
    pub key_last_rotated_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSourcesRequest {
    pub tenant_id: Option<String>,
    pub source_type: Option<String>,
    pub status: Option<String>,
    pub sdk_enabled: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSourceRequest {
    pub tenant_id: Option<String>,
    pub source_type: String,
    pub name: String,
    pub description: Option<String>,
    pub config: Option<Value>,
    pub sdk_enabled: Option<bool>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotateSourceSecretResponse {
    pub source: EngineSource,
    pub secret_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkAuthStatusResponse {
    pub source_id: String,
    pub tenant_id: Option<String>,
    pub source_type: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub sdk_enabled: bool,
    pub secret_key_hint: Option<String>,
    pub key_last_rotated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobRun {
    pub id: String,
    pub job_type: String,
    pub trigger_type: String,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub thread_id: Option<String>,
    pub subject_id: Option<String>,
    pub thread_label: Option<String>,
    pub thread_display_name: Option<String>,
    pub status: String,
    pub input_count: i64,
    pub output_count: i64,
    pub processed_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub metadata: Option<Value>,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRunsBundleResponse {
    pub thread_runs: Vec<EngineJobRun>,
    pub scheduler_runs: Vec<EngineJobRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardOverviewResponse {
    pub source_count: i64,
    pub policy_count: i64,
    pub job_stats: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListJobRunsRequest {
    pub job_type: Option<String>,
    pub trigger_type: Option<String>,
    pub thread_id: Option<String>,
    pub status: Option<String>,
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
    pub limit: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::{DashboardOverviewResponse, EngineJobPolicy, EngineModelProfile, EngineSource};

    #[test]
    fn engine_model_profile_deserializes_is_default_flag() {
        let profile: EngineModelProfile = serde_json::from_value(serde_json::json!({
            "id": "model-1",
            "name": "Default Model",
            "provider": "openai",
            "model": "gpt-test",
            "base_url": null,
            "api_key": null,
            "model_capability_revision":"test-published", "context_window_tokens":100000, "output_reserve_tokens":10000, "protocol_overhead_estimator_id":"responses-json-v1",
            "supports_images": true,
            "supports_reasoning": true,
            "supports_responses": true,
            "temperature": 0.2,
            "thinking_level": "medium",
            "is_default": true,
            "enabled": true,
            "created_at": "2026-05-20T00:00:00Z",
            "updated_at": "2026-05-20T00:00:00Z"
        }))
        .expect("profile");

        assert!(profile.is_default);
    }

    #[test]
    fn engine_model_profile_defaults_enabled_to_true() {
        let profile: EngineModelProfile = serde_json::from_value(serde_json::json!({
            "id": "model-1",
            "name": "Default Model",
            "provider": "openai",
            "model": "gpt-test",
            "base_url": null,
            "api_key": null,
            "model_capability_revision":"test-published", "context_window_tokens":100000, "output_reserve_tokens":10000, "protocol_overhead_estimator_id":"responses-json-v1",
            "supports_images": false,
            "supports_reasoning": false,
            "supports_responses": false,
            "temperature": null,
            "thinking_level": null,
            "created_at": "2026-05-20T00:00:00Z",
            "updated_at": "2026-05-20T00:00:00Z"
        }))
        .expect("profile");

        assert!(profile.enabled);
    }

    #[test]
    fn engine_job_policy_uses_explicit_pipeline_limits() {
        let policy: EngineJobPolicy = serde_json::from_value(serde_json::json!({
            "job_type": "thread_summary",
            "model_capability_revision": "published-fixture", "context_window_tokens": 100000,
            "output_reserve_tokens": 10000, "max_model_input_tokens": 90000
        }))
        .expect("policy");

        assert_eq!(policy.max_model_input_tokens, 90_000);
    }

    #[test]
    fn dashboard_overview_response_keeps_job_stats_payload() {
        let overview: DashboardOverviewResponse = serde_json::from_value(serde_json::json!({
            "source_count": 2,
            "policy_count": 4,
            "job_stats": {
                "summary": {
                    "running": 1,
                    "done": 5
                }
            }
        }))
        .expect("overview");

        assert_eq!(overview.source_count, 2);
        assert_eq!(overview.job_stats["summary"]["done"], 5);
    }

    #[test]
    fn engine_source_ignores_internal_secret_hash_field() {
        let source: EngineSource = serde_json::from_value(serde_json::json!({
            "id": "src-1",
            "tenant_id": "tenant-1",
            "source_id": "source-1",
            "source_type": "sdk",
            "name": "Source",
            "description": null,
            "config": null,
            "status": "active",
            "sdk_enabled": true,
            "secret_key_hint": "mk_***1234",
            "key_last_rotated_at": "2026-05-21T00:00:00Z",
            "secret_key_hash": "hashed-secret",
            "created_at": "2026-05-21T00:00:00Z",
            "updated_at": "2026-05-21T00:00:00Z"
        }))
        .expect("source");

        assert_eq!(source.source_id, "source-1");
        assert_eq!(source.secret_key_hint.as_deref(), Some("mk_***1234"));
    }

    #[test]
    fn engine_source_defaults_status_to_active() {
        let source: EngineSource = serde_json::from_value(serde_json::json!({
            "id": "src-1",
            "tenant_id": "tenant-1",
            "source_id": "source-1",
            "source_type": "sdk",
            "name": "Source",
            "description": null,
            "config": null,
            "sdk_enabled": true,
            "secret_key_hint": null,
            "key_last_rotated_at": null,
            "created_at": "2026-05-21T00:00:00Z",
            "updated_at": "2026-05-21T00:00:00Z"
        }))
        .expect("source");

        assert_eq!(source.status, "active");
    }
}
