// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

pub const MEMORY_FEATURE_FLAG_SCHEMA_VERSION: &str = "memory_feature_flag_v1";
pub const MEMORY_FEATURE_FLAG_CONFIG_PREFIX: &str = "memory_engine.feature.";

pub const MEMORY_PARTITION_DISPATCH_V2_FLAG: &str = "memory_partition_dispatch_v2";
pub const MEMORY_GENERATION_COMMIT_V2_FLAG: &str = "memory_generation_commit_v2";
pub const MEMORY_COVERAGE_V2_FLAG: &str = "memory_coverage_v2";
pub const MEMORY_PROJECT_ROLLUP_V2_FLAG: &str = "memory_project_rollup_v2";
pub const MEMORY_COMPOSE_V2_SHADOW_FLAG: &str = "memory_compose_v2_shadow";
pub const MEMORY_COMPOSE_V2_INJECT_FLAG: &str = "memory_compose_v2_inject";
pub const MEMORY_STRUCTURED_PAYLOAD_V1_FLAG: &str = "memory_structured_payload_v1";
pub const MEMORY_LEXICAL_RETRIEVAL_V1_FLAG: &str = "memory_lexical_retrieval_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryFeatureFlagScope {
    pub tenant_id: String,
    pub source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryFeatureFlagConfig {
    pub schema_version: String,
    pub revision: u64,
    pub enabled: bool,
    pub emergency_disabled: bool,
    #[serde(default)]
    pub tenant_source_allowlist: Vec<MemoryFeatureFlagScope>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub readiness_gates: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryFeatureFlagDefinition {
    pub name: &'static str,
    pub dependencies: &'static [&'static str],
    pub readiness_gates: &'static [&'static str],
}

impl MemoryFeatureFlagDefinition {
    pub fn config_key(self) -> String {
        memory_feature_flag_config_key(self.name)
    }

    pub fn default_config(self) -> MemoryFeatureFlagConfig {
        MemoryFeatureFlagConfig {
            schema_version: MEMORY_FEATURE_FLAG_SCHEMA_VERSION.to_string(),
            revision: 1,
            enabled: false,
            emergency_disabled: false,
            tenant_source_allowlist: Vec::new(),
            dependencies: self
                .dependencies
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            readiness_gates: self
                .readiness_gates
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        }
    }
}

pub const MEMORY_FEATURE_FLAG_DEFINITIONS: &[MemoryFeatureFlagDefinition] = &[
    MemoryFeatureFlagDefinition {
        name: MEMORY_PARTITION_DISPATCH_V2_FLAG,
        dependencies: &[],
        readiness_gates: &[],
    },
    MemoryFeatureFlagDefinition {
        name: MEMORY_GENERATION_COMMIT_V2_FLAG,
        dependencies: &[MEMORY_PARTITION_DISPATCH_V2_FLAG],
        readiness_gates: &[],
    },
    MemoryFeatureFlagDefinition {
        name: MEMORY_COVERAGE_V2_FLAG,
        dependencies: &[MEMORY_GENERATION_COMMIT_V2_FLAG],
        readiness_gates: &["source_contract_ready"],
    },
    MemoryFeatureFlagDefinition {
        name: MEMORY_PROJECT_ROLLUP_V2_FLAG,
        dependencies: &[MEMORY_COVERAGE_V2_FLAG],
        readiness_gates: &["runtime_context_assertion_verified"],
    },
    MemoryFeatureFlagDefinition {
        name: MEMORY_COMPOSE_V2_SHADOW_FLAG,
        dependencies: &[],
        readiness_gates: &[],
    },
    MemoryFeatureFlagDefinition {
        name: MEMORY_COMPOSE_V2_INJECT_FLAG,
        dependencies: &[MEMORY_COMPOSE_V2_SHADOW_FLAG],
        readiness_gates: &["history_repair_complete"],
    },
    MemoryFeatureFlagDefinition {
        name: MEMORY_STRUCTURED_PAYLOAD_V1_FLAG,
        dependencies: &[MEMORY_GENERATION_COMMIT_V2_FLAG],
        readiness_gates: &[],
    },
    MemoryFeatureFlagDefinition {
        name: MEMORY_LEXICAL_RETRIEVAL_V1_FLAG,
        dependencies: &[MEMORY_STRUCTURED_PAYLOAD_V1_FLAG],
        readiness_gates: &["fact_projection_ready"],
    },
];

