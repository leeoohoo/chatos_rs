// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) fn normalize_provider(value: Option<String>) -> String {
    match value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("gpt")
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "openai" | "gpt" => "gpt".to_string(),
        "deepseek" => "deepseek".to_string(),
        "kimi" | "kimik2" | "moonshot" => "kimi".to_string(),
        "glm" | "zhipu" | "zhipuai" | "zai" | "chatglm" => "glm".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn default_base_url_for_provider(provider: &str) -> String {
    match normalize_provider(Some(provider.to_string())).as_str() {
        "deepseek" => "https://api.deepseek.com/v1".to_string(),
        "kimi" => "https://api.moonshot.cn/v1".to_string(),
        "glm" => "https://open.bigmodel.cn/api/paas/v4".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

pub(super) fn runtime_provider_for_model(provider: &str, base_url: &str) -> String {
    let provider = normalize_provider(Some(provider.to_string()));
    if provider == "gpt"
        && !base_url
            .trim()
            .to_ascii_lowercase()
            .contains("api.openai.com")
    {
        "openai_compatible".to_string()
    } else {
        provider
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glm_aliases_keep_the_glm_parameter_dialect() {
        assert_eq!(normalize_provider(Some("zhipu".to_string())), "glm");
        assert_eq!(
            default_base_url_for_provider("glm"),
            "https://open.bigmodel.cn/api/paas/v4"
        );
        assert_eq!(
            runtime_provider_for_model("glm", "https://open.bigmodel.cn/api/paas/v4"),
            "glm"
        );
    }
}
