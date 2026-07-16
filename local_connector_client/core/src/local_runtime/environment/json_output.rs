// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::de::DeserializeOwned;

pub(super) fn parse_model_json<T: DeserializeOwned>(raw: &str) -> Result<T, String> {
    let trimmed = raw.trim();
    for candidate in [
        Some(trimmed),
        strip_fence(trimmed).as_deref(),
        extract_object(trimmed).as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Ok(value) = serde_json::from_str(candidate) {
            return Ok(value);
        }
    }
    Err("Project Environment Agent did not return valid JSON".to_string())
}

fn strip_fence(raw: &str) -> Option<String> {
    let body = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))?;
    Some(body.strip_suffix("```").unwrap_or(body).trim().to_string())
}

fn extract_object(raw: &str) -> Option<String> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    (end > start).then(|| raw[start..=end].to_string())
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::parse_model_json;

    #[derive(Deserialize)]
    struct Output {
        status: String,
    }

    #[test]
    fn parses_fenced_json() {
        let output: Output =
            parse_model_json("```json\n{\"status\":\"ready\"}\n```").expect("parse output");
        assert_eq!(output.status, "ready");
    }
}
