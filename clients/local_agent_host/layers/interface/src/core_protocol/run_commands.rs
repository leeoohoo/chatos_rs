#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateRunCommand {
    pub run_id: String,
    pub owner_user_id: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    #[serde(default)]
    pub input: Value,
    pub max_iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetRunCommand {
    pub owner_user_id: String,
    pub run_id: String,
}

impl GetRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)
    }
}

impl CreateRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("run_id", &self.run_id)?;
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("owner_entity_type", &self.owner_entity_type)?;
        validate_identifier("owner_entity_id", &self.owner_entity_id)?;
        validate_identifier("profile_key", &self.profile_key)?;
        validate_identifier("model_config_ref", &self.model_config_ref)?;
        validate_identifier("model_config_revision", &self.model_config_revision)?;
        validate_identifier(
            "capability_policy_revision",
            &self.capability_policy_revision,
        )?;
        if self.max_iterations == 0 {
            return Err("max_iterations must be greater than zero".to_string());
        }
        let input_size = serde_json::to_vec(&self.input)
            .map_err(|error| format!("input is not serializable: {error}"))?
            .len();
        if input_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "input exceeds the {} byte limit",
                LOCAL_AGENT_MAX_INPUT_BYTES
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimNextRunCommand {
    pub owner_user_id: String,
    pub worker_id: String,
    pub lease_duration_ms: u64,
}

impl ClaimNextRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("worker_id", &self.worker_id)?;
        if !(1_000..=300_000).contains(&self.lease_duration_ms) {
            return Err("lease_duration_ms must be between 1000 and 300000".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitStepCommand {
    pub owner_user_id: String,
    pub run_id: String,
    pub claim_token: String,
    pub expected_version: u64,
    pub outcome: LocalAgentStepOutcome,
}

impl CommitStepCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)?;
        validate_identifier("claim_token", &self.claim_token)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        self.outcome.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CancelRunCommand {
    pub owner_user_id: String,
    pub run_id: String,
    pub expected_version: Option<u64>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeRunCommand {
    pub owner_user_id: String,
    pub run_id: String,
    pub expected_version: u64,
    pub expected_status: LocalAgentRunStatus,
    pub reason: String,
    #[serde(default)]
    pub input: Value,
}

impl ResumeRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        if !matches!(
            self.expected_status,
            LocalAgentRunStatus::WaitingUser
                | LocalAgentRunStatus::Paused
                | LocalAgentRunStatus::NeedsReview
        ) {
            return Err(
                "only waiting_user, paused, or needs_review runs can be resumed".to_string(),
            );
        }
        validate_text("reason", &self.reason, 4_000)?;
        let input_size = serde_json::to_vec(&self.input)
            .map_err(|error| format!("resume input is not serializable: {error}"))?
            .len();
        if input_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "resume input exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
            ));
        }
        Ok(())
    }
}

impl CancelRunCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("run_id", &self.run_id)?;
        if self.expected_version == Some(0) {
            return Err("expected_version must be greater than zero".to_string());
        }
        validate_text("reason", &self.reason, 4_000)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetEventCursorCommand {
    pub owner_user_id: String,
}

impl GetEventCursorCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListEventsCommand {
    pub owner_user_id: String,
    pub after_cursor: i64,
    pub limit: u32,
    pub run_id: Option<String>,
    #[serde(default)]
    pub event_type: Option<String>,
    #[serde(default)]
    pub newest_first: bool,
    #[serde(default)]
    pub payload_mode: LocalAgentEventPayloadMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WaitEventsCommand {
    pub owner_user_id: String,
    pub after_cursor: i64,
    pub limit: u32,
    pub run_id: Option<String>,
    pub timeout_ms: u64,
    #[serde(default)]
    pub payload_mode: LocalAgentEventPayloadMode,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentEventPayloadMode {
    #[default]
    Full,
    Process,
    Routing,
    None,
}

impl WaitEventsCommand {
    pub fn validate(&self) -> Result<(), String> {
        ListEventsCommand {
            owner_user_id: self.owner_user_id.clone(),
            after_cursor: self.after_cursor,
            limit: self.limit,
            run_id: self.run_id.clone(),
            event_type: None,
            newest_first: false,
            payload_mode: self.payload_mode,
        }
        .validate()?;
        if self.timeout_ms == 0 || self.timeout_ms > 60_000 {
            return Err("timeout_ms must be between 1 and 60000".to_string());
        }
        Ok(())
    }
}

impl ListEventsCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        if self.after_cursor < 0 {
            return Err("after_cursor must not be negative".to_string());
        }
        if self.limit == 0 || self.limit > LOCAL_AGENT_MAX_EVENT_PAGE_SIZE {
            return Err(format!(
                "limit must be between 1 and {LOCAL_AGENT_MAX_EVENT_PAGE_SIZE}"
            ));
        }
        if let Some(run_id) = self.run_id.as_deref() {
            validate_identifier("run_id", run_id)?;
        }
        if let Some(event_type) = self.event_type.as_deref() {
            validate_identifier("event_type", event_type)?;
            if self.run_id.is_none() {
                return Err("event_type requires run_id".to_string());
            }
        }
        if self.newest_first && (self.run_id.is_none() || self.after_cursor != 0) {
            return Err("newest_first requires run_id and after_cursor 0".to_string());
        }
        Ok(())
    }
}
