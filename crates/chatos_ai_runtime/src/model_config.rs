// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub fn normalize_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai" | "gpt" => "gpt".to_string(),
        "kimik2" | "kimi" | "moonshot" => "kimi".to_string(),
        "glm" | "zhipu" | "zhipuai" | "zai" | "chatglm" => "glm".to_string(),
        "openai-compatible" | "openai_compatible" | "compatible" => "openai_compatible".to_string(),
        other => other.to_string(),
    }
}

pub fn is_gpt_provider(provider: &str) -> bool {
    normalize_provider(provider) == "gpt"
}

pub fn effective_responses_support(_provider: &str, base_url: &str, configured: bool) -> bool {
    if !configured {
        return false;
    }
    let base_url = base_url.trim().to_ascii_lowercase();
    if is_official_kimi_base_url(base_url.as_str()) || is_official_glm_base_url(base_url.as_str()) {
        return false;
    }
    true
}

pub fn supports_responses_input_token_count(_provider: &str, base_url: &str) -> bool {
    let base_url = base_url.trim().to_ascii_lowercase();
    if is_official_deepseek_base_url(base_url.as_str())
        || is_official_kimi_base_url(base_url.as_str())
        || is_official_glm_base_url(base_url.as_str())
    {
        return false;
    }
    // OpenAI's /responses/input_tokens route is not shared by the public
    // DeepSeek, Moonshot, or BigModel APIs. Custom gateways may implement it,
    // so only the known direct vendor hosts are excluded here.
    true
}

pub fn supports_previous_response_id(_provider: &str, base_url: &str) -> bool {
    let base_url = base_url.trim().to_ascii_lowercase();
    // DeepSeek's Responses API is stateless and silently ignores
    // previous_response_id. Sending only a delta input would lose context.
    !is_official_deepseek_base_url(base_url.as_str())
}

fn is_official_deepseek_base_url(base_url: &str) -> bool {
    base_url.contains("api.deepseek.com")
}

fn is_official_kimi_base_url(base_url: &str) -> bool {
    base_url.contains("api.moonshot.cn")
        || base_url.contains("api.moonshot.ai")
        || base_url.contains("api.kimi.com")
}

fn is_official_glm_base_url(base_url: &str) -> bool {
    base_url.contains("open.bigmodel.cn")
}

pub fn default_base_url_for_provider(provider: &str, fallback_base_url: &str) -> String {
    match normalize_provider(provider).as_str() {
        "deepseek" => "https://api.deepseek.com".to_string(),
        "kimi" => "https://api.moonshot.ai/v1".to_string(),
        "glm" => "https://open.bigmodel.cn/api/paas/v4".to_string(),
        _ => {
            let fallback = fallback_base_url.trim();
            if fallback.is_empty() {
                "https://api.openai.com/v1".to_string()
            } else {
                fallback.to_string()
            }
        }
    }
}

pub fn normalize_thinking_level(
    provider: &str,
    level: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(level) = level.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let provider = normalize_provider(provider);
    let normalized = match level.to_ascii_lowercase().as_str() {
        "off" | "disabled" | "none" => "none",
        "auto" => "auto",
        "minimal" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" | "max" => {
            if provider == "deepseek" {
                "max"
            } else {
                "xhigh"
            }
        }
        _ => return Err("invalid thinking_level".to_string()),
    };

    let allowed = match provider.as_str() {
        "gpt" => ["none", "minimal", "low", "medium", "high", "xhigh"].as_slice(),
        "deepseek" => ["none", "low", "medium", "high", "max"].as_slice(),
        "kimi" => ["none", "auto", "low", "medium", "high", "xhigh"].as_slice(),
        _ => ["none", "low", "medium", "high", "xhigh"].as_slice(),
    };
    if provider == "openai_compatible" && normalized == "minimal" {
        return Ok(Some("low".to_string()));
    }
    if !allowed.contains(&normalized) {
        return Err("invalid thinking_level".to_string());
    }
    Ok(Some(normalized.to_string()))
}

pub fn reasoning_effort_for_provider(
    provider: Option<&str>,
    level: Option<&str>,
) -> Option<String> {
    let provider = normalize_provider(provider.unwrap_or("gpt"));
    let normalized = normalize_thinking_level(provider.as_str(), level)
        .ok()
        .flatten()?;

    match provider.as_str() {
        "deepseek" => match normalized.as_str() {
            "none" => Some("none".to_string()),
            "max" | "xhigh" => Some("max".to_string()),
            "low" | "medium" | "high" | "auto" | "minimal" => Some("high".to_string()),
            _ => None,
        },
        "kimi" => None,
        _ => Some(normalized),
    }
}

