use chatos_local_agent_protocol::{
    ConfirmRequirementPlanCommand, GetRequirementPlanCommand, GetRequirementSurveyCommand,
    GetRunCommand, HostCommand, HostResult, ListRequirementSurveysCommand, LocalAgentRunRecord,
    LocalAgentRunStatus, LocalRequirementPlan, LocalRequirementSurvey,
    LocalRequirementSurveyStatus, ResolveRequirementSurveyCommand, SaveRequirementPlanCommand,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequirementSurveyFollowUp {
    status: &'static str,
    markdown: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequirementSurveyPlan {
    plan_id: String,
    revision: u64,
    project_id: String,
    source_conversation_id: String,
    source_survey_id: String,
    source_run_id: String,
    title: String,
    markdown: String,
    created_at_unix_ms: i64,
    confirmed_at_unix_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequirementSurveyFollowUpSnapshot {
    survey_id: String,
    survey_version: u64,
    follow_up: RequirementSurveyFollowUp,
    plan: Option<RequirementSurveyPlan>,
    plan_follow_up: Option<RequirementSurveyFollowUp>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RequirementSurveyPlanInput {
    owner_user_id: String,
    project_id: String,
    survey_id: String,
    expected_revision: u64,
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

#[tauri::command]
pub(crate) async fn load_requirement_survey_follow_up(
    owner_user_id: String,
    project_id: String,
    survey_id: String,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<RequirementSurveyFollowUpSnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&project_id)?;
    validate_identifier(&survey_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let survey = get_survey(&manager, &owner_user_id, &project_id, &survey_id).await?;
    let follow_up = if survey.status == LocalRequirementSurveyStatus::Open {
        RequirementSurveyFollowUp::awaiting_answers()
    } else {
        let run = get_run(&manager, &owner_user_id, &survey.source_run_id).await?;
        validate_survey_run(&survey, &run)?;
        project_follow_up(&run)
    };
    let plan = get_plan(&manager, &owner_user_id, &survey_id, None).await?;
    let (plan, plan_follow_up) = if let Some(plan) = plan {
        validate_plan(&survey, &plan)?;
        let run = get_run(&manager, &owner_user_id, &plan.source_run_id).await?;
        validate_plan_run(&plan, &run)?;
        let follow_up = project_follow_up(&run);
        (Some(project_plan(plan)), Some(follow_up))
    } else {
        (None, None)
    };
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(RequirementSurveyFollowUpSnapshot {
        survey_id,
        survey_version: survey.version,
        follow_up,
        plan,
        plan_follow_up,
    })
}

#[tauri::command]
pub(crate) async fn confirm_requirement_survey_plan(
    input: RequirementSurveyPlanInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<RequirementSurveyPlan, String> {
    validate_plan_input(&input)?;
    if input.expected_revision == 0 {
        return Err("需求方案版本无效。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let survey = get_survey(
        &manager,
        &input.owner_user_id,
        &input.project_id,
        &input.survey_id,
    )
    .await?;
    if survey.status != LocalRequirementSurveyStatus::Resolved {
        return Err("需求调研尚未提交。".into());
    }
    let current = get_plan(&manager, &input.owner_user_id, &input.survey_id, None)
        .await?
        .ok_or_else(|| "需求方案尚未生成。".to_owned())?;
    validate_plan(&survey, &current)?;
    if current.revision != input.expected_revision || current.confirmed_at_unix_ms.is_some() {
        return Err("需求方案已经变化，请刷新后重试。".into());
    }
    let source_run = get_run(
        &manager,
        &input.owner_user_id,
        &current.source_run_id,
    )
    .await?;
    validate_plan_run(&current, &source_run)?;
    if source_run.status != LocalAgentRunStatus::Succeeded {
        return Err("需求方案尚未生成完成。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::ConfirmRequirementPlan(
            ConfirmRequirementPlanCommand {
                owner_user_id: input.owner_user_id.clone(),
                survey_id: input.survey_id.clone(),
                expected_revision: input.expected_revision,
            },
        ))
        .await?;
    let HostResult::RequirementPlan {
        plan: Some(confirmed),
    } = result
    else {
        return Err("Local Agent Host 返回了错误的需求方案确认结果。".into());
    };
    validate_plan(&survey, &confirmed)?;
    if confirmed.plan_id != current.plan_id
        || confirmed.revision != current.revision
        || confirmed.markdown != current.markdown
        || confirmed.title != current.title
        || confirmed.source_run_id != current.source_run_id
        || confirmed.created_at_unix_ms != current.created_at_unix_ms
        || confirmed.confirmed_at_unix_ms.is_none()
    {
        return Err("Local Agent Host 返回了不匹配的需求方案确认结果。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(project_plan(confirmed))
}

#[tauri::command]
pub(crate) async fn adopt_requirement_survey_plan(
    input: RequirementSurveyPlanInput,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, HostManager>,
) -> Result<RequirementSurveyPlan, String> {
    validate_plan_input(&input)?;
    if input.expected_revision != 0 {
        return Err("历史回复只能创建首个需求方案版本。".into());
    }
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let survey = get_survey(
        &manager,
        &input.owner_user_id,
        &input.project_id,
        &input.survey_id,
    )
    .await?;
    if survey.status != LocalRequirementSurveyStatus::Resolved {
        return Err("需求调研尚未提交。".into());
    }
    if get_plan(&manager, &input.owner_user_id, &input.survey_id, None)
        .await?
        .is_some()
    {
        return Err("需求方案已经生成，请刷新后查看当前版本。".into());
    }
    let source_run = get_run(&manager, &input.owner_user_id, &survey.source_run_id).await?;
    validate_survey_run(&survey, &source_run)?;
    let follow_up = project_follow_up(&source_run);
    let markdown = follow_up
        .markdown
        .filter(|_| follow_up.status == "completed")
        .ok_or_else(|| "原 Run 尚未生成可保存的方案回复。".to_owned())?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::SaveRequirementPlan(SaveRequirementPlanCommand {
            owner_user_id: input.owner_user_id.clone(),
            survey_id: input.survey_id.clone(),
            source_run_id: survey.source_run_id.clone(),
            expected_revision: 0,
            markdown: markdown.clone(),
        }))
        .await?;
    let HostResult::RequirementPlan { plan: Some(saved) } = result else {
        return Err("Local Agent Host 返回了错误的需求方案保存结果。".into());
    };
    validate_plan(&survey, &saved)?;
    if saved.revision != 1
        || saved.source_run_id != survey.source_run_id
        || saved.markdown != markdown.trim()
        || saved.confirmed_at_unix_ms.is_some()
    {
        return Err("Local Agent Host 返回了不匹配的需求方案保存结果。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let result = manager
        .execute(HostCommand::ConfirmRequirementPlan(
            ConfirmRequirementPlanCommand {
                owner_user_id: input.owner_user_id.clone(),
                survey_id: input.survey_id.clone(),
                expected_revision: saved.revision,
            },
        ))
        .await?;
    let HostResult::RequirementPlan {
        plan: Some(confirmed),
    } = result
    else {
        return Err("Local Agent Host 返回了错误的需求方案确认结果。".into());
    };
    validate_plan(&survey, &confirmed)?;
    if confirmed.plan_id != saved.plan_id
        || confirmed.revision != saved.revision
        || confirmed.markdown != saved.markdown
        || confirmed.confirmed_at_unix_ms.is_none()
    {
        return Err("Local Agent Host 返回了不匹配的需求方案确认结果。".into());
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(project_plan(confirmed))
}

async fn get_survey(
    manager: &HostManager,
    owner_user_id: &str,
    project_id: &str,
    survey_id: &str,
) -> Result<LocalRequirementSurvey, String> {
    let result = manager
        .execute(HostCommand::GetRequirementSurvey(
            GetRequirementSurveyCommand {
                owner_user_id: owner_user_id.to_owned(),
                survey_id: survey_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::RequirementSurvey { survey } = result else {
        return Err("Local Agent Host 返回了错误的需求调研。".into());
    };
    if survey.owner_user_id != owner_user_id
        || survey.project_resource_id != project_id
        || survey.survey_id != survey_id
    {
        return Err("Local Agent Host 返回了不匹配的需求调研。".into());
    }
    Ok(survey)
}

async fn get_run(
    manager: &HostManager,
    owner_user_id: &str,
    run_id: &str,
) -> Result<LocalAgentRunRecord, String> {
    let result = manager
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: run_id.to_owned(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("Local Agent Host 返回了错误的 Run。".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != run_id {
        return Err("Local Agent Host 返回了不匹配的 Run。".into());
    }
    Ok(run)
}

async fn get_plan(
    manager: &HostManager,
    owner_user_id: &str,
    survey_id: &str,
    revision: Option<u64>,
) -> Result<Option<LocalRequirementPlan>, String> {
    let result = manager
        .execute(HostCommand::GetRequirementPlan(GetRequirementPlanCommand {
            owner_user_id: owner_user_id.to_owned(),
            survey_id: survey_id.to_owned(),
            revision,
        }))
        .await?;
    let HostResult::RequirementPlan { plan } = result else {
        return Err("Local Agent Host 返回了错误的需求方案。".into());
    };
    Ok(plan)
}

fn validate_survey_run(
    survey: &LocalRequirementSurvey,
    run: &LocalAgentRunRecord,
) -> Result<(), String> {
    let conversation_field = if survey.source_task_id.is_some() {
        "source_conversation_id"
    } else {
        "conversation_id"
    };
    let valid_owner = run.owner_user_id == survey.owner_user_id
        && run.run_id == survey.source_run_id
        && run.input.get(conversation_field).and_then(Value::as_str)
            == Some(survey.source_conversation_id.as_str());
    let valid_entity = match &survey.source_task_id {
        Some(task_id) => {
            run.profile_key == "task_execution"
                && run.owner_entity_type == "task"
                && run.owner_entity_id == *task_id
        }
        None => run.profile_key == "main_chat" && run.owner_entity_type == "conversation_turn",
    };
    if !valid_owner || !valid_entity {
        return Err("需求调研来源 Run 身份不一致。".into());
    }
    Ok(())
}

fn validate_plan(
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

fn validate_plan_run(plan: &LocalRequirementPlan, run: &LocalAgentRunRecord) -> Result<(), String> {
    let is_main = run.profile_key == "main_chat" && run.owner_entity_type == "conversation_turn";
    let is_task = run.profile_key == "task_execution" && run.owner_entity_type == "task";
    let conversation_matches = if is_main {
        run.input.get("conversation_id").and_then(Value::as_str)
            == Some(plan.source_conversation_id.as_str())
    } else if is_task {
        run
            .input
            .get("source_conversation_id")
            .and_then(Value::as_str)
            == Some(plan.source_conversation_id.as_str())
    } else {
        false
    };
    if run.owner_user_id != plan.owner_user_id
        || run.run_id != plan.source_run_id
        || !conversation_matches
    {
        return Err("需求方案来源 Run 身份不一致。".into());
    }
    Ok(())
}

fn project_follow_up(run: &LocalAgentRunRecord) -> RequirementSurveyFollowUp {
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
    fn status(status: &'static str) -> Self {
        Self {
            status,
            markdown: None,
        }
    }

    fn awaiting_answers() -> Self {
        Self::status("awaiting_answers")
    }

    fn completed(markdown: String) -> Self {
        Self {
            status: "completed",
            markdown: Some(markdown),
        }
    }
}

fn project_plan(plan: LocalRequirementPlan) -> RequirementSurveyPlan {
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

fn validate_plan_input(input: &RequirementSurveyPlanInput) -> Result<(), String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.project_id)?;
    validate_identifier(&input.survey_id)
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
        let projected = project_survey(survey());

        assert_eq!(projected.project_id, "project-1");
        assert_eq!(projected.status, "open");
        assert_eq!(projected.questions[0].response_kind, "multiple_choice");
        assert!(validate_identifier(" project-1").is_err());
    }

    #[test]
    fn follow_up_projects_only_the_final_user_facing_reply() {
        let mut run = run();
        run.status = LocalAgentRunStatus::Succeeded;
        run.terminal_outcome = Some(serde_json::json!({
            "content": "  # 实施方案  ",
            "trace": {"secret": "must-not-project"},
            "tool_results": ["private"]
        }));
        let projected = project_follow_up(&run);
        let value = serde_json::to_value(projected).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["markdown"], "# 实施方案");
        assert!(value.get("trace").is_none());

        run.terminal_outcome = Some(serde_json::json!({"trace": "only"}));
        assert_eq!(project_follow_up(&run).status, "completed_without_reply");
    }

    #[test]
    fn source_run_and_plan_must_match_every_survey_identity() {
        let survey = survey();
        let run = run();
        assert!(validate_survey_run(&survey, &run).is_ok());
        let mut wrong_run = run.clone();
        wrong_run.input = serde_json::json!({"conversation_id": "conversation-forged"});
        assert!(validate_survey_run(&survey, &wrong_run).is_err());

        let plan = plan();
        assert!(validate_plan(&survey, &plan).is_ok());
        assert!(validate_plan_run(&plan, &run).is_ok());
        let mut wrong_profile_input = run.clone();
        wrong_profile_input.input =
            serde_json::json!({"source_conversation_id": "conversation-1"});
        assert!(validate_plan_run(&plan, &wrong_profile_input).is_err());
        let mut wrong_plan = plan;
        wrong_plan.source_survey_id = "survey-forged".into();
        assert!(validate_plan(&survey, &wrong_plan).is_err());
    }

    fn survey() -> LocalRequirementSurvey {
        LocalRequirementSurvey {
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
        }
    }

    fn run() -> LocalAgentRunRecord {
        LocalAgentRunRecord {
            run_id: "run-1".into(),
            owner_user_id: "alice".into(),
            owner_entity_type: "conversation_turn".into(),
            owner_entity_id: "turn-1".into(),
            profile_key: "main_chat".into(),
            model_config_ref: "model-1".into(),
            model_config_revision: "1".into(),
            capability_policy_revision: "1".into(),
            input: serde_json::json!({"conversation_id": "conversation-1"}),
            status: LocalAgentRunStatus::ModelRunning,
            iteration: 1,
            model_attempt: 1,
            max_iterations: 8,
            version: 1,
            claim_token: None,
            claim_until_unix_ms: None,
            next_attempt_at_unix_ms: None,
            pending_tool_batch: None,
            checkpoint: Value::Null,
            continuation_input: None,
            terminal_outcome: None,
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
        }
    }

    fn plan() -> LocalRequirementPlan {
        LocalRequirementPlan {
            plan_id: "requirement-plan:survey-1".into(),
            revision: 1,
            owner_user_id: "alice".into(),
            project_resource_id: "project-1".into(),
            source_conversation_id: "conversation-1".into(),
            source_survey_id: "survey-1".into(),
            source_run_id: "run-1".into(),
            title: "实施方案".into(),
            markdown: "# 实施方案".into(),
            created_at_unix_ms: 30,
            confirmed_at_unix_ms: None,
        }
    }
}
