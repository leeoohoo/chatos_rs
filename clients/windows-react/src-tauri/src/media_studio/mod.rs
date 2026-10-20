mod history;
mod provider;

use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

use crate::{
    authentication::{AuthenticationManager, GatewayModelConfig},
    host_lifecycle::validate_owner,
};
use history::{ImageGenerationOutcome, MediaStudioHistoryStore};
use provider::{
    make_provider_request, parse_provider_response, provider_error_detail, resolve_image_model,
    ImageGenerationRequest,
};

const MAXIMUM_PROVIDER_RESPONSE_BYTES: usize = 48 * 1_024 * 1_024;

pub(crate) struct MediaStudioManager {
    history: MediaStudioHistoryStore,
    provider_client: reqwest::Client,
}

impl MediaStudioManager {
    pub(crate) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        let provider_client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(10 * 60))
            .build()
            .map_err(|_| "无法初始化图片生成网络客户端。".to_owned())?;
        Ok(Self {
            history: MediaStudioHistoryStore::new(app_data_directory)?,
            provider_client,
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaStudioModelOption {
    id: String,
    name: String,
    provider: String,
    model_name: String,
    enabled: bool,
    task_enabled: bool,
    has_api_key: bool,
}

#[tauri::command]
pub(crate) async fn list_media_studio_models(
    authentication: State<'_, AuthenticationManager>,
    owner_user_id: String,
) -> Result<Vec<MediaStudioModelOption>, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let catalog = authentication
        .model_catalog_for_owner(&owner_user_id)
        .await
        .map_err(|error| error.message)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    present_model_catalog(&owner_user_id, catalog)
}

#[tauri::command]
pub(crate) async fn generate_media_studio_image(
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, MediaStudioManager>,
    owner_user_id: String,
    request: ImageGenerationRequest,
) -> Result<ImageGenerationOutcome, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let mut models = authentication
        .resolved_models_for_owner(&owner_user_id, vec![request.model_config_id.clone()])
        .await
        .map_err(|error| error.message)?;
    if models.len() != 1 {
        return Err("所选模型已不可用。".into());
    }
    let runtime = resolve_image_model(&owner_user_id, &request.model_config_id, models.remove(0))?;
    authentication.save_model_credential(
        &owner_user_id,
        &runtime.model_config_id,
        &runtime.api_key,
    )?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let provider_request = make_provider_request(&runtime, &request)?;
    let response = manager
        .provider_client
        .post(provider_request.url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, provider_request.content_type)
        .bearer_auth(&runtime.api_key)
        .body(provider_request.body)
        .send()
        .await
        .map_err(|_| "无法连接图片模型服务。".to_owned())?;
    let status = response.status();
    let body = read_provider_response(response).await?;
    if !status.is_success() {
        return Err(format!(
            "模型请求失败（HTTP {}）：{}",
            status.as_u16(),
            provider_error_detail(&body, &runtime.api_key)
        ));
    }
    let result = parse_provider_response(&request, &runtime, &body)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(persist_generation_result(
        &manager.history,
        &owner_user_id,
        request.prompt.trim(),
        result,
    )
    .await)
}

async fn persist_generation_result(
    history: &MediaStudioHistoryStore,
    owner_user_id: &str,
    prompt: &str,
    result: provider::ImageGenerationResult,
) -> ImageGenerationOutcome {
    match history.persist_image(owner_user_id, prompt, &result).await {
        Ok(item) => ImageGenerationOutcome::Saved { item },
        Err(persistence_error_message) => ImageGenerationOutcome::GeneratedWithoutPersistence {
            result,
            persistence_error_message,
        },
    }
}

