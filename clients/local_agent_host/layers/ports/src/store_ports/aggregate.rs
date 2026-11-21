use super::*;

pub trait LocalAgentStore:
    LocalAgentRunStore
    + LocalAgentToolStore
    + LocalAgentTaskStore
    + LocalPluginInstallationStore
    + LocalPluginHookBundleStore
    + LocalHookEventStore
    + LocalConversationStore
    + LocalConversationRuntimeSettingsStore
    + LocalNotepadStore
    + LocalRemoteConnectionStore
    + LocalAgentArtifactStore
    + LocalRequirementSurveyStore
    + LocalCapabilitySnapshotStore
    + LocalModelConfigSnapshotStore
    + LocalMemoryOutboxStore
    + LocalMemoryContextCacheStore
{
}

impl<T> LocalAgentStore for T where
    T: LocalAgentRunStore
        + LocalAgentToolStore
        + LocalAgentTaskStore
        + LocalPluginInstallationStore
        + LocalPluginHookBundleStore
        + LocalHookEventStore
        + LocalConversationStore
        + LocalConversationRuntimeSettingsStore
        + LocalNotepadStore
        + LocalRemoteConnectionStore
        + LocalAgentArtifactStore
        + LocalRequirementSurveyStore
        + LocalCapabilitySnapshotStore
        + LocalModelConfigSnapshotStore
        + LocalMemoryOutboxStore
        + LocalMemoryContextCacheStore
{
}
