// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{config::AppConfig, models::EngineModelProfile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct MemoryModelBudget {
    pub model_config_id: String,
    pub provider: String,
    pub model: String,
    pub revision: String,
    pub context_window_tokens: i64,
    pub output_reserve_tokens: i64,
    pub protocol_overhead_estimator_id: String,
    pub tokenizer_id: Option<String>,
}

impl MemoryModelBudget {
    pub(crate) fn from_profile(
        config: &AppConfig,
        profile: &EngineModelProfile,
    ) -> Result<Self, String> {
        let capability = config
            .model_capabilities
            .get(&profile.model_capability_revision)
            .ok_or_else(|| {
                format!(
                    "model {} capability revision {} is not published in Configuration Center",
                    profile.model, profile.model_capability_revision
                )
            })?;
        Self::from_published_capability(profile, capability)
    }

    fn from_published_capability(
        profile: &EngineModelProfile,
        capability: &crate::config::PublishedModelCapability,
    ) -> Result<Self, String> {
        capability.input_limit()?;
        if capability.excluded_models.contains(&profile.model) {
            return Err("model has been removed from the selectable catalog".into());
        }
        if i64::try_from(capability.context_window_tokens).ok()
            != Some(profile.context_window_tokens)
            || i64::try_from(capability.output_reserve_tokens).ok()
                != Some(profile.output_reserve_tokens)
            || capability.protocol_overhead_estimator_id != profile.protocol_overhead_estimator_id
            || capability.tokenizer_id != profile.tokenizer_id
        {
            return Err(format!(
                "model {} capability differs from its published Configuration Center revision",
                profile.model
            ));
        }
        let budget = Self {
            model_config_id: profile.id.clone(),
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            revision: profile.model_capability_revision.clone(),
            context_window_tokens: profile.context_window_tokens,
            output_reserve_tokens: profile.output_reserve_tokens,
            protocol_overhead_estimator_id: profile.protocol_overhead_estimator_id.clone(),
            tokenizer_id: profile.tokenizer_id.clone(),
        };
        budget.input_limit()?;
        Ok(budget)
    }

    pub(crate) fn input_limit(&self) -> Result<i64, String> {
        if self.revision.trim().is_empty()
            || self.protocol_overhead_estimator_id.trim().is_empty()
            || self.context_window_tokens <= 0
            || self.output_reserve_tokens <= 0
        {
            return Err("a complete published common capacity snapshot is required".into());
        }
        self.context_window_tokens
            .checked_sub(self.output_reserve_tokens)
            .filter(|limit| *limit > 0)
            .ok_or_else(|| "model output reserve leaves no input capacity".into())
    }
}

#[cfg(test)]
pub(crate) fn test_budget(input_limit: i64) -> MemoryModelBudget {
    MemoryModelBudget {
        model_config_id: "test-model".into(),
        provider: "openai".into(),
        model: "test-model".into(),
        revision: "test-published-capability".into(),
        context_window_tokens: input_limit + 512,
        output_reserve_tokens: 512,
        protocol_overhead_estimator_id: "responses-json-v1".into(),
        tokenizer_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_changes_only_with_the_published_model_capability() {
        let budget = test_budget(9_137);
        assert_eq!(budget.input_limit().unwrap(), 9_137);
        let profile: EngineModelProfile = serde_json::from_value(serde_json::json!({
            "id":budget.model_config_id,"name":"test","provider":budget.provider,"model":budget.model,
            "model_capability_revision":budget.revision,"context_window_tokens":budget.context_window_tokens,
            "output_reserve_tokens":budget.output_reserve_tokens,"protocol_overhead_estimator_id":budget.protocol_overhead_estimator_id,
            "tokenizer_id":null,"created_at":"","updated_at":""
        })).unwrap();
        let mut capability = crate::config::PublishedModelCapability {
            context_window_tokens: profile.context_window_tokens as usize,
            output_reserve_tokens: profile.output_reserve_tokens as usize,
            protocol_overhead_estimator_id: profile.protocol_overhead_estimator_id.clone(),
            tokenizer_id: None,
            source_urls: vec!["https://specification.example/model".into()],
            excluded_models: Vec::new(),
        };
        assert_eq!(
            MemoryModelBudget::from_published_capability(&profile, &capability)
                .unwrap()
                .input_limit()
                .unwrap(),
            9_137
        );
        let mut changed = profile.clone();
        changed.context_window_tokens += 7_031;
        assert!(MemoryModelBudget::from_published_capability(&changed, &capability).is_err());
        capability.context_window_tokens += 7_031;
        assert_eq!(
            MemoryModelBudget::from_published_capability(&changed, &capability)
                .unwrap()
                .input_limit()
                .unwrap(),
            16_168
        );
        changed.tokenizer_id = Some("different".into());
        assert!(MemoryModelBudget::from_published_capability(&changed, &capability).is_err());
        capability.excluded_models.push(profile.model.clone());
        assert!(MemoryModelBudget::from_published_capability(&profile, &capability).is_err());
    }
}
