// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;

use crate::models::{EngineJobPolicy, ListResponse, UpsertEngineJobPolicyRequest};

use super::super::MemoryEngineClient;

impl MemoryEngineClient {
    pub async fn list_job_policies(&self) -> Result<Vec<EngineJobPolicy>, String> {
        let resp: ListResponse<EngineJobPolicy> = self
            .send_json(Method::GET, "/admin/job-policies", Option::<&()>::None)
            .await?;
        Ok(resp.items)
    }

    pub async fn get_job_policy(&self, job_type: &str) -> Result<EngineJobPolicy, String> {
        self.send_json(
            Method::GET,
            &format!("/admin/job-policies/{}", urlencoding::encode(job_type)),
            Option::<&()>::None,
        )
        .await
    }

    pub async fn upsert_job_policy(
        &self,
        job_type: &str,
        req: &UpsertEngineJobPolicyRequest,
    ) -> Result<EngineJobPolicy, String> {
        self.send_json(
            Method::PUT,
            &format!("/admin/job-policies/{}", urlencoding::encode(job_type)),
            Some(req),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::models::EngineJobPolicy;

    #[test]
    fn list_job_policies_response_deserializes_items() {
        #[derive(serde::Deserialize)]
        struct Response {
            items: Vec<EngineJobPolicy>,
        }

        let resp: Response = serde_json::from_value(serde_json::json!({
            "items": [
                {
                    "job_type": "summary",
                    "enabled": true,
                    "model_profile_id": "model-1",
                    "summary_prompt": null,
                    "rollup_summary_prompt": null,
                    "token_limit": 1200,
                    "target_summary_tokens": 600,
                    "interval_seconds": 60,
                    "max_threads_per_tick": 10,
                    "count_limit": 12,
                    "keep_level0_count": 3,
                    "max_level": 2,
                    "updated_at": "2026-05-20T00:00:00Z"
                }
            ]
        }))
        .expect("response");

        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].job_type, "summary");
    }
}
