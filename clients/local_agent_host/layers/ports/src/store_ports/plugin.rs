use super::*;

#[async_trait]
pub trait LocalPluginInstallationStore: Send + Sync {
    async fn put_plugin_installation(
        &self,
        command: &IdempotentCommand,
        installation: &LocalPluginInstallationSpec,
        expected_version: Option<u64>,
        now_unix_ms: i64,
    ) -> Result<LocalPluginInstallationRecord, ClientStorageError>;

    async fn get_plugin_installation(
        &self,
        owner_user_id: &str,
        installation_id: &str,
    ) -> Result<Option<LocalPluginInstallationRecord>, ClientStorageError>;

    async fn list_plugin_installations(
        &self,
        owner_user_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_installation_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalPluginInstallationPage, ClientStorageError>;

    async fn remove_plugin_installation(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        installation_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<LocalPluginInstallationRecord, ClientStorageError>;
}
