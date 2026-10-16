// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Deserialize;
use std::{collections::HashMap, sync::LazyLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelReasoningCapabilities {
    pub model: String,
    pub levels: Vec<String>,
    pub default_level: String,
    legacy_aliases: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Catalog {
    models: Vec<ModelReasoningCapabilities>,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../clients/macos/Sources/ChatOSModelCapabilities/Resources/ModelReasoningCapabilities.json"
    )).expect("valid shared model reasoning capabilities")
});

pub fn capabilities_for_model(model: &str) -> Option<&'static ModelReasoningCapabilities> {
    let model = model.trim().to_ascii_lowercase();
    CATALOG.models.iter().find(|capability| {
        model == capability.model
            || model
                .strip_prefix(&format!("{}-", capability.model))
                .is_some_and(|suffix| {
                    suffix.len() == 10 && suffix.chars().all(|c| c.is_ascii_digit() || c == '-')
                })
    })
}

impl ModelReasoningCapabilities {
    pub fn normalized(&self, level: Option<&str>) -> Option<String> {
        let level = level?.trim().to_ascii_lowercase();
        if self.levels.contains(&level) {
            return Some(level);
        }
        self.legacy_aliases.get(&level).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_contract_preserves_supported_levels_and_repairs_legacy_values() {
        for model in [
            "gpt-5.6-sol",
            " GPT-5.6-SOL ",
            "gpt-5.6-sol-2026-10-09",
            "gpt-6.1-sol",
            " GPT-6.1-SOL ",
            "gpt-6.1-sol-2026-10-09",
        ] {
            let caps = capabilities_for_model(model).unwrap();
            assert_eq!(caps.default_level, "medium");
            for level in &caps.levels {
                assert_eq!(caps.normalized(Some(level)), Some(level.clone()));
            }
            for level in ["none", "minimal", "off", "disabled"] {
                assert_eq!(caps.normalized(Some(level)), Some("low".into()));
            }
            assert_eq!(caps.normalized(Some("auto")), None);
        }
        assert!(capabilities_for_model("gpt-6.1-solitude").is_none());
        assert!(capabilities_for_model("gpt-5.6-solitude").is_none());
    }
}
