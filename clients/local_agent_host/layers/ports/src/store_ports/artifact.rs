#[async_trait]
pub trait LocalAgentArtifactStore: Send + Sync {
    async fn create_artifact(
        &self,
        command: &IdempotentCommand,
        write: &LocalAgentArtifactWrite,
    ) -> Result<LocalAgentArtifact, ClientStorageError>;

    async fn list_artifacts(
        &self,
        owner_user_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_artifact_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalAgentArtifactPage, ClientStorageError>;

    async fn read_artifact_data(
        &self,
        owner_user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<Vec<u8>>, ClientStorageError>;

    async fn read_artifact_file(
        &self,
        owner_user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<LocalAgentArtifactFile>, ClientStorageError>;

    async fn delete_artifact(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        artifact_id: &str,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError>;
}
