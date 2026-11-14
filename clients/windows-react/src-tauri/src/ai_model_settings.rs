use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    authentication::{
        AuthenticationManager, GatewayManagedModelConfig, GatewayModelConfigMutation,
        GatewayModelProvider, GatewayModelProviderMutation, GatewayModelSettings,
        GatewayModelSettingsMutation,
    },
    control_plane::ControlPlaneModelOption,
    tool_approval::settings::ApprovalSettingsStore,
};

const DEFAULT_MODEL_REQUEST_MAX_RETRIES: i64 = 5;
const MAX_MODEL_REQUEST_MAX_RETRIES: i64 = 10;
const MAX_TEXT_LENGTH: usize = 1_024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiModelSettingsSnapshot {
    models: Vec<AiModelConfig>,
    providers: Vec<AiModelProvider>,
    settings: AiModelDefaults,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct AiModelConfig {
    id: String,
    source_provider_id: Option<String>,
    name: String,
    provider: String,
    prompt_vendor: String,
    base_url: String,
    model_name: String,
    task_usage_scenario: Option<String>,
    task_thinking_level: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    enabled: bool,
    task_enabled: bool,
    has_api_key: bool,
    supports_images: bool,
    supports_reasoning: bool,
    supports_responses: bool,
    thinking_levels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AiModelProvider {
    id: String,
    name: String,
    provider: String,
    prompt_vendor: String,
    base_url: String,
    has_api_key: bool,
    enabled: bool,
    supports_images: bool,
    supports_reasoning: bool,
    supports_responses: bool,
    last_sync_status: Option<String>,
    last_sync_error: Option<String>,
    imported_model_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AiModelDefaults {
    model_request_max_retries: i64,
    memory_summary_model_id: Option<String>,
    memory_summary_thinking_level: Option<String>,
    approval_model_id: Option<String>,
    approval_thinking_level: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AiModelProviderInput {
    owner_user_id: String,
    provider_id: Option<String>,
    name: String,
    prompt_vendor: String,
    base_url: String,
    api_key: Option<String>,
    #[serde(default)]
    clear_api_key: bool,
    enabled: bool,
    supports_images: bool,
    supports_reasoning: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AiModelProviderIdentityInput {
    owner_user_id: String,
    provider_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveAiModelConfigurationInput {
    owner_user_id: String,
    model_request_max_retries: i64,
    memory_summary_model_id: Option<String>,
    memory_summary_thinking_level: Option<String>,
    approval_model_id: Option<String>,
    approval_thinking_level: Option<String>,
    model_updates: Vec<AiModelConfigUpdateInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AiModelConfigUpdateInput {
    id: String,
    task_usage_scenario: Option<String>,
    task_thinking_level: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    task_enabled: bool,
}

#[tauri::command]
pub async fn load_ai_model_settings(
    authentication: State<'_, AuthenticationManager>,
    approval_settings: State<'_, ApprovalSettingsStore>,
    owner_user_id: String,
) -> Result<AiModelSettingsSnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", owner_user_id)?;
    load_snapshot(&authentication, &approval_settings, &owner_user_id).await
}

#[tauri::command]
pub async fn create_ai_model_provider(
    authentication: State<'_, AuthenticationManager>,
    approval_settings: State<'_, ApprovalSettingsStore>,
    input: AiModelProviderInput,
) -> Result<AiModelSettingsSnapshot, String> {
    let input = validate_provider_input(input, false)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let request = provider_request(&input, true);
    authentication
        .create_model_provider_for_owner(&input.owner_user_id, &request)
        .await
        .map_err(|error| error.message)?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    load_snapshot(&authentication, &approval_settings, &input.owner_user_id).await
}

#[tauri::command]
pub async fn update_ai_model_provider(
    authentication: State<'_, AuthenticationManager>,
    approval_settings: State<'_, ApprovalSettingsStore>,
    input: AiModelProviderInput,
) -> Result<AiModelSettingsSnapshot, String> {
    let input = validate_provider_input(input, true)?;
    let provider_id = input
        .provider_id
        .as_deref()
        .ok_or_else(|| "provider_id is required".to_owned())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let request = provider_request(&input, false);
    authentication
        .update_model_provider_for_owner(&input.owner_user_id, provider_id, &request)
        .await
        .map_err(|error| error.message)?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    load_snapshot(&authentication, &approval_settings, &input.owner_user_id).await
}

#[tauri::command]
pub async fn refresh_ai_model_provider(
    authentication: State<'_, AuthenticationManager>,
    approval_settings: State<'_, ApprovalSettingsStore>,
    input: AiModelProviderIdentityInput,
) -> Result<AiModelSettingsSnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", input.owner_user_id)?;
    let provider_id = required_identifier("provider_id", input.provider_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .refresh_model_provider_for_owner(&owner_user_id, &provider_id)
        .await
        .map_err(|error| error.message)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    load_snapshot(&authentication, &approval_settings, &owner_user_id).await
}

#[tauri::command]
pub async fn delete_ai_model_provider(
    authentication: State<'_, AuthenticationManager>,
    approval_settings: State<'_, ApprovalSettingsStore>,
    input: AiModelProviderIdentityInput,
) -> Result<AiModelSettingsSnapshot, String> {
    let owner_user_id = required_identifier("owner_user_id", input.owner_user_id)?;
    let provider_id = required_identifier("provider_id", input.provider_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .delete_model_provider_for_owner(&owner_user_id, &provider_id)
        .await
        .map_err(|error| error.message)?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    load_snapshot(&authentication, &approval_settings, &owner_user_id).await
}

#[tauri::command]
pub async fn save_ai_model_configuration(
    authentication: State<'_, AuthenticationManager>,
    approval_settings: State<'_, ApprovalSettingsStore>,
    input: SaveAiModelConfigurationInput,
) -> Result<AiModelSettingsSnapshot, String> {
    let input = validate_configuration_input(input)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let models = authentication
        .managed_model_catalog_for_owner(&input.owner_user_id)
        .await
        .map_err(|error| error.message)?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    validate_configuration_models(&input, &models)?;

    for update in &input.model_updates {
        let request = GatewayModelConfigMutation {
            task_usage_scenario: update.task_usage_scenario.as_deref(),
            task_thinking_level: update.task_thinking_level.as_deref(),
            temperature: update.temperature,
            clear_temperature: update.temperature.is_none(),
            max_output_tokens: update.max_output_tokens,
            clear_max_output_tokens: update.max_output_tokens.is_none(),
            task_enabled: update.task_enabled,
        };
        authentication
            .update_model_config_for_owner(&input.owner_user_id, &update.id, &request)
            .await
            .map_err(|error| error.message)?;
        authentication
            .require_session_generation(&input.owner_user_id, generation)
            .await?;
    }

    let settings_request = GatewayModelSettingsMutation {
        model_request_max_retries: input.model_request_max_retries,
        memory_summary_model_config_id: input.memory_summary_model_id.as_deref(),
        memory_summary_thinking_level: input.memory_summary_thinking_level.as_deref(),
    };
    authentication
        .update_model_settings_for_owner(&input.owner_user_id, &settings_request)
        .await
        .map_err(|error| error.message)?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;

    let options = model_options(&models);
    let store = approval_settings.inner().clone();
    let owner = input.owner_user_id.clone();
    let approval_model_id = input.approval_model_id.clone();
    let approval_thinking_level = input.approval_thinking_level.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store.update_model(&owner, approval_model_id, approval_thinking_level, &options)
    })
    .await
    .map_err(|_| "审批模型设置任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;

    load_snapshot(&authentication, &approval_settings, &input.owner_user_id).await
}

async fn load_snapshot(
    authentication: &AuthenticationManager,
    approval_settings: &ApprovalSettingsStore,
    owner_user_id: &str,
) -> Result<AiModelSettingsSnapshot, String> {
    let generation = authentication
        .session_generation_for_owner(owner_user_id)
        .await?;
    let (models, settings, providers) = tokio::join!(
        authentication.managed_model_catalog_for_owner(owner_user_id),
        authentication.model_settings_for_owner(owner_user_id),
        authentication.model_providers_for_owner(owner_user_id),
    );
    let models = models.map_err(|error| error.message)?;
    let settings = settings.map_err(|error| error.message)?;
    let providers = providers.map_err(|error| error.message)?;
    authentication
        .require_session_generation(owner_user_id, generation)
        .await?;
    ensure_owner_scope(owner_user_id, &models, &providers)?;

    let options = model_options(&models);
    let store = approval_settings.clone();
    let owner = owner_user_id.to_owned();
    let approval = tauri::async_runtime::spawn_blocking(move || store.load(&owner, &options))
        .await
        .map_err(|_| "审批模型设置任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(owner_user_id, generation)
        .await?;

    Ok(AiModelSettingsSnapshot {
        models: models.into_iter().map(project_model).collect(),
        providers: providers.into_iter().map(project_provider).collect(),
        settings: project_settings(
            settings,
            approval.selected_model_id,
            approval.selected_thinking_level,
        ),
    })
}

fn project_model(model: GatewayManagedModelConfig) -> AiModelConfig {
    let prompt_vendor = model
        .prompt_vendor
        .clone()
        .unwrap_or_else(|| model.provider.clone());
    let thinking_levels = thinking_levels(&prompt_vendor, model.supports_reasoning);
    AiModelConfig {
        id: model.id,
        source_provider_id: model.source_provider_id,
        name: model.name,
        provider: model.provider,
        prompt_vendor,
        base_url: model.base_url.unwrap_or_default(),
        model_name: model.model,
        task_usage_scenario: model.task_usage_scenario,
        task_thinking_level: model.task_thinking_level,
        temperature: model.temperature,
        max_output_tokens: model.max_output_tokens,
        enabled: model.enabled,
        task_enabled: model.task_enabled.unwrap_or(model.enabled),
        has_api_key: model.has_api_key,
        supports_images: model.supports_images,
        supports_reasoning: model.supports_reasoning,
        supports_responses: model.supports_responses,
        thinking_levels,
    }
}

fn project_provider(provider: GatewayModelProvider) -> AiModelProvider {
    AiModelProvider {
        id: provider.id,
        name: provider.name,
        prompt_vendor: provider
            .prompt_vendor
            .unwrap_or_else(|| provider.provider.clone()),
        provider: provider.provider,
        base_url: provider.base_url.unwrap_or_default(),
        has_api_key: provider.has_api_key,
        enabled: provider.enabled.unwrap_or(true),
        supports_images: provider.supports_images.unwrap_or(false),
        supports_reasoning: provider.supports_reasoning.unwrap_or(false),
        supports_responses: provider.supports_responses.unwrap_or(false),
        last_sync_status: provider.last_sync_status,
        last_sync_error: provider.last_sync_error,
        imported_model_count: provider.imported_model_count.unwrap_or_default().max(0),
    }
}

fn project_settings(
    settings: GatewayModelSettings,
    approval_model_id: Option<String>,
    approval_thinking_level: Option<String>,
) -> AiModelDefaults {
    AiModelDefaults {
        model_request_max_retries: settings
            .model_request_max_retries
            .unwrap_or(DEFAULT_MODEL_REQUEST_MAX_RETRIES),
        memory_summary_model_id: settings.memory_summary_model_config_id,
        memory_summary_thinking_level: settings.memory_summary_thinking_level,
        approval_model_id,
        approval_thinking_level,
    }
}

fn provider_request<'a>(
    input: &'a AiModelProviderInput,
    creating: bool,
) -> GatewayModelProviderMutation<'a> {
    GatewayModelProviderMutation {
        name: &input.name,
        provider: "gpt",
        prompt_vendor: &input.prompt_vendor,
        api_key: input
            .api_key
            .as_deref()
            .filter(|value| creating || !value.is_empty()),
        clear_api_key: input.clear_api_key,
        base_url: &input.base_url,
        enabled: input.enabled,
        supports_images: input.supports_images,
        supports_reasoning: input.supports_reasoning,
        supports_responses: true,
    }
}

fn validate_provider_input(
    mut input: AiModelProviderInput,
    updating: bool,
) -> Result<AiModelProviderInput, String> {
    input.owner_user_id = required_identifier("owner_user_id", input.owner_user_id)?;
    input.provider_id = match input.provider_id {
        Some(value) => Some(required_identifier("provider_id", value)?),
        None => None,
    };
    if updating != input.provider_id.is_some() {
        return Err(if updating {
            "provider_id is required".into()
        } else {
            "新增供应商时不能指定 provider_id。".into()
        });
    }
    input.name = required_text("name", input.name)?;
    input.prompt_vendor = input.prompt_vendor.trim().to_ascii_lowercase();
    if !matches!(
        input.prompt_vendor.as_str(),
        "gpt" | "deepseek" | "kimi" | "glm"
    ) {
        return Err("Prompt 模板必须是 gpt、deepseek、kimi 或 glm。".into());
    }
    input.base_url = required_text("base_url", input.base_url)?;
    let url = reqwest::Url::parse(&input.base_url).map_err(|_| "Base URL 无效。".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Base URL 必须是有效的 HTTP 或 HTTPS 地址。".into());
    }
    input.api_key = input.api_key.and_then(trimmed_non_empty);
    if !updating && input.api_key.is_none() {
        return Err("新增供应商时必须填写 API Key。".into());
    }
    if input.clear_api_key && input.api_key.is_some() {
        return Err("不能同时填写并清除 API Key。".into());
    }
    Ok(input)
}

fn validate_configuration_input(
    mut input: SaveAiModelConfigurationInput,
) -> Result<SaveAiModelConfigurationInput, String> {
    input.owner_user_id = required_identifier("owner_user_id", input.owner_user_id)?;
    if !(0..=MAX_MODEL_REQUEST_MAX_RETRIES).contains(&input.model_request_max_retries) {
        return Err("模型请求最大重试次数必须在 0 到 10 之间。".into());
    }
    input.memory_summary_model_id = optional_identifier(input.memory_summary_model_id)?;
    input.approval_model_id = optional_identifier(input.approval_model_id)?;
    input.memory_summary_thinking_level = optional_identifier(input.memory_summary_thinking_level)?;
    input.approval_thinking_level = optional_identifier(input.approval_thinking_level)?;
    let mut seen = std::collections::HashSet::new();
    for update in &mut input.model_updates {
        update.id = required_identifier("model_id", std::mem::take(&mut update.id))?;
        if !seen.insert(update.id.clone()) {
            return Err("模型更新列表包含重复项目。".into());
        }
        update.task_usage_scenario = optional_text(update.task_usage_scenario.take())?;
        update.task_thinking_level = optional_identifier(update.task_thinking_level.take())?;
        if update
            .temperature
            .is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
        {
            return Err("Temperature 必须留空或填写 0 到 2。".into());
        }
        if update.max_output_tokens.is_some_and(|value| value <= 0) {
            return Err("Max Tokens 必须留空或填写大于 0 的整数。".into());
        }
    }
    Ok(input)
}

fn validate_configuration_models(
    input: &SaveAiModelConfigurationInput,
    models: &[GatewayManagedModelConfig],
) -> Result<(), String> {
    for update in &input.model_updates {
        let model = models
            .iter()
            .find(|model| model.id == update.id)
            .ok_or_else(|| "要更新的模型已不存在，请刷新后重试。".to_owned())?;
        validate_thinking_level(
            update.task_thinking_level.as_deref(),
            model.prompt_vendor.as_deref().unwrap_or(&model.provider),
            model.supports_reasoning,
        )?;
    }
    validate_default_model(
        "Memory 总结",
        input.memory_summary_model_id.as_deref(),
        input.memory_summary_thinking_level.as_deref(),
        models,
    )?;
    validate_default_model(
        "本机审批 Agent",
        input.approval_model_id.as_deref(),
        input.approval_thinking_level.as_deref(),
        models,
    )
}

fn validate_default_model(
    label: &str,
    model_id: Option<&str>,
    thinking_level: Option<&str>,
    models: &[GatewayManagedModelConfig],
) -> Result<(), String> {
    let Some(model_id) = model_id else {
        if thinking_level.is_some() {
            return Err(format!("未选择{label}模型时不能设置推理等级。"));
        }
        return Ok(());
    };
    let model = models
        .iter()
        .find(|model| model.id == model_id && model.enabled && model.has_api_key)
        .ok_or_else(|| format!("{label}必须使用已启用且配置了密钥的模型。"))?;
    validate_thinking_level(
        thinking_level,
        model.prompt_vendor.as_deref().unwrap_or(&model.provider),
        model.supports_reasoning,
    )
}

fn validate_thinking_level(
    value: Option<&str>,
    provider: &str,
    supports_reasoning: bool,
) -> Result<(), String> {
    let Some(value) = value else { return Ok(()) };
    if !supports_reasoning
        || !thinking_levels(provider, true)
            .iter()
            .any(|item| item == value)
    {
        return Err("选择的模型不支持该推理等级。".into());
    }
    Ok(())
}

fn model_options(models: &[GatewayManagedModelConfig]) -> Vec<ControlPlaneModelOption> {
    models
        .iter()
        .filter(|model| model.enabled && model.has_api_key)
        .map(|model| {
            let provider = model.prompt_vendor.as_deref().unwrap_or(&model.provider);
            let levels = thinking_levels(provider, model.supports_reasoning);
            let default = model
                .task_thinking_level
                .as_ref()
                .filter(|value| levels.contains(value))
                .cloned()
                .or_else(|| {
                    levels
                        .iter()
                        .find(|value| value.as_str() == "medium")
                        .cloned()
                })
                .or_else(|| levels.first().cloned())
                .unwrap_or_else(|| "none".into());
            ControlPlaneModelOption {
                id: model.id.clone(),
                display_name: model.name.clone(),
                model_name: model.model.clone(),
                provider: provider.to_owned(),
                default_thinking_level: default,
                supports_reasoning: model.supports_reasoning,
                thinking_levels: levels,
                model_config_revision: String::new(),
            }
        })
        .collect()
}

fn thinking_levels(provider: &str, supports_reasoning: bool) -> Vec<String> {
    if !supports_reasoning {
        return Vec::new();
    }
    let normalized = provider.trim().to_ascii_lowercase().replace('-', "_");
    let values: &[&str] = match normalized.as_str() {
        "deepseek" => &["none", "high", "max"],
        "kimi" | "kimik2" | "moonshot" => &["auto", "none"],
        "glm" | "zhipu" | "zai" => &["none", "low", "medium", "high", "xhigh"],
        _ => &["none", "minimal", "low", "medium", "high", "xhigh"],
    };
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn ensure_owner_scope(
    owner_user_id: &str,
    models: &[GatewayManagedModelConfig],
    providers: &[GatewayModelProvider],
) -> Result<(), String> {
    if models
        .iter()
        .any(|model| model.owner_user_id != owner_user_id)
        || providers
            .iter()
            .any(|provider| provider.owner_user_id != owner_user_id)
    {
        return Err("模型目录包含其他账号的数据。".into());
    }
    Ok(())
}

fn required_identifier(field: &str, value: String) -> Result<String, String> {
    let value = required_text(field, value)?;
    if value.chars().any(char::is_whitespace) {
        return Err(format!("{field} is invalid"));
    }
    Ok(value)
}

fn optional_identifier(value: Option<String>) -> Result<Option<String>, String> {
    value
        .map(|value| required_identifier("identifier", value))
        .transpose()
}

fn required_text(field: &str, value: String) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_TEXT_LENGTH || value.chars().any(char::is_control) {
        return Err(format!("{field} is invalid"));
    }
    Ok(value.to_owned())
}

fn optional_text(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    required_text("text", value.to_owned()).map(Some)
}

fn trimmed_non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str, enabled: bool, has_api_key: bool) -> GatewayManagedModelConfig {
        GatewayManagedModelConfig {
            id: id.into(),
            owner_user_id: "owner-1".into(),
            source_provider_id: Some("provider-1".into()),
            name: "GPT".into(),
            provider: "gpt".into(),
            prompt_vendor: Some("gpt".into()),
            model: "gpt-5".into(),
            base_url: Some("https://example.com/v1".into()),
            task_usage_scenario: None,
            task_thinking_level: Some("medium".into()),
            temperature: None,
            max_output_tokens: None,
            enabled,
            task_enabled: Some(true),
            has_api_key,
            supports_images: true,
            supports_reasoning: true,
            supports_responses: true,
        }
    }

    #[test]
    fn provider_validation_requires_secret_only_for_creation() {
        let mut input = AiModelProviderInput {
            owner_user_id: "owner-1".into(),
            provider_id: None,
            name: "OpenAI".into(),
            prompt_vendor: "gpt".into(),
            base_url: "https://api.example.com/v1".into(),
            api_key: None,
            clear_api_key: false,
            enabled: true,
            supports_images: true,
            supports_reasoning: true,
        };
        assert!(validate_provider_input(input, false).is_err());

        input = AiModelProviderInput {
            owner_user_id: "owner-1".into(),
            provider_id: Some("provider-1".into()),
            name: "OpenAI".into(),
            prompt_vendor: "gpt".into(),
            base_url: "https://api.example.com/v1".into(),
            api_key: None,
            clear_api_key: false,
            enabled: true,
            supports_images: true,
            supports_reasoning: true,
        };
        assert!(validate_provider_input(input, true).is_ok());
    }

    #[test]
    fn defaults_only_accept_enabled_models_with_credentials() {
        let models = vec![model("model-1", true, true)];
        assert!(validate_default_model("Memory", Some("model-1"), Some("high"), &models).is_ok());
        assert!(validate_default_model("Memory", Some("missing"), None, &models).is_err());
        assert!(validate_default_model("Memory", None, Some("high"), &models).is_err());
        assert!(validate_default_model("Memory", Some("model-1"), Some("max"), &models).is_err());
    }

    #[test]
    fn safe_projection_never_contains_api_key_material() {
        let encoded = serde_json::to_string(&project_model(model("model-1", true, true))).unwrap();
        assert!(!encoded.contains("apiKey"));
        assert!(encoded.contains("hasApiKey"));
    }

    #[test]
    fn model_input_bounds_match_macos_contract() {
        let invalid_temperature = SaveAiModelConfigurationInput {
            owner_user_id: "owner-1".into(),
            model_request_max_retries: 5,
            memory_summary_model_id: None,
            memory_summary_thinking_level: None,
            approval_model_id: None,
            approval_thinking_level: None,
            model_updates: vec![AiModelConfigUpdateInput {
                id: "model-1".into(),
                task_usage_scenario: None,
                task_thinking_level: None,
                temperature: Some(2.1),
                max_output_tokens: None,
                task_enabled: true,
            }],
        };
        assert!(validate_configuration_input(invalid_temperature).is_err());
    }
}
