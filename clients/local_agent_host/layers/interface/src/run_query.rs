// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, LocalAgentRunRecord};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentRunListScope {
    Active,
    Terminal,
    #[default]
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListRunsCommand {
    pub owner_user_id: String,
    #[serde(default)]
    pub scope: LocalAgentRunListScope,
    pub before_updated_at_unix_ms: Option<i64>,
    pub before_run_id: Option<String>,
    pub limit: u32,
}

impl ListRunsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        if !(1..=100).contains(&self.limit) {
            return Err("limit must be between 1 and 100".to_string());
        }
        match (
            self.before_updated_at_unix_ms,
            self.before_run_id.as_deref(),
        ) {
            (None, None) => Ok(()),
            (Some(timestamp), Some(run_id)) if timestamp >= 0 => {
                validate_identifier("before_run_id", run_id)
            }
            _ => Err(
                "before_updated_at_unix_ms and before_run_id must be supplied together".to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentRunPage {
    pub runs: Vec<LocalAgentRunRecord>,
    pub next_before_updated_at_unix_ms: Option<i64>,
    pub next_before_run_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_cursor_is_paired_and_bounded() {
        let valid = ListRunsCommand {
            owner_user_id: "user-1".to_string(),
            scope: LocalAgentRunListScope::Active,
            before_updated_at_unix_ms: Some(1_000),
            before_run_id: Some("run-1".to_string()),
            limit: 25,
        };
        assert!(valid.validate().is_ok());
        assert!(ListRunsCommand {
            before_run_id: None,
            ..valid.clone()
        }
        .validate()
        .is_err());
        assert!(ListRunsCommand {
            limit: 101,
            ..valid
        }
        .validate()
        .is_err());
    }
}
