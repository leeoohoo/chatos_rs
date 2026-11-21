#[path = "requirement_survey_projection.rs"]
mod projection;

use chatos_local_agent_protocol::{
    ConfirmRequirementPlanCommand, GetRequirementPlanCommand, GetRequirementSurveyCommand,
    GetRunCommand, HostCommand, HostResult, ListRequirementSurveysCommand, LocalAgentRunRecord,
    LocalAgentRunStatus, LocalRequirementPlan, LocalRequirementSurvey,
    LocalRequirementSurveyStatus, ResolveRequirementSurveyCommand, SaveRequirementPlanCommand,
};
use projection::{
    project_follow_up, project_plan, project_survey, source_team_route, validate_identifier,
    validate_plan, validate_plan_input, validate_plan_run, validate_survey_run,
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
pub(crate) struct RequirementSurveyTeamRoute {
    room_id: String,
    todo_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequirementSurveyFollowUpSnapshot {
    survey_id: String,
    survey_version: u64,
    follow_up: RequirementSurveyFollowUp,
    plan: Option<RequirementSurveyPlan>,
    plan_follow_up: Option<RequirementSurveyFollowUp>,
    source_team_route: Option<RequirementSurveyTeamRoute>,
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
    let (follow_up, source_team_route) = if survey.status == LocalRequirementSurveyStatus::Open {
        (RequirementSurveyFollowUp::awaiting_answers(), None)
    } else {
        let run = get_run(&manager, &owner_user_id, &survey.source_run_id).await?;
        validate_survey_run(&survey, &run)?;
        (project_follow_up(&run), source_team_route(&survey, &run)?)
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
        source_team_route,
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
    let source_run = get_run(&manager, &input.owner_user_id, &current.source_run_id).await?;
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
        .execute(HostCommand::SaveRequirementPlan(
            SaveRequirementPlanCommand {
                owner_user_id: input.owner_user_id.clone(),
                survey_id: input.survey_id.clone(),
                source_run_id: survey.source_run_id.clone(),
                expected_revision: 0,
                markdown: markdown.clone(),
            },
        ))
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
        wrong_profile_input.input = serde_json::json!({"source_conversation_id": "conversation-1"});
        assert!(validate_plan_run(&plan, &wrong_profile_input).is_err());
        let mut wrong_plan = plan;
        wrong_plan.source_survey_id = "survey-forged".into();
        assert!(validate_plan(&survey, &wrong_plan).is_err());
    }

    #[test]
    fn team_todo_source_is_bound_to_its_project_conversation_and_route() {
        let mut survey = survey();
        survey.source_conversation_id = "agent-team-executor-delivery-1".into();
        survey.source_task_id = Some("todo-1".into());
        let mut run = run();
        run.profile_key = "task_execution".into();
        run.owner_entity_type = "conversation_turn".into();
        run.input = serde_json::json!({
            "conversation_id": "agent-team-executor-delivery-1",
            "team_executor_context": {
                "team_id": "team-1",
                "todo_id": "todo-1",
                "project_id": "project-1"
            }
        });

        assert!(validate_survey_run(&survey, &run).is_ok());
        let route = source_team_route(&survey, &run).unwrap().unwrap();
        assert_eq!(route.room_id, "team-1");
        assert_eq!(route.todo_id, "todo-1");

        let mut plan = plan();
        plan.source_conversation_id = survey.source_conversation_id.clone();
        assert!(validate_plan_run(&plan, &run).is_ok());

        run.input["team_executor_context"]["project_id"] = serde_json::json!("project-forged");
        assert!(validate_survey_run(&survey, &run).is_err());
        assert!(source_team_route(&survey, &run).is_err());
        assert!(validate_plan_run(&plan, &run).is_err());
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
