// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, validate_text, LocalAgentRunRecord, LOCAL_AGENT_MAX_INPUT_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub const LOCAL_REQUIREMENT_SURVEY_MAX_QUESTIONS: usize = 50;
pub const LOCAL_REQUIREMENT_SURVEY_MAX_LIST_LIMIT: u32 = 200;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalRequirementSurveyStatus {
    Open,
    Resolved,
}

impl LocalRequirementSurveyStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalRequirementSurveyResponseKind {
    Text,
    SingleChoice,
    MultipleChoice,
    Boolean,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalRequirementSurveyQuestion {
    pub question_id: String,
    pub prompt: String,
    pub response_kind: LocalRequirementSurveyResponseKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<String>,
}

impl LocalRequirementSurveyQuestion {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("question_id", &self.question_id)?;
        validate_text("question prompt", &self.prompt, 4_000)?;
        if self.options.len() > 50 {
            return Err("question options cannot contain more than 50 items".to_string());
        }
        let mut unique = HashSet::new();
        for option in &self.options {
            validate_text("question option", option, 1_000)?;
            if !unique.insert(option.trim()) {
                return Err(format!("question option is duplicated: {}", option.trim()));
            }
        }
        match self.response_kind {
            LocalRequirementSurveyResponseKind::SingleChoice
            | LocalRequirementSurveyResponseKind::MultipleChoice
                if self.options.len() < 2 =>
            {
                Err("choice questions require at least two options".to_string())
            }
            LocalRequirementSurveyResponseKind::Text
            | LocalRequirementSurveyResponseKind::Boolean
                if !self.options.is_empty() =>
            {
                Err("text and boolean questions cannot define options".to_string())
            }
            _ => Ok(()),
        }
    }

    pub fn validate_answer(&self, answer: &Value) -> Result<(), String> {
        let invalid = || format!("invalid answer for question {}", self.question_id);
        match self.response_kind {
            LocalRequirementSurveyResponseKind::Text => {
                let value = answer.as_str().ok_or_else(invalid)?;
                validate_text("survey text answer", value, 16_000)
            }
            LocalRequirementSurveyResponseKind::Boolean => {
                answer.as_bool().ok_or_else(invalid).map(|_| ())
            }
            LocalRequirementSurveyResponseKind::SingleChoice => {
                let value = answer.as_str().ok_or_else(invalid)?;
                if self.options.iter().any(|option| option == value) {
                    Ok(())
                } else {
                    Err(invalid())
                }
            }
            LocalRequirementSurveyResponseKind::MultipleChoice => {
                let values = answer.as_array().ok_or_else(invalid)?;
                if values.is_empty() {
                    return Err(invalid());
                }
                let mut unique = HashSet::new();
                for value in values {
                    let value = value.as_str().ok_or_else(invalid)?;
                    if !self.options.iter().any(|option| option == value) || !unique.insert(value) {
                        return Err(invalid());
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateRequirementSurveyCommand {
    pub survey_id: String,
    pub owner_user_id: String,
    pub project_resource_id: String,
    pub source_conversation_id: String,
    pub source_run_id: String,
    #[serde(default)]
    pub source_task_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub questions: Vec<LocalRequirementSurveyQuestion>,
}

impl CreateRequirementSurveyCommand {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("survey_id", self.survey_id.as_str()),
            ("owner_user_id", self.owner_user_id.as_str()),
            ("project_resource_id", self.project_resource_id.as_str()),
            (
                "source_conversation_id",
                self.source_conversation_id.as_str(),
            ),
            ("source_run_id", self.source_run_id.as_str()),
        ] {
            validate_identifier(name, value)?;
        }
        if let Some(task_id) = self.source_task_id.as_deref() {
            validate_identifier("source_task_id", task_id)?;
        }
        validate_text("title", &self.title, 1_000)?;
        if let Some(description) = self.description.as_deref() {
            validate_text("description", description, 16_000)?;
        }
        if self.questions.is_empty()
            || self.questions.len() > LOCAL_REQUIREMENT_SURVEY_MAX_QUESTIONS
        {
            return Err(format!(
                "questions must contain 1..={LOCAL_REQUIREMENT_SURVEY_MAX_QUESTIONS} items"
            ));
        }
        let mut ids = HashSet::new();
        for question in &self.questions {
            question.validate()?;
            if !ids.insert(question.question_id.as_str()) {
                return Err(format!(
                    "question_id is duplicated: {}",
                    question.question_id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListRequirementSurveysCommand {
    pub owner_user_id: String,
    #[serde(default)]
    pub project_resource_id: Option<String>,
    #[serde(default)]
    pub status: Option<LocalRequirementSurveyStatus>,
    pub limit: u32,
}

impl ListRequirementSurveysCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        if let Some(project_id) = self.project_resource_id.as_deref() {
            validate_identifier("project_resource_id", project_id)?;
        }
        if self.limit == 0 || self.limit > LOCAL_REQUIREMENT_SURVEY_MAX_LIST_LIMIT {
            return Err(format!(
                "limit must be between 1 and {LOCAL_REQUIREMENT_SURVEY_MAX_LIST_LIMIT}"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetRequirementSurveyCommand {
    pub owner_user_id: String,
    pub survey_id: String,
}

impl GetRequirementSurveyCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("survey_id", &self.survey_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolveRequirementSurveyCommand {
    pub owner_user_id: String,
    pub survey_id: String,
    pub expected_version: u64,
    pub answers: BTreeMap<String, Value>,
}

impl ResolveRequirementSurveyCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("survey_id", &self.survey_id)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        let size = serde_json::to_vec(&self.answers)
            .map_err(|error| format!("answers are not serializable: {error}"))?
            .len();
        if size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "answers exceed the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
            ));
        }
        for id in self.answers.keys() {
            validate_identifier("answer question_id", id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalRequirementSurvey {
    pub survey_id: String,
    pub owner_user_id: String,
    pub project_resource_id: String,
    pub source_conversation_id: String,
    pub source_run_id: String,
    pub source_task_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub questions: Vec<LocalRequirementSurveyQuestion>,
    pub answers: Option<BTreeMap<String, Value>>,
    pub status: LocalRequirementSurveyStatus,
    pub version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub resolved_at_unix_ms: Option<i64>,
}

impl LocalRequirementSurvey {
    pub fn validate_answers(&self, answers: &BTreeMap<String, Value>) -> Result<(), String> {
        for id in answers.keys() {
            if !self
                .questions
                .iter()
                .any(|question| &question.question_id == id)
            {
                return Err(format!("answer references unknown question_id: {id}"));
            }
        }
        for question in &self.questions {
            match answers.get(&question.question_id) {
                Some(answer) => question.validate_answer(answer)?,
                None if question.required => {
                    return Err(format!(
                        "answer is required for question {}",
                        question.question_id
                    ))
                }
                None => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalRequirementSurveyResolution {
    pub survey: LocalRequirementSurvey,
    pub resumed_run: LocalAgentRunRecord,
}
