// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text};
use serde::{Deserialize, Serialize};

pub const LOCAL_REQUIREMENT_PLAN_MAX_BYTES: usize = 64 * 1024;
pub const LOCAL_REQUIREMENT_PLAN_SAVE_TOOL_NAME: &str = "requirement_plan_save";

/// Body and provenance are immutable per revision. Confirmation is a separate
/// human action on the exact current revision, never a model-supplied boolean.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalRequirementPlan {
    pub plan_id: String,
    pub revision: u64,
    pub owner_user_id: String,
    pub project_resource_id: String,
    pub source_conversation_id: String,
    pub source_survey_id: String,
    pub source_run_id: String,
    pub title: String,
    pub markdown: String,
    pub created_at_unix_ms: i64,
    pub confirmed_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetRequirementPlanCommand {
    pub owner_user_id: String,
    pub survey_id: String,
    #[serde(default)]
    pub revision: Option<u64>,
}

impl GetRequirementPlanCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("survey_id", &self.survey_id)?;
        if self.revision == Some(0) {
            return Err("plan revision must be positive".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SaveRequirementPlanCommand {
    pub owner_user_id: String,
    pub survey_id: String,
    pub source_run_id: String,
    pub expected_revision: u64,
    pub markdown: String,
}

impl SaveRequirementPlanCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("survey_id", &self.survey_id)?;
        validate_identifier("source_run_id", &self.source_run_id)?;
        validate_text(
            "plan markdown",
            &self.markdown,
            LOCAL_REQUIREMENT_PLAN_MAX_BYTES,
        )?;
        if self.expected_revision >= i64::MAX as u64 {
            return Err("plan revision overflow".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfirmRequirementPlanCommand {
    pub owner_user_id: String,
    pub survey_id: String,
    pub expected_revision: u64,
}

impl ConfirmRequirementPlanCommand {
    pub fn validate(&self) -> Result<(), String> {
        GetRequirementPlanCommand {
            owner_user_id: self.owner_user_id.clone(),
            survey_id: self.survey_id.clone(),
            revision: Some(self.expected_revision),
        }
        .validate()
    }
}
