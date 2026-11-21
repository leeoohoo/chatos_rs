use chatos_local_agent_protocol::{
    LocalAgentRunRecord, LocalAgentRunStatus, LocalRequirementPlan, LocalRequirementSurvey,
};
use serde_json::Value;

use super::{
    ProjectRequirementSurvey, ProjectRequirementSurveyQuestion, RequirementSurveyFollowUp,
    RequirementSurveyPlan, RequirementSurveyPlanInput, RequirementSurveyTeamRoute,
};
use crate::host_lifecycle::validate_owner;

pub(super) fn validate_survey_run(
    survey: &LocalRequirementSurvey,
    run: &LocalAgentRunRecord,
) -> Result<(), String> {
    let is_main_chat = survey.source_task_id.is_none()
        && run.profile_key == "main_chat"
        && run.owner_entity_type == "conversation_turn"
        && run.input.get("conversation_id").and_then(Value::as_str)
            == Some(survey.source_conversation_id.as_str());
    let is_task = survey.source_task_id.as_deref().is_some_and(|task_id| {
        run.profile_key == "task_execution"
            && run.owner_entity_type == "task"
            && run.owner_entity_id == task_id
            && run
                .input
                .get("source_conversation_id")
                .and_then(Value::as_str)
                == Some(survey.source_conversation_id.as_str())
    });
    let is_team_todo = survey.source_task_id.as_deref().is_some_and(|todo_id| {
        let team = run
            .input
            .get("team_executor_context")
            .and_then(Value::as_object);
        run.profile_key == "task_execution"
            && run.owner_entity_type == "conversation_turn"
            && run.input.get("conversation_id").and_then(Value::as_str)
                == Some(survey.source_conversation_id.as_str())
            && team
                .and_then(|value| value.get("todo_id"))
                .and_then(Value::as_str)
                == Some(todo_id)
            && team
                .and_then(|value| value.get("project_id"))
                .and_then(Value::as_str)
                == Some(survey.project_resource_id.as_str())
    });
    if run.owner_user_id != survey.owner_user_id
        || run.run_id != survey.source_run_id
        || !(is_main_chat || is_task || is_team_todo)
    {
        return Err("需求调研来源 Run 身份不一致。".into());
    }
    Ok(())
}

