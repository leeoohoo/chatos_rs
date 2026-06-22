use reqwest::Method;

use crate::models::{EngineModelProfile, ListResponse, UpsertEngineModelProfileRequest};

use super::super::MemoryEngineClient;

impl MemoryEngineClient {
    pub async fn list_model_profiles(&self) -> Result<Vec<EngineModelProfile>, String> {
        let resp: ListResponse<EngineModelProfile> = self
            .send_json(Method::GET, "/admin/model-profiles", Option::<&()>::None)
            .await?;
        Ok(resp.items)
    }

    pub async fn get_model_profile_by_id(
        &self,
        model_id: &str,
    ) -> Result<Option<EngineModelProfile>, String> {
        match self
            .send_json::<EngineModelProfile, _>(
                Method::GET,
                &format!("/admin/model-profiles/{}", urlencoding::encode(model_id)),
                Option::<&()>::None,
            )
            .await
        {
            Ok(item) => Ok(Some(item)),
            Err(err) if err.starts_with("status=404") => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub async fn create_model_profile(
        &self,
        req: &UpsertEngineModelProfileRequest,
    ) -> Result<EngineModelProfile, String> {
        self.send_json(Method::POST, "/admin/model-profiles", Some(req))
            .await
    }

    pub async fn update_model_profile(
        &self,
        model_id: &str,
        req: &UpsertEngineModelProfileRequest,
    ) -> Result<EngineModelProfile, String> {
        self.send_json(
            Method::PUT,
            &format!("/admin/model-profiles/{}", urlencoding::encode(model_id)),
            Some(req),
        )
        .await
    }

    pub async fn delete_model_profile(&self, model_id: &str) -> Result<(), String> {
        let _: serde_json::Value = self
            .send_json(
                Method::DELETE,
                &format!("/admin/model-profiles/{}", urlencoding::encode(model_id)),
                Option::<&()>::None,
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::models::EngineModelProfile;

    #[test]
    fn model_profile_response_deserializes_is_default_flag() {
        let profile: EngineModelProfile = serde_json::from_value(serde_json::json!({
            "id": "model-1",
            "name": "Main",
            "provider": "openai",
            "model": "gpt-test",
            "base_url": null,
            "api_key": null,
            "supports_images": false,
            "supports_reasoning": true,
            "supports_responses": true,
            "temperature": null,
            "thinking_level": null,
            "is_default": true,
            "enabled": true,
            "created_at": "2026-05-20T00:00:00Z",
            "updated_at": "2026-05-20T00:00:00Z"
        }))
        .expect("profile");

        assert!(profile.is_default);
    }
}
