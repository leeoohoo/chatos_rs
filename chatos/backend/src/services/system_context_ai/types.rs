// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::core::internal_context_locale::InternalContextLocale;

#[derive(Debug, Clone, Default)]
pub struct PromptRuntimeOverrides {
    pub temperature: Option<f64>,
}

pub struct GenerateDraftInput {
    pub user_id: Option<String>,
    pub internal_context_locale: InternalContextLocale,
    pub scene: Option<String>,
    pub style: Option<String>,
    pub language: Option<String>,
    pub output_format: Option<String>,
    pub constraints: Option<Vec<String>>,
    pub forbidden: Option<Vec<String>>,
    pub candidate_count: Option<usize>,
    pub model_config_id: Option<String>,
    pub ai_model_config: Option<PromptRuntimeOverrides>,
}

pub struct OptimizeDraftInput {
    pub user_id: Option<String>,
    pub internal_context_locale: InternalContextLocale,
    pub content: Option<String>,
    pub goal: Option<String>,
    pub keep_intent: Option<bool>,
    pub model_config_id: Option<String>,
    pub ai_model_config: Option<PromptRuntimeOverrides>,
}

pub struct EvaluateDraftInput {
    pub internal_context_locale: InternalContextLocale,
    pub content: Option<String>,
    pub model_config_id: Option<String>,
    pub ai_model_config: Option<PromptRuntimeOverrides>,
}

#[derive(Debug)]
pub enum SystemContextAiError {
    BadRequest {
        message: String,
    },
    Upstream {
        message: String,
        raw: Option<String>,
    },
}

impl PromptRuntimeOverrides {
    pub fn sanitize(self) -> Self {
        Self {
            temperature: self.temperature.map(|value| value.clamp(0.0, 2.0)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
    }

    pub fn into_value(self) -> Option<Value> {
        let mut map = serde_json::Map::new();
        if let Some(temperature) = self.temperature {
            map.insert("temperature".to_string(), Value::from(temperature));
        }
        if map.is_empty() {
            None
        } else {
            Some(Value::Object(map))
        }
    }
}
