// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;

use crate::models::{EngineJobPolicy, ListResponse};

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
                    "model_capability_revision": "published-fixture", "context_window_tokens": 100000,
                    "output_reserve_tokens": 10000, "max_model_input_tokens": 90000
                }
            ]
        }))
        .expect("response");

        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].job_type, "summary");
    }
}