pub(super) fn source_team_route(
    survey: &LocalRequirementSurvey,
    run: &LocalAgentRunRecord,
) -> Result<Option<RequirementSurveyTeamRoute>, String> {
    validate_survey_run(survey, run)?;
    if run.profile_key != "task_execution"
        || run.owner_entity_type != "conversation_turn"
        || survey.source_task_id.is_none()
    {
        return Ok(None);
    }
    let Some(team) = run
        .input
        .get("team_executor_context")
        .and_then(Value::as_object)
    else {
        return Err("团队需求调研缺少团队上下文。".into());
    };
    let room_id = team
        .get("team_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "团队需求调研缺少团队路由。".to_owned())?;
    let todo_id = team
        .get("todo_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "团队需求调研缺少任务路由。".to_owned())?;
    validate_identifier(room_id)?;
    validate_identifier(todo_id)?;
    if survey.source_task_id.as_deref() != Some(todo_id) {
        return Err("团队需求调研任务路由不一致。".into());
    }
    Ok(Some(RequirementSurveyTeamRoute {
        room_id: room_id.to_owned(),
        todo_id: todo_id.to_owned(),
    }))
}

pub(super) fn validate_plan(
    survey: &LocalRequirementSurvey,
    plan: &LocalRequirementPlan,
) -> Result<(), String> {
    if plan.owner_user_id != survey.owner_user_id
        || plan.project_resource_id != survey.project_resource_id
        || plan.source_survey_id != survey.survey_id
        || plan.source_conversation_id != survey.source_conversation_id
        || plan.plan_id != format!("requirement-plan:{}", survey.survey_id)
        || plan.revision == 0
        || plan.markdown.trim().is_empty()
    {
        return Err("需求方案身份不一致。".into());
    }
    Ok(())
}

pub(super) fn validate_plan_run(
    plan: &LocalRequirementPlan,
    run: &LocalAgentRunRecord,
) -> Result<(), String> {
    let is_main = run.profile_key == "main_chat" && run.owner_entity_type == "conversation_turn";
    let is_task = run.profile_key == "task_execution" && run.owner_entity_type == "task";
    let is_team_todo = run.profile_key == "task_execution"
        && run.owner_entity_type == "conversation_turn"
        && run.input.get("conversation_id").and_then(Value::as_str)
            == Some(plan.source_conversation_id.as_str())
        && run
            .input
            .get("team_executor_context")
            .and_then(Value::as_object)
            .and_then(|team| team.get("project_id"))
            .and_then(Value::as_str)
            == Some(plan.project_resource_id.as_str());
    let conversation_matches = if is_main {
        run.input.get("conversation_id").and_then(Value::as_str)
            == Some(plan.source_conversation_id.as_str())
    } else if is_task {
        run.input
            .get("source_conversation_id")
            .and_then(Value::as_str)
            == Some(plan.source_conversation_id.as_str())
    } else {
        is_team_todo
    };
    if run.owner_user_id != plan.owner_user_id
        || run.run_id != plan.source_run_id
        || !conversation_matches
    {
        return Err("需求方案来源 Run 身份不一致。".into());
    }
    Ok(())
}

pub(super) fn project_follow_up(run: &LocalAgentRunRecord) -> RequirementSurveyFollowUp {
    match run.status {
        LocalAgentRunStatus::Succeeded => run
            .terminal_outcome
            .as_ref()
            .and_then(model_output)
            .map(RequirementSurveyFollowUp::completed)
            .unwrap_or_else(|| RequirementSurveyFollowUp::status("completed_without_reply")),
        LocalAgentRunStatus::Failed => RequirementSurveyFollowUp::status("failed"),
        LocalAgentRunStatus::Cancelled => RequirementSurveyFollowUp::status("cancelled"),
        LocalAgentRunStatus::WaitingUser => RequirementSurveyFollowUp::status("waiting_for_input"),
        LocalAgentRunStatus::Paused | LocalAgentRunStatus::NeedsReview => {
            RequirementSurveyFollowUp::status("needs_review")
        }
        _ => RequirementSurveyFollowUp::status("processing"),
    }
}

fn model_output(outcome: &Value) -> Option<String> {
    ["content", "answer", "text", "output"]
        .into_iter()
        .find_map(|key| outcome.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

impl RequirementSurveyFollowUp {
    pub(super) fn status(status: &'static str) -> Self {
        Self {
            status,
            markdown: None,
        }
    }

    pub(super) fn awaiting_answers() -> Self {
        Self::status("awaiting_answers")
    }

    pub(super) fn completed(markdown: String) -> Self {
        Self {
            status: "completed",
            markdown: Some(markdown),
        }
    }
}

pub(super) fn project_plan(plan: LocalRequirementPlan) -> RequirementSurveyPlan {
    RequirementSurveyPlan {
        plan_id: plan.plan_id,
        revision: plan.revision,
        project_id: plan.project_resource_id,
        source_conversation_id: plan.source_conversation_id,
        source_survey_id: plan.source_survey_id,
        source_run_id: plan.source_run_id,
        title: plan.title,
        markdown: plan.markdown,
        created_at_unix_ms: plan.created_at_unix_ms,
        confirmed_at_unix_ms: plan.confirmed_at_unix_ms,
    }
}

pub(super) fn validate_plan_input(input: &RequirementSurveyPlanInput) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.project_id)?;
    validate_identifier(&input.survey_id)
}

pub(super) fn project_survey(survey: LocalRequirementSurvey) -> ProjectRequirementSurvey {
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

pub(super) fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        return Err("需求调研标识无效。".into());
    }
    Ok(())
}
