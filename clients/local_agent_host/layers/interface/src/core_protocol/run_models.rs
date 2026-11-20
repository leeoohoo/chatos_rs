#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalAgentRunStatus {
    Queued,
    ModelReady,
    ModelRunning,
    WaitingToolResult,
    ContinuationReady,
    WaitingUser,
    RetryScheduled,
    Paused,
    NeedsReview,
    Succeeded,
    Failed,
    Cancelled,
}

impl LocalAgentRunStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::ModelReady => "model_ready",
            Self::ModelRunning => "model_running",
            Self::WaitingToolResult => "waiting_tool_result",
            Self::ContinuationReady => "continuation_ready",
            Self::WaitingUser => "waiting_user",
            Self::RetryScheduled => "retry_scheduled",
            Self::Paused => "paused",
            Self::NeedsReview => "needs_review",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for LocalAgentRunStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for LocalAgentRunStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "model_ready" => Ok(Self::ModelReady),
            "model_running" => Ok(Self::ModelRunning),
            "waiting_tool_result" => Ok(Self::WaitingToolResult),
            "continuation_ready" => Ok(Self::ContinuationReady),
            "waiting_user" => Ok(Self::WaitingUser),
            "retry_scheduled" => Ok(Self::RetryScheduled),
            "paused" => Ok(Self::Paused),
            "needs_review" => Ok(Self::NeedsReview),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown Local Agent run status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentRunRecord {
    pub run_id: String,
    pub owner_user_id: String,
    pub owner_entity_type: String,
    pub owner_entity_id: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    pub input: Value,
    pub status: LocalAgentRunStatus,
    pub iteration: u32,
    pub model_attempt: u32,
    pub max_iterations: u32,
    pub version: u64,
    pub claim_token: Option<String>,
    pub claim_until_unix_ms: Option<i64>,
    pub next_attempt_at_unix_ms: Option<i64>,
    pub pending_tool_batch: Option<Value>,
    pub checkpoint: Value,
    pub continuation_input: Option<Value>,
    pub terminal_outcome: Option<Value>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentRunClaim {
    pub worker_id: String,
    pub claim_token: String,
    pub run: LocalAgentRunRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalAgentEventRecord {
    pub cursor: i64,
    pub event_id: String,
    pub run_id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at_unix_ms: i64,
}

pub fn validate_identifier(name: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!("{name} must be 1..=256 non-control characters"));
    }
    Ok(())
}

pub fn validate_text(name: &str, value: &str, maximum_length: usize) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > maximum_length || value.chars().any(|value| value == '\0')
    {
        return Err(format!(
            "{name} must be 1..={maximum_length} characters without NUL"
        ));
    }
    Ok(())
}
