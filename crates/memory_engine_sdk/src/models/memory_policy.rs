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
    ThreadRepair,
}

impl MemoryPolicyKind {
    pub const ALL: [Self; 4] = [
        Self::Summary,
        Self::Rollup,
        Self::SubjectMemory,
        Self::ThreadRepair,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Rollup => "rollup",
            Self::SubjectMemory => "subject_memory",
            Self::ThreadRepair => "thread_repair",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "summary" => Some(Self::Summary),
            "rollup" => Some(Self::Rollup),
            "subject_memory" | "memory_rollup" => Some(Self::SubjectMemory),
            "thread_repair" => Some(Self::ThreadRepair),
            _ => None,
        }
    }

    pub const fn defaults(self) -> ManagedMemoryPolicy {
        match self {
            Self::Summary => ManagedMemoryPolicy {
                job_type: Self::Summary,
                enabled: true,
                token_limit: Some(6_000),
                target_summary_tokens: Some(700),
                interval_seconds: Some(60),
                max_threads_per_tick: Some(10),
                count_limit: None,
                keep_level0_count: None,
                max_level: None,
            },
            Self::Rollup => ManagedMemoryPolicy {
                job_type: Self::Rollup,
                enabled: true,
                token_limit: Some(6_000),
                target_summary_tokens: Some(700),
                interval_seconds: Some(120),
                max_threads_per_tick: Some(8),
                count_limit: Some(0),
                keep_level0_count: Some(5),
                max_level: Some(4),
            },
            Self::SubjectMemory => ManagedMemoryPolicy {
                job_type: Self::SubjectMemory,
                enabled: true,
                token_limit: Some(6_000),
                target_summary_tokens: Some(700),
                interval_seconds: Some(180),
                max_threads_per_tick: Some(5),
                count_limit: Some(0),
                keep_level0_count: Some(5),
                max_level: Some(4),
            },
            Self::ThreadRepair => ManagedMemoryPolicy {
                job_type: Self::ThreadRepair,
                enabled: true,
                token_limit: Some(200_000),
                target_summary_tokens: None,
                interval_seconds: Some(60),
                max_threads_per_tick: None,
                count_limit: None,
                keep_level0_count: None,
                max_level: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedMemoryPolicy {
    pub job_type: MemoryPolicyKind,
    pub enabled: bool,
    pub token_limit: Option<i64>,
    pub target_summary_tokens: Option<i64>,
    pub interval_seconds: Option<i64>,
    pub max_threads_per_tick: Option<i64>,
    pub count_limit: Option<i64>,
    pub keep_level0_count: Option<i64>,
    pub max_level: Option<i64>,
}

impl ManagedMemoryPolicy {
    pub fn config_key(&self, field: &str) -> String {
        memory_policy_config_key(self.job_type, field)
    }

    pub fn from_config_values(kind: MemoryPolicyKind, values: &BTreeMap<String, Value>) -> Self {
        let mut policy = kind.defaults();
        policy.enabled = config_bool(values, kind, "enabled").unwrap_or(policy.enabled);
        policy.token_limit = config_optional_i64(values, kind, "token_limit", policy.token_limit);
        policy.target_summary_tokens = config_optional_i64(
            values,
            kind,
            "target_summary_tokens",
            policy.target_summary_tokens,
        );
        policy.interval_seconds =
            config_optional_i64(values, kind, "interval_seconds", policy.interval_seconds);
        policy.max_threads_per_tick = config_optional_i64(
            values,
            kind,
            "max_threads_per_tick",
            policy.max_threads_per_tick,
        );
        policy.count_limit = config_optional_i64(values, kind, "count_limit", policy.count_limit);
        policy.keep_level0_count =
            config_optional_i64(values, kind, "keep_level0_count", policy.keep_level0_count);
        policy.max_level = config_optional_i64(values, kind, "max_level", policy.max_level);
        policy.normalized()
    }

    pub fn from_env(kind: MemoryPolicyKind) -> Self {
        let values = memory_policy_env_values(kind);
        Self::from_config_values(kind, &values)
    }

    pub fn apply_to_engine_job_policy(&self, policy: &mut EngineJobPolicy) {
        policy.enabled = self.enabled;
        policy.token_limit = self.token_limit;
        policy.target_summary_tokens = self.target_summary_tokens;
        policy.interval_seconds = self.interval_seconds;
        policy.max_threads_per_tick = self.max_threads_per_tick;
        policy.count_limit = self.count_limit;
        policy.keep_level0_count = self.keep_level0_count;
        policy.max_level = self.max_level;
    }

    pub fn normalized(mut self) -> Self {
        self.token_limit = self.token_limit.map(|value| value.max(128));
        self.target_summary_tokens = self.target_summary_tokens.map(|value| value.max(128));
        self.interval_seconds = self.interval_seconds.map(|value| value.max(3));
        self.max_threads_per_tick = self.max_threads_per_tick.map(|value| value.max(1));
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

    pub fn from_env() -> Self {
        Self {
            environment: std::env::var("CHATOS_ENV").unwrap_or_else(|_| "local".to_string()),
            revision: 0,
            checksum: "environment-fallback".to_string(),
            generated_at: String::new(),
            stale: true,
            source: Some("environment".to_string()),
            policies: MemoryPolicyKind::ALL
                .into_iter()
                .map(ManagedMemoryPolicy::from_env)
                .collect(),
        }
    }

    pub fn policy(&self, kind: MemoryPolicyKind) -> ManagedMemoryPolicy {
        self.policies
            .iter()
            .find(|policy| policy.job_type == kind)
            .cloned()
            .unwrap_or_else(|| kind.defaults())
            .normalized()
    }
}

pub fn memory_policy_config_key(kind: MemoryPolicyKind, field: &str) -> String {
    format!("{MEMORY_POLICY_CONFIG_PREFIX}.{}.{}", kind.as_str(), field)
}

pub fn memory_policy_env_key(kind: MemoryPolicyKind, field: &str) -> String {
    format!(
        "MEMORY_ENGINE_POLICY_{}_{}",
        kind.as_str().to_ascii_uppercase(),
        field.to_ascii_uppercase()
    )
}

pub fn managed_memory_policy_env_available(kind: MemoryPolicyKind) -> bool {
    [
        "enabled",
        "token_limit",
        "target_summary_tokens",
        "interval_seconds",
        "max_threads_per_tick",
        "count_limit",
        "keep_level0_count",
        "max_level",
    ]
    .into_iter()
    .any(|field| std::env::var_os(memory_policy_env_key(kind, field)).is_some())
}

fn memory_policy_env_values(kind: MemoryPolicyKind) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    for field in [
        "enabled",
        "token_limit",
        "target_summary_tokens",
        "interval_seconds",
        "max_threads_per_tick",
        "count_limit",
        "keep_level0_count",
        "max_level",
    ] {
        let Ok(raw) = std::env::var(memory_policy_env_key(kind, field)) else {
            continue;
        };
        let value = match field {
            "enabled" => parse_bool(raw.as_str()).map(Value::Bool),
            _ => raw
                .trim()
                .parse::<i64>()
                .ok()
                .map(|value| Value::Number(value.into())),
        };
        if let Some(value) = value {
            values.insert(memory_policy_config_key(kind, field), value);
        }
    }
    values
}

fn config_bool(
    values: &BTreeMap<String, Value>,
    kind: MemoryPolicyKind,
    field: &str,
) -> Option<bool> {
    values
        .get(memory_policy_config_key(kind, field).as_str())
        .and_then(|value| match value {
            Value::Bool(value) => Some(*value),
            Value::String(value) => parse_bool(value),
            Value::Number(value) => value.as_i64().map(|value| value != 0),
            _ => None,
        })
}

fn config_optional_i64(
    values: &BTreeMap<String, Value>,
    kind: MemoryPolicyKind,
    field: &str,
    fallback: Option<i64>,
) -> Option<i64> {
    match values.get(memory_policy_config_key(kind, field).as_str()) {
        Some(Value::Null) => None,
        Some(Value::Number(value)) => value.as_i64().or(fallback),
        Some(Value::String(value)) => value.trim().parse::<i64>().ok().or(fallback),
        Some(_) | None => fallback,
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_values_override_shared_defaults() {
        let kind = MemoryPolicyKind::Rollup;
        let values = BTreeMap::from([
            (
                memory_policy_config_key(kind, "enabled"),
                Value::Bool(false),
            ),
            (
                memory_policy_config_key(kind, "token_limit"),
                Value::Number(9_000.into()),
            ),
        ]);

        let policy = ManagedMemoryPolicy::from_config_values(kind, &values);
        assert!(!policy.enabled);
        assert_eq!(policy.token_limit, Some(9_000));
        assert_eq!(policy.keep_level0_count, Some(5));
    }

    #[test]
    fn memory_rollup_uses_subject_memory_policy() {
        assert_eq!(
            MemoryPolicyKind::parse("memory_rollup"),
            Some(MemoryPolicyKind::SubjectMemory)
        );
    }
}
