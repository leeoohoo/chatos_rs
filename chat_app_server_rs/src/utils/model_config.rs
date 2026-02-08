pub fn normalize_provider(provider: &str) -> String {
    let p = provider.trim().to_lowercase();
    if p == "openai" { "gpt".to_string() } else { p }
}

pub fn is_gpt_provider(provider: &str) -> bool {
    normalize_provider(provider) == "gpt"
}

pub fn normalize_thinking_level(provider: &str, level: Option<&str>) -> Result<Option<String>, String> {
    let level = level.map(|v| v.trim()).filter(|v| !v.is_empty());
    if level.is_none() {
        return Ok(None);
    }
    if !is_gpt_provider(provider) {
        return Err("thinking_level only supported for gpt provider".to_string());
    }
    let lvl = level.unwrap().to_lowercase();
    let allowed = ["none", "minimal", "low", "medium", "high", "xhigh"];
    if !allowed.contains(&lvl.as_str()) {
        return Err("invalid thinking_level".to_string());
    }
    Ok(Some(lvl))
}