pub fn memory_feature_flag_config_key(name: &str) -> String {
    format!("{MEMORY_FEATURE_FLAG_CONFIG_PREFIX}{name}")
}

pub fn memory_feature_flag_definition(name: &str) -> Option<MemoryFeatureFlagDefinition> {
    MEMORY_FEATURE_FLAG_DEFINITIONS
        .iter()
        .copied()
        .find(|definition| definition.name == name)
}

impl MemoryFeatureFlagConfig {
    pub fn validate_for(&self, definition: MemoryFeatureFlagDefinition) -> Vec<String> {
        let mut errors = Vec::new();
        if self.schema_version != MEMORY_FEATURE_FLAG_SCHEMA_VERSION {
            errors.push(format!(
                "schema_version must be {MEMORY_FEATURE_FLAG_SCHEMA_VERSION}"
            ));
        }
        if self.revision == 0 {
            errors.push("revision must be greater than zero".to_string());
        }
        let expected_dependencies = definition
            .dependencies
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        if self.dependencies != expected_dependencies {
            errors.push(format!(
                "dependencies must be [{}]",
                expected_dependencies.join(", ")
            ));
        }
        let expected_readiness = definition
            .readiness_gates
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        if self.readiness_gates != expected_readiness {
            errors.push(format!(
                "readiness_gates must be [{}]",
                expected_readiness.join(", ")
            ));
        }
        for (index, scope) in self.tenant_source_allowlist.iter().enumerate() {
            if scope.tenant_id.trim().is_empty() || scope.source_id.trim().is_empty() {
                errors.push(format!(
                    "tenant_source_allowlist[{index}] requires non-empty tenant_id and source_id"
                ));
            }
        }
        let mut normalized = self
            .tenant_source_allowlist
            .iter()
            .map(|scope| (scope.tenant_id.trim(), scope.source_id.trim()))
            .collect::<Vec<_>>();
        normalized.sort_unstable();
        normalized.dedup();
        if normalized.len() != self.tenant_source_allowlist.len() {
            errors.push("tenant_source_allowlist contains duplicate scopes".to_string());
        }
        errors
    }

    pub fn is_enabled_for(&self, tenant_id: &str, source_id: &str) -> bool {
        self.enabled
            && !self.emergency_disabled
            && self.tenant_source_allowlist.iter().any(|scope| {
                scope.tenant_id == tenant_id.trim() && scope.source_id == source_id.trim()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_closed_and_preserve_declared_dependencies() {
        for definition in MEMORY_FEATURE_FLAG_DEFINITIONS {
            let value = definition.default_config();
            assert!(!value.enabled);
            assert!(!value.emergency_disabled);
            assert!(value.tenant_source_allowlist.is_empty());
            assert!(value.validate_for(*definition).is_empty());
        }
    }

    #[test]
    fn emergency_switch_wins_over_an_allowlisted_scope() {
        let mut value = MEMORY_FEATURE_FLAG_DEFINITIONS[0].default_config();
        value.enabled = true;
        value.tenant_source_allowlist.push(MemoryFeatureFlagScope {
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
        });
        assert!(value.is_enabled_for("tenant-1", "source-1"));
        value.emergency_disabled = true;
        assert!(!value.is_enabled_for("tenant-1", "source-1"));
    }

    #[test]
    fn mutable_dependency_contract_is_rejected() {
        let definition = memory_feature_flag_definition(MEMORY_COVERAGE_V2_FLAG).unwrap();
        let mut value = definition.default_config();
        value.dependencies.clear();
        assert!(!value.validate_for(definition).is_empty());
    }
}
