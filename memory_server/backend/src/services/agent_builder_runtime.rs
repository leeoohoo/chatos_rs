use axum::http::StatusCode;
use serde_json::Value;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::AiModelConfig;
use crate::repositories::configs;

use super::{
    bad_request_error, internal_error,
    support::{normalize_base_url, normalize_model_name, normalize_provider},
    ModelRuntime, NormalizedRequest,
};

pub(super) async fn resolve_model_runtime(
    db: &Db,
    config: &AppConfig,
    request: &NormalizedRequest,
) -> Result<ModelRuntime, (StatusCode, String)> {
    if let Some(model_config_id) = request.model_config_id.as_deref() {
        let item = configs::get_model_config_by_id(db, model_config_id)
            .await
            .map_err(|err| internal_error(format!("load selected model config failed: {err}")))?;
        let Some(item) = item else {
            return Err(bad_request_error("所选创建模型不存在"));
        };
        if item.user_id != request.scope_user_id {
            return Err(bad_request_error("所选创建模型不属于当前作用域账号"));
        }
        if item.enabled != 1 {
            return Err(bad_request_error("所选创建模型未启用"));
        }
        return model_runtime_from_model_config(config, &item);
    }

    if let Some(value) = request.ai_model_config.as_ref() {
        return model_runtime_from_value(config, value);
    }

    let enabled_items = configs::list_model_configs(db, request.scope_user_id.as_str())
        .await
        .map_err(|err| internal_error(format!("load model configs failed: {err}")))?
        .into_iter()
        .filter(|item| item.enabled == 1)
        .collect::<Vec<_>>();
    if enabled_items.len() == 1 {
        return model_runtime_from_model_config(config, &enabled_items[0]);
    }
    if enabled_items.len() > 1 {
        return Err(bad_request_error(
            "已配置多个启用模型，请传 model_config_id 指定用于创建智能体的模型",
        ));
    }

    let api_key = config
        .openai_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            bad_request_error(
                "请先在 Memory 的模型配置中启用一个可用模型，或配置 MEMORY_SERVER_OPENAI_API_KEY",
            )
        })?;

    Ok(ModelRuntime {
        provider: "gpt".to_string(),
        model: normalize_model_name(config.openai_model.as_str()),
        base_url: normalize_base_url(config.openai_base_url.as_str()),
        api_key: api_key.to_string(),
        temperature: config.openai_temperature.clamp(0.0, 2.0),
        request_timeout_secs: config.ai_request_timeout_secs,
        supports_responses: false,
    })
}

fn model_runtime_from_model_config(
    config: &AppConfig,
    item: &AiModelConfig,
) -> Result<ModelRuntime, (StatusCode, String)> {
    let api_key = item
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(config
            .openai_api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()))
        .ok_or_else(|| bad_request_error(format!("模型 {} 未配置 api_key", item.name)))?;

    Ok(ModelRuntime {
        provider: normalize_provider(item.provider.as_str()),
        model: normalize_model_name(item.model.as_str()),
        base_url: item
            .base_url
            .as_deref()
            .map(normalize_base_url)
            .unwrap_or_else(|| normalize_base_url(config.openai_base_url.as_str())),
        api_key: api_key.to_string(),
        temperature: item
            .temperature
            .unwrap_or(config.openai_temperature)
            .clamp(0.0, 2.0),
        request_timeout_secs: config.ai_request_timeout_secs,
        supports_responses: item.supports_responses == 1,
    })
}

fn model_runtime_from_value(
    config: &AppConfig,
    value: &Value,
) -> Result<ModelRuntime, (StatusCode, String)> {
    let provider = value
        .get("provider")
        .and_then(Value::as_str)
        .map(normalize_provider)
        .unwrap_or_else(|| "gpt".to_string());
    let model = value
        .get("model")
        .or_else(|| value.get("model_name"))
        .and_then(Value::as_str)
        .map(normalize_model_name)
        .unwrap_or_else(|| normalize_model_name(config.openai_model.as_str()));
    let base_url = value
        .get("base_url")
        .and_then(Value::as_str)
        .map(normalize_base_url)
        .unwrap_or_else(|| normalize_base_url(config.openai_base_url.as_str()));
    let api_key = value
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .or(config
            .openai_api_key
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty()))
        .ok_or_else(|| bad_request_error("显式模型配置缺少 api_key"))?;
    let temperature = value
        .get("temperature")
        .and_then(Value::as_f64)
        .unwrap_or(config.openai_temperature)
        .clamp(0.0, 2.0);
    let supports_responses = value
        .get("supports_responses")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Ok(ModelRuntime {
        provider,
        model,
        base_url,
        api_key: api_key.to_string(),
        temperature,
        request_timeout_secs: config.ai_request_timeout_secs,
        supports_responses,
    })
}
