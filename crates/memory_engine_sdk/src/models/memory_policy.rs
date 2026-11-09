// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::EngineJobPolicy;

pub const MEMORY_POLICY_CONFIG_PREFIX: &str = "memory_engine.policy";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPolicyKind {
    Summary,
    Rollup,
    SubjectMemory,
}

impl MemoryPolicyKind {
    pub const ALL: [Self; 3] = [Self::Summary, Self::Rollup, Self::SubjectMemory];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Rollup => "rollup",
            Self::SubjectMemory => "subject_memory",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "summary" => Some(Self::Summary),
            "rollup" => Some(Self::Rollup),
            "subject_memory" => Some(Self::SubjectMemory),
            _ => None,
        }
    }

    pub const fn defaults(self) -> ManagedMemoryPolicy {
        match self {
            Self::Summary => ManagedMemoryPolicy {
                job_type: Self::Summary,
                dispatch_threshold_tokens: Some(4_000),
                max_source_batch_tokens: Some(16_000),
                max_model_input_tokens: Some(32_000),
                max_leaf_tokens: Some(6_000),
                max_merge_rounds: Some(8),
                count_limit: None,
                keep_level0_count: None,
                max_level: None,
            },
            Self::Rollup => ManagedMemoryPolicy {
                job_type: Self::Rollup,
                dispatch_threshold_tokens: Some(12_000),
                max_source_batch_tokens: Some(48_000),
                max_model_input_tokens: Some(32_000),
                max_leaf_tokens: None,
                max_merge_rounds: None,
                count_limit: Some(12),
                keep_level0_count: Some(4),
                max_level: Some(4),
            },
            Self::SubjectMemory => ManagedMemoryPolicy {
                job_type: Self::SubjectMemory,
                dispatch_threshold_tokens: Some(8_000),
                max_source_batch_tokens: Some(32_000),
                max_model_input_tokens: Some(32_000),
                max_leaf_tokens: None,
                max_merge_rounds: None,
                count_limit: Some(12),
                keep_level0_count: Some(4),
                max_level: Some(4),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedMemoryPolicy {
    pub job_type: MemoryPolicyKind,
    pub dispatch_threshold_tokens: Option<i64>,
    pub max_source_batch_tokens: Option<i64>,
    pub max_model_input_tokens: Option<i64>,
    pub max_leaf_tokens: Option<i64>,
    pub max_merge_rounds: Option<i64>,
    pub count_limit: Option<i64>,
    pub keep_level0_count: Option<i64>,
    pub max_level: Option<i64>,
}

impl ManagedMemoryPolicy {
    pub fn config_key(&self, field: &str) -> String {
        memory_policy_config_key(self.job_type, field)
    }

    pub fn from_config_values(kind: MemoryPolicyKind, values: &BTreeMap<String, Value>) -> Self {
        ManagedMemoryPolicy {
            job_type: kind,
            dispatch_threshold_tokens: config_optional_i64(
                values,
                kind,
                "dispatch_threshold_tokens",
            ),
            max_source_batch_tokens: config_optional_i64(values, kind, "max_source_batch_tokens"),
            max_model_input_tokens: config_optional_i64(values, kind, "max_model_input_tokens"),
            max_leaf_tokens: config_optional_i64(values, kind, "max_leaf_tokens"),
            max_merge_rounds: config_optional_i64(values, kind, "max_merge_rounds"),
            count_limit: config_optional_i64(values, kind, "count_limit"),
            keep_level0_count: config_optional_i64(values, kind, "keep_level0_count"),
            max_level: config_optional_i64(values, kind, "max_level"),
        }
        .normalized()
    }

    pub fn apply_to_engine_job_policy(&self, policy: &mut EngineJobPolicy) {
        policy.dispatch_threshold_tokens = self.dispatch_threshold_tokens;
        policy.max_source_batch_tokens = self.max_source_batch_tokens;
        policy.max_model_input_tokens = self.max_model_input_tokens;
        policy.max_leaf_tokens = self.max_leaf_tokens;
        policy.max_merge_rounds = self.max_merge_rounds;
        policy.count_limit = self.count_limit;
        policy.keep_level0_count = self.keep_level0_count;
        policy.max_level = self.max_level;
    }

    pub fn normalized(mut self) -> Self {
        self.dispatch_threshold_tokens = self.dispatch_threshold_tokens.map(|value| value.max(1));
        self.max_source_batch_tokens = self.max_source_batch_tokens.map(|value| value.max(128));
        self.max_model_input_tokens = self.max_model_input_tokens.map(|value| value.max(128));
        self.max_leaf_tokens = self.max_leaf_tokens.map(|value| value.max(128));
        self.max_merge_rounds = self.max_merge_rounds.map(|value| value.max(1));
        self.count_limit = self.count_limit.map(|value| value.max(0));
        self.keep_level0_count = self.keep_level0_count.map(|value| value.max(0));
        self.max_level = self.max_level.map(|value| value.max(1));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedMemoryPolicyBundle {
    pub environment: String,
    pub revision: i64,
    pub checksum: String,
    pub generated_at: String,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub policies: Vec<ManagedMemoryPolicy>,
}

impl ManagedMemoryPolicyBundle {
    pub fn from_config_values(
        environment: impl Into<String>,
        revision: i64,
        checksum: impl Into<String>,
        generated_at: impl Into<String>,
        stale: bool,
        source: Option<String>,
        values: &BTreeMap<String, Value>,
    ) -> Self {
        Self {
            environment: environment.into(),
            revision,
            checksum: checksum.into(),
            generated_at: generated_at.into(),
            stale,
            source,
            policies: MemoryPolicyKind::ALL
                .into_iter()
                .map(|kind| ManagedMemoryPolicy::from_config_values(kind, values))
                .collect(),
        }
    }

    pub fn policy(&self, kind: MemoryPolicyKind) -> Option<ManagedMemoryPolicy> {
        self.policies
            .iter()
            .find(|policy| policy.job_type == kind)
            .cloned()
            .map(ManagedMemoryPolicy::normalized)
    }
}

pub fn memory_policy_config_key(kind: MemoryPolicyKind, field: &str) -> String {
    format!("{MEMORY_POLICY_CONFIG_PREFIX}.{}.{}", kind.as_str(), field)
}

fn config_optional_i64(
    values: &BTreeMap<String, Value>,
    kind: MemoryPolicyKind,
    field: &str,
) -> Option<i64> {
    match values.get(memory_policy_config_key(kind, field).as_str()) {
        Some(Value::Null) => None,
        Some(Value::Number(value)) => value.as_i64(),
        Some(Value::String(value)) => value.trim().parse::<i64>().ok(),
        Some(_) | None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_values_override_shared_defaults() {
        let kind = MemoryPolicyKind::Rollup;
        let values = BTreeMap::from([(
            memory_policy_config_key(kind, "max_model_input_tokens"),
            Value::Number(9_000.into()),
        )]);

        let policy = ManagedMemoryPolicy::from_config_values(kind, &values);
        assert_eq!(policy.max_model_input_tokens, Some(9_000));
        assert_eq!(policy.keep_level0_count, None);
    }

    #[test]
    fn retired_policy_alias_is_rejected() {
        assert_eq!(MemoryPolicyKind::parse("memory_rollup"), None);
    }
}
