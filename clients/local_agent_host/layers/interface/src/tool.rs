// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text, LocalAgentRunRecord, LOCAL_AGENT_MAX_INPUT_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentToolCall {
    pub call_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub arguments: Value,
    #[serde(default)]
    pub side_effecting: bool,
}

impl LocalAgentToolCall {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("call_id", &self.call_id)?;
        validate_identifier("tool_name", &self.tool_name)?;
        let size = serde_json::to_vec(&self.arguments)
            .map_err(|error| format!("tool arguments are not serializable: {error}"))?
            .len();
        if size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "tool arguments exceed the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentToolBatch {
    pub batch_id: String,
    pub calls: Vec<LocalAgentToolCall>,
}

impl LocalAgentToolBatch {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("batch_id", &self.batch_id)?;
        if self.calls.is_empty() || self.calls.len() > 128 {
            return Err("tool batch must contain 1..=128 calls".to_string());
        }
        let mut call_ids = HashSet::with_capacity(self.calls.len());
        for call in &self.calls {
            call.validate()?;
            if !call_ids.insert(call.call_id.as_str()) {
                return Err(format!(
                    "tool batch contains duplicate call_id: {}",
                    call.call_id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimNextToolCommand {
    pub worker_id: String,
    pub lease_duration_ms: u64,
    #[serde(default)]
    pub include_tool_names: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_tool_names: Vec<String>,
}

impl ClaimNextToolCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("worker_id", &self.worker_id)?;
        if !(1_000..=300_000).contains(&self.lease_duration_ms) {
            return Err("lease_duration_ms must be between 1000 and 300000".to_string());
        }
        if self
            .include_tool_names
            .as_ref()
            .is_some_and(|names| names.is_empty())
        {
            return Err("include_tool_names cannot be an empty list".to_string());
        }
        let included =
            validate_tool_names("include_tool_names", self.include_tool_names.as_deref())?;
        let excluded = validate_tool_names("exclude_tool_names", Some(&self.exclude_tool_names))?;
        if included.iter().any(|name| excluded.contains(name)) {
            return Err("a tool cannot be both included and excluded".to_string());
        }
        Ok(())
    }
}

fn validate_tool_names<'a>(
    field: &str,
    names: Option<&'a [String]>,
) -> Result<HashSet<&'a str>, String> {
    let names = names.unwrap_or_default();
    if names.len() > 128 {
        return Err(format!("{field} must contain at most 128 items"));
    }
    let mut unique = HashSet::with_capacity(names.len());
    for name in names {
        validate_identifier(field, name)?;
        if !unique.insert(name.as_str()) {
            return Err(format!("{field} contains a duplicate tool name: {name}"));
        }
    }
    Ok(unique)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitToolCommand {
    pub invocation_id: String,
    pub claim_token: String,
    pub expected_version: u64,
    pub outcome: LocalAgentToolOutcome,
}

impl CommitToolCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("invocation_id", &self.invocation_id)?;
        validate_identifier("claim_token", &self.claim_token)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        self.outcome.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalAgentToolOutcome {
    Succeeded {
        #[serde(default)]
        output: Value,
    },
    Failed {
        error: String,
        #[serde(default)]
        detail: Value,
    },
    NeedsReview {
        reason: String,
        #[serde(default)]
        detail: Value,
    },
}

impl LocalAgentToolOutcome {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Succeeded { .. } => Ok(()),
            Self::Failed { error, .. } => validate_text("error", error, 8_000),
            Self::NeedsReview { reason, .. } => validate_text("reason", reason, 4_000),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentToolStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    NeedsReview,
}

impl LocalAgentToolStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::NeedsReview => "needs_review",
        }
    }
}

impl std::str::FromStr for LocalAgentToolStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "needs_review" => Ok(Self::NeedsReview),
            _ => Err(format!("unknown Local Agent tool status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentToolInvocationRecord {
    pub invocation_id: String,
    pub run_id: String,
    pub batch_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub side_effecting: bool,
    pub status: LocalAgentToolStatus,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub version: u64,
    pub claim_token: Option<String>,
    pub claim_until_unix_ms: Option<i64>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentToolClaim {
    pub worker_id: String,
    pub claim_token: String,
    pub invocation: LocalAgentToolInvocationRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentToolCommitResult {
    pub invocation: LocalAgentToolInvocationRecord,
    pub run: LocalAgentRunRecord,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_filter_rejects_overlap_and_duplicates() {
        let overlap = ClaimNextToolCommand {
            worker_id: "worker-1".to_string(),
            lease_duration_ms: 10_000,
            include_tool_names: Some(vec!["create_task".to_string()]),
            exclude_tool_names: vec!["create_task".to_string()],
        };
        assert!(overlap.validate().is_err());

        let duplicates = ClaimNextToolCommand {
            worker_id: "worker-1".to_string(),
            lease_duration_ms: 10_000,
            include_tool_names: None,
            exclude_tool_names: vec!["read_file".to_string(), "read_file".to_string()],
        };
        assert!(duplicates.validate().is_err());
    }
}
