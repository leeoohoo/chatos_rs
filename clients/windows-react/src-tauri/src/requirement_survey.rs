use chatos_local_agent_protocol::{
    GetRequirementSurveyCommand, HostCommand, HostResult, ListRequirementSurveysCommand,
    LocalRequirementSurvey, LocalRequirementSurveyStatus, ResolveRequirementSurveyCommand,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use tauri::State;

use crate::{
    authentication::AuthenticationManager,
    host_lifecycle::{validate_owner, HostManager},
};

const SURVEY_LIMIT: u32 = 200;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRequirementSurveyQuestion {
    question_id: String,
    prompt: String,
    response_kind: String,
    required: bool,
    options: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectRequirementSurvey {
    survey_id: String,
    project_id: String,
    source_conversation_id: String,
    source_run_id: String,
    source_task_id: Option<String>,
    title: String,
    description: Option<String>,
    questions: Vec<ProjectRequirementSurveyQuestion>,
    answers: Option<BTreeMap<String, Value>>,
    status: String,
    version: u64,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
    resolved_at_unix_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResolveProjectRequirementSurveyInput {
    owner_user_id: String,
    project_id: String,
    survey_id: String,
    expected_version: u64,
    answers: BTreeMap<String, Value>,
}

#[tauri::command]
pub(crate) async fn list_project_requirement_surveys(
    owner_user_id: String,
    project_id: String,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<Vec<ProjectRequirementSurvey>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&project_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let result = manager
        .execute(HostCommand::ListRequirementSurveys(
            ListRequirementSurveysCommand {
                owner_user_id: owner_user_id.clone(),
                project_resource_id: Some(project_id.clone()),
                status: None,
                limit: SURVEY_LIMIT,
            },
        ))
        .await?;
    let HostResult::RequirementSurveys { surveys } = result else {
        return Err("Local Agent Host 返回了错误的需求调研列表。".into());
    };
    if surveys.iter().any(|survey| {
        survey.owner_user_id != owner_user_id || survey.project_resource_id != project_id
    }) {
        return Err("Local Agent Host 返回了不匹配的需求调研。".into());
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(surveys.into_iter().map(project_survey).collect())
}

#[tauri::command]
pub(crate) async fn resolve_project_requirement_survey(
    input: ResolveProjectRequirementSurveyInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<ProjectRequirementSurvey, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.project_id)?;
    validate_identifier(&input.survey_id)?;
    if input.expected_version == 0 {
        return Err("需求调研版本无效。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let current = manager
        .execute(HostCommand::GetRequirementSurvey(
            GetRequirementSurveyCommand {
                owner_user_id: input.owner_user_id.clone(),
                survey_id: input.survey_id.clone(),
            },
        ))
        .await?;
    let HostResult::RequirementSurvey { survey } = current else {
        return Err("Local Agent Host 返回了错误的需求调研。".into());
    };
    if survey.owner_user_id != input.owner_user_id
        || survey.project_resource_id != input.project_id
        || survey.survey_id != input.survey_id
        || survey.status != LocalRequirementSurveyStatus::Open
        || survey.version != input.expected_version
    {
        return Err("需求调研已经变化，请刷新后重试。".into());
    }
    survey
        .validate_answers(&input.answers)
        .map_err(|_| "需求调研回答无效，请检查必填项和选项。".to_owned())?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::ResolveRequirementSurvey(
            ResolveRequirementSurveyCommand {
                owner_user_id: input.owner_user_id.clone(),
                survey_id: input.survey_id.clone(),
                expected_version: input.expected_version,
                answers: input.answers,
            },
        ))
        .await?;
    let HostResult::RequirementSurveyResolved { resolution } = result else {
        return Err("Local Agent Host 返回了错误的需求调研提交结果。".into());
    };
    let resolved = resolution.survey;
    if resolved.owner_user_id != input.owner_user_id
        || resolved.project_resource_id != input.project_id
        || resolved.survey_id != input.survey_id
        || resolved.status != LocalRequirementSurveyStatus::Resolved
        || resolved.version <= input.expected_version
    {
        return Err("Local Agent Host 返回了不匹配的需求调研提交结果。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(project_survey(resolved))
}

fn project_survey(survey: LocalRequirementSurvey) -> ProjectRequirementSurvey {
    ProjectRequirementSurvey {
        survey_id: survey.survey_id,
        project_id: survey.project_resource_id,
        source_conversation_id: survey.source_conversation_id,
        source_run_id: survey.source_run_id,
        source_task_id: survey.source_task_id,
        title: survey.title,
        description: survey.description,
        questions: survey
            .questions
            .into_iter()
            .map(|question| ProjectRequirementSurveyQuestion {
                question_id: question.question_id,
                prompt: question.prompt,
                response_kind: match question.response_kind {
                    chatos_local_agent_protocol::LocalRequirementSurveyResponseKind::Text => {
                        "text"
                    }
                    chatos_local_agent_protocol::LocalRequirementSurveyResponseKind::SingleChoice => {
                        "single_choice"
                    }
                    chatos_local_agent_protocol::LocalRequirementSurveyResponseKind::MultipleChoice => {
                        "multiple_choice"
                    }
                    chatos_local_agent_protocol::LocalRequirementSurveyResponseKind::Boolean => {
                        "boolean"
                    }
                }
                .into(),
                required: question.required,
                options: question.options,
            })
            .collect(),
        answers: survey.answers,
        status: survey.status.as_str().into(),
        version: survey.version,
        created_at_unix_ms: survey.created_at_unix_ms,
        updated_at_unix_ms: survey.updated_at_unix_ms,
        resolved_at_unix_ms: survey.resolved_at_unix_ms,
    }
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        return Err("需求调研标识无效。".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_protocol::{
        LocalRequirementSurveyQuestion, LocalRequirementSurveyResponseKind,
    };

    #[test]
    fn project_projection_keeps_public_survey_fields_and_camel_case_values() {
        let projected = project_survey(LocalRequirementSurvey {
            survey_id: "survey-1".into(),
            owner_user_id: "alice".into(),
            project_resource_id: "project-1".into(),
            source_conversation_id: "conversation-1".into(),
            source_run_id: "run-1".into(),
            source_task_id: None,
            title: "项目目标".into(),
            description: Some("请补充目标".into()),
            questions: vec![LocalRequirementSurveyQuestion {
                question_id: "goal".into(),
                prompt: "需要交付什么？".into(),
                response_kind: LocalRequirementSurveyResponseKind::MultipleChoice,
                required: true,
                options: vec!["Alpha".into(), "Beta".into()],
            }],
            answers: None,
            status: LocalRequirementSurveyStatus::Open,
            version: 1,
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
            resolved_at_unix_ms: None,
        });

        assert_eq!(projected.project_id, "project-1");
        assert_eq!(projected.status, "open");
        assert_eq!(projected.questions[0].response_kind, "multiple_choice");
        assert!(validate_identifier(" project-1").is_err());
    }
}