pub fn thinking_mode_for_provider(
    provider: Option<&str>,
    level: Option<&str>,
) -> Option<&'static str> {
    let provider = normalize_provider(provider.unwrap_or("gpt"));
    let normalized = normalize_thinking_level(provider.as_str(), level)
        .ok()
        .flatten()?;
    match provider.as_str() {
        "deepseek" => {
            if normalized == "none" {
                Some("disabled")
            } else {
                Some("enabled")
            }
        }
        "kimi" => {
            if normalized == "none" {
                Some("disabled")
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_base_url_for_provider, effective_responses_support, normalize_provider,
        normalize_thinking_level, reasoning_effort_for_provider, supports_previous_response_id,
        supports_responses_input_token_count, thinking_mode_for_provider,
    };

    #[test]
    fn normalizes_provider_aliases() {
        assert_eq!(normalize_provider("openai"), "gpt");
        assert_eq!(normalize_provider("kimik2"), "kimi");
        assert_eq!(normalize_provider("moonshot"), "kimi");
        assert_eq!(normalize_provider("zhipu"), "glm");
        assert_eq!(normalize_provider("openai-compatible"), "openai_compatible");
    }

    #[test]
    fn maps_provider_default_base_urls() {
        assert_eq!(
            default_base_url_for_provider("deepseek", "https://api.openai.com/v1"),
            "https://api.deepseek.com"
        );
        assert_eq!(
            default_base_url_for_provider("kimi", "https://api.openai.com/v1"),
            "https://api.moonshot.ai/v1"
        );
        assert_eq!(
            default_base_url_for_provider("glm", "https://api.openai.com/v1"),
            "https://open.bigmodel.cn/api/paas/v4"
        );
        assert_eq!(
            default_base_url_for_provider("gpt", "https://gateway.local/v1"),
            "https://gateway.local/v1"
        );
    }

    #[test]
    fn maps_deepseek_thinking_controls() {
        assert_eq!(
            normalize_thinking_level("deepseek", Some("xhigh")).unwrap(),
            Some("max".to_string())
        );
        assert_eq!(
            reasoning_effort_for_provider(Some("deepseek"), Some("medium")).as_deref(),
            Some("high")
        );
        assert_eq!(
            thinking_mode_for_provider(Some("deepseek"), Some("none")),
            Some("disabled")
        );
        assert_eq!(
            reasoning_effort_for_provider(Some("deepseek"), Some("none")).as_deref(),
            Some("none")
        );
        assert_eq!(
            thinking_mode_for_provider(Some("deepseek"), Some("max")),
            Some("enabled")
        );
    }

    #[test]
    fn maps_kimi_thinking_controls() {
        assert_eq!(
            normalize_thinking_level("kimik2", Some("auto")).unwrap(),
            Some("auto".to_string())
        );
        assert_eq!(
            reasoning_effort_for_provider(Some("kimi"), Some("auto")),
            None
        );
        assert_eq!(
            thinking_mode_for_provider(Some("kimi"), Some("none")),
            Some("disabled")
        );
    }

    #[test]
    fn maps_openai_compatible_minimal_to_low() {
        assert_eq!(
            normalize_thinking_level("openai_compatible", Some("minimal")).unwrap(),
            Some("low".to_string())
        );
        assert_eq!(
            reasoning_effort_for_provider(Some("openai_compatible"), Some("minimal")).as_deref(),
            Some("low")
        );
    }

    #[test]
    fn direct_vendor_endpoints_use_only_supported_transports_and_count_routes() {
        assert!(effective_responses_support(
            "deepseek",
            "https://api.deepseek.com",
            true
        ));
        assert!(!supports_responses_input_token_count(
            "deepseek",
            "https://api.deepseek.com"
        ));
        assert!(!supports_previous_response_id(
            "deepseek",
            "https://api.deepseek.com"
        ));
        assert!(!supports_previous_response_id(
            "openai_compatible",
            "https://api.deepseek.com/v1"
        ));

        for (provider, base_url) in [
            ("kimi", "https://api.moonshot.ai/v1"),
            ("kimi", "https://api.moonshot.cn/v1"),
            ("glm", "https://open.bigmodel.cn/api/paas/v4"),
        ] {
            assert!(!effective_responses_support(provider, base_url, true));
            assert!(!supports_responses_input_token_count(provider, base_url));
        }
        assert!(!effective_responses_support(
            "openai_compatible",
            "https://api.moonshot.ai/v1",
            true
        ));

        assert!(effective_responses_support(
            "kimi",
            "https://gateway.example.test/v1",
            true
        ));
        assert!(supports_responses_input_token_count(
            "glm",
            "https://gateway.example.test/v1"
        ));
    }
}
