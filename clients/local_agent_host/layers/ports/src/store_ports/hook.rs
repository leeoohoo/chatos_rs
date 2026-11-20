#[async_trait]
pub trait LocalPluginHookBundleStore: Send + Sync {
    /// Appends an immutable trust snapshot. Repeating the same bundle is
    /// idempotent; reusing its identity for different material is a conflict.
    async fn put_plugin_hook_bundle(
        &self,
        command: &IdempotentCommand,
        bundle: &LocalPluginHookBundleSpec,
        now_unix_ms: i64,
    ) -> Result<LocalPluginHookBundleRecord, ClientStorageError>;

    async fn list_plugin_hook_bundles(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalPluginHookBundleRecord>, ClientStorageError>;

    async fn list_all_plugin_hook_bundles(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalPluginHookBundleRecord>, ClientStorageError>;

    /// Loads immutable trusted bundles by their exact event-time identities,
    /// regardless of their current activation state.
    async fn get_plugin_hook_bundles(
        &self,
        owner_user_id: &str,
        bundle_ids: &[String],
    ) -> Result<Vec<LocalPluginHookBundleRecord>, ClientStorageError>;

    async fn set_active_plugin_hook_bundles(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        bundle_ids: &[String],
        now_unix_ms: i64,
    ) -> Result<Vec<String>, ClientStorageError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookEventSpec {
    pub event_id: String,
    pub owner_user_id: String,
    pub event_type: String,
    pub subject_id: String,
    pub sequence: u64,
    pub payload_sha256: String,
    pub payload_json: String,
    pub matcher_value: String,
    pub active_bundle_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookEventRecord {
    pub spec: LocalHookEventSpec,
    pub status: String,
    pub additional_context: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookInvocationSpec {
    pub invocation_id: String,
    pub event_id: String,
    pub bundle_id: String,
    pub hook_id: String,
    pub is_async: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookInvocationRecord {
    pub spec: LocalHookInvocationSpec,
    pub attempt: u32,
    pub status: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub stdout_sha256: Option<String>,
    pub stdout_bytes: u64,
    pub stdout_overflow: bool,
    pub stderr_sha256: Option<String>,
    pub stderr_bytes: u64,
    pub stderr_overflow: bool,
    pub stderr_summary: Option<String>,
    pub additional_context: Option<String>,
    pub async_delivery_key: Option<String>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAsyncHookContextRecord {
    pub invocation_id: String,
    pub event_id: String,
    pub event_type: String,
    pub bundle_id: String,
    pub hook_id: String,
    pub additional_context: String,
    pub async_delivery_key: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct LocalHookOutputSpill {
    bytes: Vec<u8>,
}

impl LocalHookOutputSpill {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn byte_count(&self) -> usize {
        self.bytes.len()
    }
}

impl std::fmt::Debug for LocalHookOutputSpill {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalHookOutputSpill")
            .field("byte_count", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookOutputBlobRecord {
    pub blob_id: String,
    pub invocation_id: String,
    pub stream: String,
    pub sha256: String,
    pub byte_count: u64,
    pub storage_ref: String,
    pub expires_at_unix_ms: i64,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookInvocationCompletion {
    pub status: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub stdout_sha256: Option<String>,
    pub stdout_bytes: u64,
    pub stdout_overflow: bool,
    pub stderr_sha256: Option<String>,
    pub stderr_bytes: u64,
    pub stderr_overflow: bool,
    pub stderr_summary: Option<String>,
    pub additional_context: Option<String>,
    pub stdout_spill: Option<LocalHookOutputSpill>,
    pub stderr_spill: Option<LocalHookOutputSpill>,
}

#[async_trait]
pub trait LocalHookEventStore: Send + Sync {
    async fn has_hook_event(
        &self,
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
    ) -> Result<bool, ClientStorageError>;

    async fn get_hook_event(
        &self,
        owner_user_id: &str,
        event_type: &str,
        subject_id: &str,
        sequence: u64,
    ) -> Result<Option<LocalHookEventRecord>, ClientStorageError>;

    async fn list_pending_hook_events(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalHookEventRecord>, ClientStorageError>;

    async fn list_plugin_hook_invocations(
        &self,
        owner_user_id: &str,
        plugin_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_invocation_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalPluginHookInvocationPage, ClientStorageError>;

    async fn prepare_hook_event(
        &self,
        event: &LocalHookEventSpec,
        now_unix_ms: i64,
    ) -> Result<LocalHookEventRecord, ClientStorageError>;

    async fn begin_hook_invocation(
        &self,
        invocation: &LocalHookInvocationSpec,
        now_unix_ms: i64,
    ) -> Result<LocalHookInvocationRecord, ClientStorageError>;

    async fn complete_hook_invocation(
        &self,
        invocation_id: &str,
        expected_attempt: u32,
        completion: &LocalHookInvocationCompletion,
        now_unix_ms: i64,
    ) -> Result<LocalHookInvocationRecord, ClientStorageError>;

    async fn list_hook_output_blobs(
        &self,
        invocation_id: &str,
    ) -> Result<Vec<LocalHookOutputBlobRecord>, ClientStorageError>;

    async fn claim_async_hook_contexts(
        &self,
        owner_user_id: &str,
        session_id: &str,
        delivery_key: &str,
        limit: u32,
        now_unix_ms: i64,
    ) -> Result<Vec<LocalAsyncHookContextRecord>, ClientStorageError>;

    async fn abandon_running_async_hook_invocations(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    async fn complete_hook_event(
        &self,
        event_id: &str,
        additional_context: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<LocalHookEventRecord, ClientStorageError>;
}
