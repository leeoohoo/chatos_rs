// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::models::ai_model_config::AiModelConfig;

pub(super) fn fallback_model_list(profile: &AiModelConfig) -> Vec<Value> {
    let model = profile.model.trim();
    if model.is_empty() {
        return Vec::new();
    }
    vec![json!({
        "id": model,
        "owned_by": profile.provider,
        "context_length": null,
        "supports_images": profile.supports_images,
        "supports_video": false,
        "supports_reasoning": profile.supports_reasoning,
        "supports_responses": profile.supports_responses,
        "raw": null,
    })]
}
