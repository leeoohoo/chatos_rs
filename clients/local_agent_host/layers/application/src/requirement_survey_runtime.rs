// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{new_event_id, LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand};
use chatos_local_agent_protocol::{
    CreateRequirementSurveyCommand, HostCommand, HostResult, LocalAgentStepOutcome,
    LocalRequirementSurvey, LocalRequirementSurveyStatus,
    LOCAL_REQUIREMENT_SURVEY_CREATE_TOOL_NAME,
};

pub(super) fn validate_tool_batch(
    outcome: &LocalAgentStepOutcome,
) -> Result<(), LocalAgentRuntimeError> {
    let LocalAgentStepOutcome::WaitForTool { tool_calls, .. } = outcome else {
        return Ok(());
    };
    let survey_calls = tool_calls
        .iter()
        .filter(|call| call.tool_name == LOCAL_REQUIREMENT_SURVEY_CREATE_TOOL_NAME)
        .count();
    if survey_calls > 0 && (survey_calls != 1 || tool_calls.len() != 1) {
        return Err(LocalAgentRuntimeError::InvalidRequest(
            "requirement_survey_create must be the only call in its tool batch".to_string(),
        ));
    }
    Ok(())
}

impl LocalAgentRuntime {
    pub(super) async fn handle_requirement_survey_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::CreateRequirementSurvey(command) => {
                let survey = new_survey(command, self.now()?);
                let survey = self
                    .store
                    .create_requirement_survey(idempotency, &survey)
                    .await?;
                Ok(HostResult::RequirementSurvey { survey })
            }
            HostCommand::ListRequirementSurveys(command) => {
                let surveys = self
                    .store
                    .list_requirement_surveys(
                        &command.owner_user_id,
                        command.project_resource_id.as_deref(),
                        command.status,
                        command.limit,
                    )
                    .await?;
                Ok(HostResult::RequirementSurveys { surveys })
            }
            HostCommand::GetRequirementSurvey(command) => {
                let survey = self
                    .store
                    .get_requirement_survey(&command.owner_user_id, &command.survey_id)
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(command.survey_id.clone()))?;
                Ok(HostResult::RequirementSurvey { survey })
            }
            HostCommand::ResolveRequirementSurvey(command) => {
                let current = self
                    .store
                    .get_requirement_survey(&command.owner_user_id, &command.survey_id)
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(command.survey_id.clone()))?;
                current
                    .validate_answers(&command.answers)
                    .map_err(LocalAgentRuntimeError::InvalidRequest)?;
                let next_version = command.expected_version.checked_add(1).ok_or_else(|| {
                    LocalAgentRuntimeError::InvalidRequest(
                        "requirement survey version overflow".to_string(),
                    )
                })?;
                let now = self.now()?;
                let survey = LocalRequirementSurvey {
                    answers: Some(command.answers),
                    status: LocalRequirementSurveyStatus::Resolved,
                    version: next_version,
                    updated_at_unix_ms: now,
                    resolved_at_unix_ms: Some(now),
                    ..current
                };
                let resolution = self
                    .store
                    .resolve_requirement_survey(
                        idempotency,
                        &survey,
                        command.expected_version,
                        &new_event_id(),
                    )
                    .await?;
                Ok(HostResult::RequirementSurveyResolved {
                    resolution: Box::new(resolution),
                })
            }
            _ => unreachable!("non-survey command routed to survey runtime"),
        }
    }
}

fn new_survey(command: CreateRequirementSurveyCommand, now: i64) -> LocalRequirementSurvey {
    LocalRequirementSurvey {
        survey_id: command.survey_id,
        owner_user_id: command.owner_user_id,
        project_resource_id: command.project_resource_id,
        source_conversation_id: command.source_conversation_id,
        source_run_id: command.source_run_id,
        source_task_id: command.source_task_id,
        title: command.title.trim().to_string(),
        description: command.description.map(|value| value.trim().to_string()),
        questions: command.questions,
        answers: None,
        status: LocalRequirementSurveyStatus::Open,
        version: 1,
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
        resolved_at_unix_ms: None,
    }
}
