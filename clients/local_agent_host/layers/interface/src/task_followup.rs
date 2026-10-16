// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ResolveTaskFollowupCommand {
    pub owner_user_id: String,
    pub run_id: String,
    pub review_message_id: String,
    pub decisions: Vec<TaskFollowupDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskFollowupDecision {
    pub task_id: String,
    pub action: TaskFollowupAction,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskFollowupAction {
    Continue,
    Cancel,
    Clarify,
}

impl ResolveTaskFollowupCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)?;
        validate_identifier("review_message_id", &self.review_message_id)?;
        if self.decisions.is_empty() || self.decisions.len() > 500 {
            return Err("decisions must contain 1..=500 items".into());
        }
        let mut ids = HashSet::new();
        for decision in &self.decisions {
            validate_identifier("task_id", &decision.task_id)?;
            validate_text("reason", &decision.reason, 1_000)?;
            if !ids.insert(&decision.task_id) {
                return Err("duplicate task decision".into());
            }
        }
        Ok(())
    }
}
