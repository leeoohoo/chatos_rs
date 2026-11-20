// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::watch;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigSnapshot {
    pub environment: String,
    pub service_name: String,
    pub revision: i64,
    pub checksum: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub generated_at: String,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PlatformPressureLevel {
    Normal,
    Elevated,
    Critical,
}

impl PlatformPressureLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Elevated => "elevated",
            Self::Critical => "critical",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "normal" => Ok(Self::Normal),
            "elevated" => Ok(Self::Elevated),
            "critical" => Ok(Self::Critical),
            _ => Err("pressure level must be normal, elevated, or critical".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServicePressureSignal {
    pub level: PlatformPressureLevel,
    pub reason: String,
}

#[derive(Clone)]
pub struct ConfigWatchState<Value: Clone + PartialEq> {
    sender: watch::Sender<Value>,
}

impl<Value: Clone + PartialEq> ConfigWatchState<Value> {
    pub fn new(initial: Value) -> Self {
        let (sender, _) = watch::channel(initial);
        Self { sender }
    }

    pub fn snapshot(&self) -> Value {
        self.sender.borrow().clone()
    }

    pub fn update(&self, next: Value) -> bool {
        if *self.sender.borrow() == next {
            return false;
        }
        self.sender.send_replace(next);
        true
    }
}

impl ConfigSnapshot {
    pub fn etag(&self) -> String {
        format!("\"{}-{}\"", self.revision, self.checksum)
    }

    pub fn value(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn string(&self, key: &str) -> Option<String> {
        self.value(key).and_then(|value| match value {
            Value::String(value) => Some(value.clone()),
            Value::Bool(value) => Some(value.to_string()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        self.value(key).and_then(|value| match value {
            Value::Bool(value) => Some(*value),
            Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            },
            Value::Number(value) => value.as_i64().map(|value| value != 0),
            _ => None,
        })
    }

    pub fn i64(&self, key: &str) -> Option<i64> {
        self.value(key).and_then(|value| match value {
            Value::Number(value) => value.as_i64(),
            Value::String(value) => value.trim().parse::<i64>().ok(),
            _ => None,
        })
    }

    pub fn u64(&self, key: &str) -> Option<u64> {
        self.value(key).and_then(|value| match value {
            Value::Number(value) => value.as_u64(),
            Value::String(value) => value.trim().parse::<u64>().ok(),
            _ => None,
        })
    }

    pub fn usize(&self, key: &str) -> Option<usize> {
        self.u64(key).and_then(|value| usize::try_from(value).ok())
    }

    pub fn required_u64(&self, key: &str, minimum: u64, maximum: u64) -> Result<u64, String> {
        let value = self
            .u64(key)
            .ok_or_else(|| format!("{key} is required from configuration center"))?;
        if !(minimum..=maximum).contains(&value) {
            return Err(format!("{key} must be between {minimum} and {maximum}"));
        }
        Ok(value)
    }

    pub fn required_usize(
        &self,
        key: &str,
        minimum: usize,
        maximum: usize,
    ) -> Result<usize, String> {
        let value = self
            .usize(key)
            .ok_or_else(|| format!("{key} is required from configuration center"))?;
        if !(minimum..=maximum).contains(&value) {
            return Err(format!("{key} must be between {minimum} and {maximum}"));
        }
        Ok(value)
    }

    pub fn required_pressure_level(&self, key: &str) -> Result<PlatformPressureLevel, String> {
        self.value(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{key} is required from configuration center"))
            .and_then(PlatformPressureLevel::parse)
    }

    pub fn with_source(mut self, source: impl Into<String>, stale: bool) -> Self {
        self.source = Some(source.into());
        self.stale = stale;
        self
    }
}
