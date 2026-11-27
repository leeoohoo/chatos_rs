fn is_team_executor_run(run: &LocalAgentRunRecord) -> bool {
    run.profile_key == TASK_EXECUTION_PROFILE_KEY
        && run.input.get("team_executor_context").is_some()
        && run
            .input
            .pointer("/agent_execution_context/schema_version")
            .and_then(Value::as_u64)
            == Some(2)
}

#[async_trait]
impl LocalHookContextProvider for LocalHookCoordinator {
    async fn prefixed_context(&self, claim: &LocalAgentRunClaim) -> Result<Vec<Value>, String> {
        self.contexts_for_claim(claim).await
    }

    async fn main_chat_task_route_state(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<LocalMainChatTaskRouteState, String> {
        if claim.run.profile_key != MAIN_CHAT_PROFILE_KEY
            || claim.run.owner_entity_type != "conversation_turn"
        {
            return Ok(LocalMainChatTaskRouteState::default());
        }
        let required = self
            .runtime
            .active_plugin_hook_bundles_for_host_worker(&claim.run.owner_user_id)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .any(|bundle| {
                bundle.spec.enabled
                    && bundle
                        .spec
                        .task_policy
                        .is_some_and(|policy| policy.require_main_chat_task_route)
            });
        let decided = required
            && self
                .runtime
                .successful_tool_invocation_count_for_host_worker(
                    &claim.run.run_id,
                    DECIDE_TASK_ROUTE_TOOL,
                )
                .await
                .map_err(|error| error.to_string())?
                > 0;
        Ok(LocalMainChatTaskRouteState { required, decided })
    }
}

#[async_trait]
impl LocalToolHookGate for LocalHookCoordinator {
    async fn gate_tool_outcome(
        &self,
        claim: &LocalAgentRunClaim,
        outcome: LocalAgentStepOutcome,
        tool_definitions: &[Value],
    ) -> Result<LocalAgentStepOutcome, String> {
        LocalHookCoordinator::gate_tool_outcome(self, claim, outcome, tool_definitions).await
    }
}

#[derive(Clone)]
struct HookEvent {
    spec: LocalHookEventSpec,
    event_type: String,
    matcher_value: String,
    stdin_json: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveHookSession {
    sequence: u64,
    source: String,
    compact_marker: Option<String>,
    last_active_at: Instant,
}

impl HookEvent {
    fn new(
        run: &LocalAgentRunRecord,
        event_type: &str,
        subject_id: &str,
        matcher_value: &str,
        payload: Value,
    ) -> Result<Self, String> {
        Self::new_for_owner(
            &run.owner_user_id,
            event_type,
            subject_id,
            matcher_value,
            payload,
        )
    }

    fn new_for_owner(
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
        matcher_value: &str,
        payload: Value,
    ) -> Result<Self, String> {
        Self::new_for_owner_with_sequence(
            owner_user_id,
            event_type,
            subject_id,
            1,
            matcher_value,
            payload,
        )
    }

    fn new_for_owner_with_sequence(
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
        sequence: u64,
        matcher_value: &str,
        payload: Value,
    ) -> Result<Self, String> {
        let stdin_json = serde_json::to_vec(&payload)
            .map_err(|error| format!("encode Hook event payload failed: {error}"))?;
        let payload_sha256 = sha256(&stdin_json);
        let payload_json = String::from_utf8(stdin_json.clone())
            .map_err(|error| format!("Hook event payload was not UTF-8 JSON: {error}"))?;
        Ok(Self {
            spec: LocalHookEventSpec {
                event_id: stable_id(
                    "hook-event",
                    &[owner_user_id, event_type, subject_id, &sequence.to_string()],
                ),
                owner_user_id: owner_user_id.to_string(),
                event_type: event_type.to_string(),
                subject_id: subject_id.to_string(),
                sequence,
                payload_sha256,
                payload_json,
                matcher_value: matcher_value.to_string(),
                active_bundle_ids: Vec::new(),
            },
            event_type: event_type.to_string(),
            matcher_value: matcher_value.to_string(),
            stdin_json,
        })
    }

    fn from_record(record: &LocalHookEventRecord) -> Result<Self, String> {
        if record.spec.payload_json.is_empty() {
            return Err(format!(
                "Hook event has no recoverable payload: {}",
                record.spec.event_id
            ));
        }
        let stdin_json = record.spec.payload_json.as_bytes().to_vec();
        if sha256(&stdin_json) != record.spec.payload_sha256 {
            return Err(format!(
                "Hook event recovery payload hash changed: {}",
                record.spec.event_id
            ));
        }
        Ok(Self {
            spec: record.spec.clone(),
            event_type: record.spec.event_type.clone(),
            matcher_value: record.spec.matcher_value.clone(),
            stdin_json,
        })
    }
}

#[derive(Debug, Clone)]
struct InvocationOutcome {
    bundle_id: String,
    plugin_id: String,
    hook_id: String,
    status: String,
    additional_context: Option<String>,
    block_reason: Option<String>,
    permission_decision: Option<String>,
    decision_reason: Option<String>,
    updated_input: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedEventOutcome {
    event_type: String,
    blocked: bool,
    #[serde(default)]
    failed: bool,
    block_reason: Option<String>,
    additional_context: Option<String>,
    #[serde(default)]
    permission_decision: Option<String>,
    #[serde(default)]
    decision_reason: Option<String>,
    #[serde(default)]
    updated_input: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct PersistedInvocationOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    additional_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    system_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permission_decision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decision_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    updated_input: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct HookOutput {
    #[serde(default = "default_true")]
    r#continue: bool,
    #[serde(rename = "stopReason")]
    stop_reason: Option<String>,
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: Option<HookSpecificOutput>,
    decision: Option<PermissionHookDecision>,
}

#[derive(Debug, Deserialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: String,
    #[serde(rename = "additionalContext")]
    additional_context: Option<String>,
    #[serde(rename = "permissionDecision")]
    permission_decision: Option<String>,
    #[serde(rename = "permissionDecisionReason")]
    permission_decision_reason: Option<String>,
    #[serde(rename = "updatedInput")]
    updated_input: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PermissionHookDecision {
    behavior: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(rename = "updatedInput")]
    updated_input: Option<Value>,
}

fn default_true() -> bool {
    true
}
