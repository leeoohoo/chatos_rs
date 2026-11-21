use super::*;

#[async_trait]
pub trait LocalRemoteConnectionStore: Send + Sync {
    async fn list_remote_connections(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalRemoteConnection>, ClientStorageError>;

    async fn get_remote_connection(
        &self,
        owner_user_id: &str,
        connection_id: &str,
    ) -> Result<Option<LocalRemoteConnection>, ClientStorageError>;

    async fn create_remote_connection(
        &self,
        command: &IdempotentCommand,
        connection: &LocalRemoteConnection,
    ) -> Result<LocalRemoteConnection, ClientStorageError>;

    async fn update_remote_connection(
        &self,
        command: &IdempotentCommand,
        connection: &LocalRemoteConnection,
        expected_version: u64,
    ) -> Result<LocalRemoteConnection, ClientStorageError>;

    async fn delete_remote_connection(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        connection_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError>;
}
