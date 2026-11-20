#[async_trait]
pub trait LocalAgentToolStore: Send + Sync {
    async fn get_tool_invocation(
        &self,
        invocation_id: &str,
    ) -> Result<Option<LocalAgentToolInvocationRecord>, ClientStorageError>;

    /// Resolves one completed source call inside the active owner's Run.
    /// This is the only lookup used by progressive tool-result reads; callers
    /// cannot cross owner or Run boundaries with a model-supplied call id.
    async fn get_completed_tool_invocation_for_run_call(
        &self,
        owner_user_id: &str,
        run_id: &str,
        call_id: &str,
    ) -> Result<Option<LocalAgentToolInvocationRecord>, ClientStorageError>;

    /// Finds the newest semantically identical completed call inside one owner-scoped Run.
    /// File reads can require invalidation after any later uncertain or successful mutation.
    async fn latest_equivalent_tool_invocation(
        &self,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
        arguments: &Value,
        invalidate_after_side_effect: bool,
    ) -> Result<Option<LocalAgentToolInvocationRecord>, ClientStorageError>;

    async fn successful_tool_invocation_count(
        &self,
        run_id: &str,
        tool_name: &str,
    ) -> Result<u64, ClientStorageError>;

    async fn has_successful_tool_invocation_argument(
        &self,
        run_id: &str,
        tool_name: &str,
        argument_name: &str,
        argument_value: &str,
    ) -> Result<bool, ClientStorageError>;

    async fn successful_tool_invocation_argument_values(
        &self,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
        argument_name: &str,
    ) -> Result<Vec<String>, ClientStorageError>;

    /// Returns successful, non-error calls for one tool inside an owner-scoped Run.
    /// Used to restore durable control-plane facts after a native provider restart.
    async fn successful_tool_invocations_for_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
    ) -> Result<Vec<LocalAgentToolInvocationRecord>, ClientStorageError>;

    /// Trusted, owner/Run-scoped facts only; never model-provided descriptions.
    #[allow(clippy::too_many_arguments)]
    async fn latest_successful_tool_invocation_result(
        &self,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
        argument_name: &str,
        argument_value: &str,
        described_tool_option: Option<&str>,
    ) -> Result<Option<Value>, ClientStorageError>;

    async fn recover_expired_tool_claims(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn renew_tool_claim(
        &self,
        owner_user_id: &str,
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
    ) -> Result<bool, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn claim_next_tool(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
        include_tool_names: Option<&[String]>,
        exclude_tool_names: &[String],
        execution_route: Option<chatos_local_agent_protocol::LocalAgentToolExecutionRoute>,
    ) -> Result<Option<LocalAgentToolClaim>, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn commit_tool(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        outcome: &LocalAgentToolOutcome,
        event_id: &str,
        batch_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentToolCommitResult, ClientStorageError>;

    async fn list_pending_tool_approvals(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalAgentToolInvocationRecord>, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn decide_tool_approval(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        invocation_id: &str,
        expected_version: u64,
        decision: LocalAgentToolApprovalDecision,
        decided_by: &str,
        reason: &str,
        event_id: &str,
        batch_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentToolApprovalResult, ClientStorageError>;
}
