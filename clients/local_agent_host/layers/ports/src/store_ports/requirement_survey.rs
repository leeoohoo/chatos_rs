#[async_trait]
pub trait LocalRequirementSurveyStore: Send + Sync {
    async fn get_requirement_plan(
        &self,
        owner_user_id: &str,
        survey_id: &str,
        revision: Option<u64>,
    ) -> Result<Option<chatos_local_agent_protocol::LocalRequirementPlan>, ClientStorageError>;

    async fn save_requirement_plan(
        &self,
        command: &IdempotentCommand,
        save: &chatos_local_agent_protocol::SaveRequirementPlanCommand,
        now_unix_ms: i64,
    ) -> Result<chatos_local_agent_protocol::LocalRequirementPlan, ClientStorageError>;

    async fn confirm_requirement_plan(
        &self,
        command: &IdempotentCommand,
        confirm: &chatos_local_agent_protocol::ConfirmRequirementPlanCommand,
        now_unix_ms: i64,
    ) -> Result<chatos_local_agent_protocol::LocalRequirementPlan, ClientStorageError>;

    async fn create_requirement_survey(
        &self,
        command: &IdempotentCommand,
        survey: &LocalRequirementSurvey,
    ) -> Result<LocalRequirementSurvey, ClientStorageError>;

    async fn list_requirement_surveys(
        &self,
        owner_user_id: &str,
        project_resource_id: Option<&str>,
        status: Option<LocalRequirementSurveyStatus>,
        limit: u32,
    ) -> Result<Vec<LocalRequirementSurvey>, ClientStorageError>;

    async fn get_requirement_survey(
        &self,
        owner_user_id: &str,
        survey_id: &str,
    ) -> Result<Option<LocalRequirementSurvey>, ClientStorageError>;

    async fn resolve_requirement_survey(
        &self,
        command: &IdempotentCommand,
        survey: &LocalRequirementSurvey,
        expected_version: u64,
        run_event_id: &str,
    ) -> Result<LocalRequirementSurveyResolution, ClientStorageError>;
}
