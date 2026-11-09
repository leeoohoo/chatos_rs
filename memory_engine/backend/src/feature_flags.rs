// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use chatos_config_sdk::ConfigSnapshot;
use memory_engine_sdk::{MemoryFeatureFlagConfig, MEMORY_FEATURE_FLAG_DEFINITIONS};
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEngineFeatureFlags {
    pub config_revision: i64,
    configs: BTreeMap<String, MemoryFeatureFlagConfig>,
}

impl MemoryEngineFeatureFlags {
    pub fn disabled(config_revision: i64) -> Self {
        Self {
            config_revision,
            configs: MEMORY_FEATURE_FLAG_DEFINITIONS
                .iter()
                .map(|definition| (definition.name.to_string(), definition.default_config()))
                .collect(),
        }
    }

    pub fn from_snapshot(snapshot: &ConfigSnapshot) -> Result<Self, String> {
        let mut configs = BTreeMap::new();
        for definition in MEMORY_FEATURE_FLAG_DEFINITIONS {
            let key = definition.config_key();
            let config = match snapshot.value(key.as_str()) {
                Some(value) => serde_json::from_value::<MemoryFeatureFlagConfig>(value.clone())
                    .map_err(|error| format!("invalid {key}: {error}"))?,
                None => definition.default_config(),
            };
            let errors = config.validate_for(*definition);
            if !errors.is_empty() {
                return Err(format!("invalid {key}: {}", errors.join("; ")));
            }
            configs.insert(definition.name.to_string(), config);
        }
        Ok(Self {
            config_revision: snapshot.revision,
            configs,
        })
    }

    pub fn is_enabled_for(&self, name: &str, tenant_id: &str, source_id: &str) -> bool {
        self.is_enabled_for_inner(name, tenant_id, source_id, &mut BTreeSet::new())
    }

    fn is_enabled_for_inner(
        &self,
        name: &str,
        tenant_id: &str,
        source_id: &str,
        visited: &mut BTreeSet<String>,
    ) -> bool {
        if !visited.insert(name.to_string()) {
            return false;
        }
        let Some(definition) = memory_engine_sdk::memory_feature_flag_definition(name) else {
            return false;
        };
        let Some(config) = self.configs.get(name) else {
            return false;
        };
        config.is_enabled_for(tenant_id, source_id)
            && definition.dependencies.iter().all(|dependency| {
                self.is_enabled_for_inner(dependency, tenant_id, source_id, visited)
            })
    }
}

#[derive(Clone)]
pub struct MemoryEngineFeatureState {
    sender: watch::Sender<MemoryEngineFeatureFlags>,
}

impl MemoryEngineFeatureState {
    pub fn new(initial: MemoryEngineFeatureFlags) -> Self {
        let (sender, _) = watch::channel(initial);
        Self { sender }
    }

    pub fn snapshot(&self) -> MemoryEngineFeatureFlags {
        self.sender.borrow().clone()
    }

    pub fn update(&self, next: MemoryEngineFeatureFlags) -> bool {
        if *self.sender.borrow() == next {
            return false;
        }
        self.sender.send_replace(next);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_engine_sdk::{
        MemoryFeatureFlagScope, MEMORY_COVERAGE_V2_FLAG, MEMORY_GENERATION_COMMIT_V2_FLAG,
        MEMORY_PARTITION_DISPATCH_V2_FLAG,
    };

    fn snapshot() -> ConfigSnapshot {
        let values = MEMORY_FEATURE_FLAG_DEFINITIONS
            .iter()
            .map(|definition| {
                (
                    definition.config_key(),
                    serde_json::to_value(definition.default_config()).unwrap(),
                )
            })
            .collect();
        ConfigSnapshot {
            environment: "test".to_string(),
            service_name: "memory-engine".to_string(),
            revision: 7,
            checksum: "checksum".to_string(),
            values,
            env: BTreeMap::new(),
            generated_at: "2026-11-09T00:00:00Z".to_string(),
            stale: false,
            source: None,
        }
    }

    fn enable(snapshot: &mut ConfigSnapshot, name: &str) {
        let definition = memory_engine_sdk::memory_feature_flag_definition(name).unwrap();
        let mut config = definition.default_config();
        config.enabled = true;
        config.tenant_source_allowlist = vec![MemoryFeatureFlagScope {
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
        }];
        snapshot.values.insert(
            definition.config_key(),
            serde_json::to_value(config).unwrap(),
        );
    }

    #[test]
    fn missing_new_keys_fail_closed_without_failing_service_startup() {
        let snapshot = ConfigSnapshot {
            values: BTreeMap::new(),
            ..snapshot()
        };
        let flags = MemoryEngineFeatureFlags::from_snapshot(&snapshot).unwrap();
        assert!(!flags.is_enabled_for(MEMORY_PARTITION_DISPATCH_V2_FLAG, "tenant-1", "source-1"));
    }

    #[test]
    fn runtime_evaluation_enforces_transitive_dependencies() {
        let mut snapshot = snapshot();
        enable(&mut snapshot, MEMORY_COVERAGE_V2_FLAG);
        let flags = MemoryEngineFeatureFlags::from_snapshot(&snapshot).unwrap();
        assert!(!flags.is_enabled_for(MEMORY_COVERAGE_V2_FLAG, "tenant-1", "source-1"));

        enable(&mut snapshot, MEMORY_GENERATION_COMMIT_V2_FLAG);
        enable(&mut snapshot, MEMORY_PARTITION_DISPATCH_V2_FLAG);
        let flags = MemoryEngineFeatureFlags::from_snapshot(&snapshot).unwrap();
        assert!(flags.is_enabled_for(MEMORY_COVERAGE_V2_FLAG, "tenant-1", "source-1"));
    }
}
