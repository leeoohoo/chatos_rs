#[async_trait]
pub trait LocalCapabilitySnapshotStore: Send + Sync {
    /// Stores an immutable revision. Repeating the same value is safe; reusing
    /// its key for different content must be rejected as a conflict.
    async fn put_capability_snapshot(
        &self,
        command: &IdempotentCommand,
        snapshot: &LocalCapabilityPolicySnapshot,
        now_unix_ms: i64,
    ) -> Result<LocalCapabilityPolicySnapshot, ClientStorageError>;

    async fn get_capability_snapshot(
        &self,
        owner_user_id: &str,
        profile_key: &str,
        capability_policy_revision: &str,
    ) -> Result<Option<LocalCapabilityPolicySnapshot>, ClientStorageError>;

    async fn get_latest_capability_snapshot(
        &self,
        owner_user_id: &str,
        profile_key: &str,
    ) -> Result<Option<LocalCapabilityPolicySnapshot>, ClientStorageError>;
}

#[async_trait]
pub trait LocalModelConfigSnapshotStore: Send + Sync {
    /// Stores a non-secret immutable model configuration revision. API keys
    /// are represented only by native credential-store references.
    async fn put_model_config_snapshot(
        &self,
        command: &IdempotentCommand,
        snapshot: &LocalModelConfigSnapshot,
        now_unix_ms: i64,
    ) -> Result<LocalModelConfigSnapshot, ClientStorageError>;

    async fn get_model_config_snapshot(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<Option<LocalModelConfigSnapshot>, ClientStorageError>;

    /// Resolves the most recently published immutable revision for a model id.
    /// Task creation uses this to preserve the former Task Runner model override
    /// contract without guessing or copying credentials into a Task record.
    async fn get_latest_model_config_snapshot(
        &self,
        owner_user_id: &str,
        model_config_ref: &str,
    ) -> Result<Option<LocalModelConfigSnapshot>, ClientStorageError>;

    async fn list_latest_model_config_snapshots(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalModelConfigSnapshot>, ClientStorageError>;
}