async fn read_provider_response(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > MAXIMUM_PROVIDER_RESPONSE_BYTES as u64)
    {
        return Err("模型返回的数据过大。".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "无法读取图片模型响应。".to_owned())?
    {
        if body.len().saturating_add(chunk.len()) > MAXIMUM_PROVIDER_RESPONSE_BYTES {
            return Err("模型返回的数据过大。".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn present_model_catalog(
    owner_user_id: &str,
    catalog: Vec<GatewayModelConfig>,
) -> Result<Vec<MediaStudioModelOption>, String> {
    let mut models = catalog
        .into_iter()
        .filter(|model| {
            model.owner_user_id == owner_user_id
                && model.enabled
                && model.has_api_key
                && !model.id.trim().is_empty()
                && !model.name.trim().is_empty()
                && !model.provider.trim().is_empty()
                && !model.model.trim().is_empty()
        })
        .map(|model| MediaStudioModelOption {
            id: model.id,
            name: model.name,
            provider: model.provider,
            model_name: model.model,
            enabled: model.enabled,
            task_enabled: model.task_enabled.unwrap_or(model.enabled),
            has_api_key: model.has_api_key,
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| {
        image_model_rank(left)
            .cmp(&image_model_rank(right))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(models)
}

fn image_model_rank(model: &MediaStudioModelOption) -> u8 {
    if format!("{} {}", model.name, model.model_name)
        .to_lowercase()
        .contains("image")
    {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use provider::{GeneratedImageAsset, ImageGenerationResult};
    use std::fs;
    use tempfile::tempdir;

    fn model(id: &str, owner: &str, name: &str, enabled: bool) -> GatewayModelConfig {
        GatewayModelConfig {
            id: id.into(),
            owner_user_id: owner.into(),
            name: name.into(),
            provider: "openai".into(),
            model: name.into(),
            api_key: Some("must-never-cross".into()),
            base_url: Some("https://provider.example/v1".into()),
            task_thinking_level: None,
            temperature: None,
            max_output_tokens: None,
            enabled,
            task_enabled: Some(false),
            has_api_key: true,
            supports_images: true,
            supports_reasoning: false,
            supports_responses: true,
        }
    }

    #[test]
    fn catalog_matches_macos_filter_sort_and_redacts_secrets() {
        let models = present_model_catalog(
            "alice",
            vec![
                model("chat", "alice", "Zeta", true),
                model("image", "alice", "Image Alpha", true),
                model("disabled", "alice", "Image Disabled", false),
                model("other", "bob", "Image Other", true),
            ],
        )
        .unwrap();
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["image", "chat"]
        );
        let encoded = serde_json::to_string(&models).unwrap();
        assert!(!encoded.contains("must-never-cross"));
        assert!(!encoded.contains("api_key"));
        assert!(encoded.contains("hasApiKey"));
    }

    #[tokio::test]
    async fn persistence_failure_preserves_generated_result_without_native_paths_or_secrets() {
        let root = tempdir().unwrap();
        let blocked_root = root.path().join("blocked");
        fs::write(&blocked_root, b"not-a-directory").unwrap();
        let history = MediaStudioHistoryStore::for_tests(blocked_root);
        let result = ImageGenerationResult {
            id: "provider-result".into(),
            model_config_id: "image-model".into(),
            model_name: "gpt-image-1".into(),
            created_at_unix_ms: 1_700_000_000_000,
            images: vec![GeneratedImageAsset {
                id: "asset-1".into(),
                mime_type: "image/png".into(),
                base64_data: Some(BASE64.encode(b"generated-image")),
                url: None,
                revised_prompt: None,
            }],
            client_request_id: "request-1".into(),
        };

        let outcome =
            persist_generation_result(&history, "alice", "quiet lake", result.clone()).await;
        let ImageGenerationOutcome::GeneratedWithoutPersistence {
            result: preserved,
            persistence_error_message,
        } = &outcome
        else {
            panic!("generation result must survive a persistence failure");
        };
        assert_eq!(preserved, &result);
        assert!(!persistence_error_message.is_empty());

        let encoded = serde_json::to_string(&outcome).unwrap();
        assert!(encoded.contains("generatedWithoutPersistence"));
        assert!(!encoded.contains("api_key"));
        assert!(!encoded.contains("must-never-cross"));
        assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
    }
}
