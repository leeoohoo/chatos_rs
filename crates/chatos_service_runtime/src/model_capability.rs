// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MODEL_CAPABILITIES_CONFIG_KEY: &str = "shared.ai.model_capabilities";
pub const ACTIVE_MODEL_CAPABILITY_CONFIG_KEY: &str = "shared.ai.active_model_capability_revision";

/// An explicitly published request budget. There is no product or model-name fallback.
/// Sources describe the vendor specifications used to select the common budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedModelCapability {
    pub context_window_tokens: usize,
    pub output_reserve_tokens: usize,
    pub protocol_overhead_estimator_id: String,
    pub tokenizer_id: Option<String>,
    pub source_urls: Vec<String>,
    pub excluded_models: Vec<String>,
}

impl PublishedModelCapability {
    pub fn input_limit(&self) -> Result<usize, String> {
        if self.output_reserve_tokens == 0
            || self.protocol_overhead_estimator_id.trim().is_empty()
            || self.source_urls.is_empty()
            || self
                .source_urls
                .iter()
                .any(|source| !source.starts_with("https://"))
        {
            return Err("published model budget requires positive output reserve, estimator and specification sources".into());
        }
        self.context_window_tokens
            .checked_sub(self.output_reserve_tokens)
            .filter(|input| *input > 0)
            .ok_or_else(|| "published model budget has no input capacity".into())
    }
}

pub fn parse_catalog(raw: &str) -> Result<HashMap<String, PublishedModelCapability>, String> {
    let catalog: HashMap<String, PublishedModelCapability> =
        serde_json::from_str(raw).map_err(|error| {
            format!("invalid Configuration Center model capability catalog: {error}")
        })?;
    for (revision, capability) in &catalog {
        if revision.trim().is_empty() {
            return Err("model capability revision must not be empty".into());
        }
        capability.input_limit()?;
    }
    Ok(catalog)
}

pub fn catalog_from_env() -> Result<HashMap<String, PublishedModelCapability>, String> {
    let raw = std::env::var("CHATOS_MODEL_CAPABILITIES_JSON").map_err(|_| {
        format!("{MODEL_CAPABILITIES_CONFIG_KEY} is required from Configuration Center")
    })?;
    parse_catalog(&raw)
}

pub fn active_from_env() -> Result<Option<(String, PublishedModelCapability)>, String> {
    let Ok(revision) = std::env::var("CHATOS_ACTIVE_MODEL_CAPABILITY_REVISION") else {
        return Ok(None);
    };
    if revision.trim().is_empty() {
        return Ok(None);
    }
    let mut catalog = catalog_from_env()?;
    let capability = catalog.remove(&revision).ok_or_else(|| {
        format!(
            "active model capability revision {revision} is not published in Configuration Center"
        )
    })?;
    Ok(Some((revision, capability)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_requires_explicit_budgets_and_sources() {
        assert!(parse_catalog("{}").unwrap().is_empty());
        assert!(parse_catalog(r#"{"missing":{}}"#).is_err());
        let mut cap = PublishedModelCapability {
            context_window_tokens: 9137,
            output_reserve_tokens: 1137,
            protocol_overhead_estimator_id: "responses-json-v1".into(),
            tokenizer_id: None,
            source_urls: vec!["https://specification.example/model".into()],
            excluded_models: Vec::new(),
        };
        assert_eq!(cap.input_limit().unwrap(), 8000);
        cap.context_window_tokens = 17137;
        assert_eq!(cap.input_limit().unwrap(), 16000);
        cap.source_urls.clear();
        assert!(cap.input_limit().is_err());
    }
}
