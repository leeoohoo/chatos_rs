#[derive(Debug, Error)]
pub enum ClientStorageError {
    #[error("client storage database error: {0}")]
    Database(String),
    #[error("client storage serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("record not found: {0}")]
    NotFound(String),
    #[error("storage conflict: {0}")]
    Conflict(String),
    #[error("invalid stored state: {0}")]
    InvalidState(String),
    #[error("command id was reused with different input: {0}")]
    CommandMismatch(String),
    #[error("storage maintenance required: {0}")]
    MaintenanceRequired(String),
    #[error(
        "command completed but its original response is no longer retained; reconcile the \
             original operation instead of resubmitting it with a new command id: {0}"
    )]
    ReplayExpired(String),
}

impl ClientStorageError {
    /// Erases an infrastructure-specific database error at the storage port
    /// boundary. Application packages must not depend on SQLx or SQLite types.
    pub fn database(error: impl std::fmt::Display) -> Self {
        Self::Database(error.to_string())
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "storage_unavailable",
            Self::Serialization(_) | Self::InvalidState(_) => "storage_corrupt",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::CommandMismatch(_) => "command_mismatch",
            Self::MaintenanceRequired(_) => "storage_maintenance_required",
            Self::ReplayExpired(_) => "command_replay_expired",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, Self::Database(_))
    }
}
