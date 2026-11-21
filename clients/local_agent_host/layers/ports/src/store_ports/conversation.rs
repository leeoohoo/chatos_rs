use super::*;

#[async_trait]
pub trait LocalConversationStore: Send + Sync {
    async fn create_conversation(
        &self,
        command: &IdempotentCommand,
        conversation: &CreateConversationCommand,
        now_unix_ms: i64,
    ) -> Result<LocalConversationDetail, ClientStorageError>;

    async fn get_conversation(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationDetail>, ClientStorageError>;

    async fn get_conversation_context(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationDetail>, ClientStorageError>;

    async fn get_conversation_attachment(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        authorized_local_ref: &str,
    ) -> Result<Option<LocalConversationAttachmentRecord>, ClientStorageError>;

    async fn list_recent_conversation_attachments(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        turn_limit: u32,
    ) -> Result<Vec<LocalConversationAttachmentRecord>, ClientStorageError>;

    async fn get_conversation_history(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        before_ordinal: Option<u64>,
        limit: u32,
    ) -> Result<LocalConversationHistoryPage, ClientStorageError>;

    async fn list_conversations(
        &self,
        owner_user_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_conversation_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalConversationPage, ClientStorageError>;

    async fn start_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &StartConversationTurnCommand,
        run: &LocalAgentRunRecord,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnStart, ClientStorageError>;

    async fn guide_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &GuideConversationTurnCommand,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnUpdate, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn resume_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &ResumeConversationTurnCommand,
        continuation_input: &Value,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnUpdate, ClientStorageError>;

    async fn cancel_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &CancelConversationTurnCommand,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnUpdate, ClientStorageError>;
}

#[async_trait]
pub trait LocalConversationRuntimeSettingsStore: Send + Sync {
    async fn get_conversation_runtime_settings(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationRuntimeSettings>, ClientStorageError>;

    async fn put_conversation_runtime_settings(
        &self,
        command: &IdempotentCommand,
        settings: &PutConversationRuntimeSettingsCommand,
        now_unix_ms: i64,
    ) -> Result<LocalConversationRuntimeSettings, ClientStorageError>;
}
