// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub fn normalize_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai" | "gpt" => "gpt".to_string(),
        "kimik2" | "kimi" | "moonshot" => "kimi".to_string(),
        "openai-compatible" | "openai_compatible" | "compatible" => "openai_compatible".to_string(),
        other => other.to_string(),
    }
}

pub fn is_gpt_provider(provider: &str) -> bool {
    normalize_provider(provider) == "gpt"
}

pub fn default_base_url_for_provider(provider: &str, fallback_base_url: &str) -> String {
    match normalize_provider(provider).as_str() {
        "deepseek" => "https://api.deepseek.com".to_string(),
        "kimi" => "https://api.moonshot.ai/v1".to_string(),
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
            "none" => None,
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
        default_base_url_for_provider, normalize_provider, normalize_thinking_level,
        reasoning_effort_for_provider, thinking_mode_for_provider,
    };

    #[test]
    fn normalizes_provider_aliases() {
        assert_eq!(normalize_provider("openai"), "gpt");
        assert_eq!(normalize_provider("kimik2"), "kimi");
        assert_eq!(normalize_provider("moonshot"), "kimi");
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
}
