// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn definitions(now: &str) -> Vec<ConfigDefinitionRecord> {
    memory_engine_sdk::MEMORY_FEATURE_FLAG_DEFINITIONS
        .iter()
        .enumerate()
        .map(|(index, flag)| {
            let key = flag.config_key();
            definition(
                key.as_str(),
                flag.name,
                "Memory Engine reliability rollout flag. The JSON contract is fail-closed and carries an emergency switch, exact tenant/source allowlist, immutable dependencies, and readiness gates.",
                "Memory Engine / Feature Flags",
                "service",
                Some("memory-engine"),
                "json",
                serde_json::to_value(flag.default_config())
                    .expect("serialize Memory Engine feature flag default"),
                None,
                None,
                &[],
                "hot_reload",
                &[],
                3_640 + i32::try_from(index).unwrap_or_default(),
                now,
            )
        })
        .collect()
}
