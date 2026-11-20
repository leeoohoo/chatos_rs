#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalAgentStepOutcome {
    Continue {
        #[serde(default)]
        checkpoint: Value,
    },
    WaitForTool {
        batch_id: String,
        #[serde(default)]
        tool_calls: Vec<LocalAgentToolCall>,
        #[serde(default)]
        checkpoint: Value,
    },
    WaitForUser {
        #[serde(default)]
        prompt: Value,
        #[serde(default)]
        checkpoint: Value,
    },
    Retry {
        resume_at_unix_ms: i64,
        next_model_attempt: u32,
        reason: String,
    },
    Pause {
        reason: String,
    },
    NeedsReview {
        reason: String,
        #[serde(default)]
        detail: Value,
    },
    Succeed {
        #[serde(default)]
        output: Value,
    },
    Fail {
        error: String,
        #[serde(default)]
        detail: Value,
    },
}

impl LocalAgentStepOutcome {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Continue { .. } | Self::WaitForUser { .. } | Self::Succeed { .. } => Ok(()),
            Self::WaitForTool {
                batch_id,
                tool_calls,
                ..
            } => {
                validate_identifier("batch_id", batch_id)?;
                if tool_calls.is_empty() {
                    return Err("wait_for_tool requires at least one tool call".to_string());
                }
                LocalAgentToolBatch {
                    batch_id: batch_id.clone(),
                    calls: tool_calls.clone(),
                }
                .validate()?;
                Ok(())
            }
            Self::Retry {
                resume_at_unix_ms,
                next_model_attempt,
                reason,
            } => {
                if *resume_at_unix_ms <= 0 {
                    return Err("resume_at_unix_ms must be positive".to_string());
                }
                if *next_model_attempt < 2 {
                    return Err("next_model_attempt must be at least 2".to_string());
                }
                validate_text("reason", reason, 4_000)
            }
            Self::Pause { reason } | Self::NeedsReview { reason, .. } => {
                validate_text("reason", reason, 4_000)
            }
            Self::Fail { error, .. } => validate_text("error", error, 8_000),
        }
    }
}
