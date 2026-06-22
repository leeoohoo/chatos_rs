import { useCallback, useMemo } from 'react';

import { useSessionRuntimeSettings } from '../../../features/sessionRuntime/useSessionRuntimeSettings';
import type { ContactItem } from './types';
import { useTeamMemberRuntimeContext } from './useTeamMemberRuntimeContext';
import { useTeamMembersContactResources } from './useTeamMembersContactResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';
import { useConversationSummariesRealtime } from '../../../lib/realtime/useConversationSummariesRealtime';

interface UseTeamMembersRuntimeResourcesOptions {
  store: ReturnType<typeof useTeamMembersPaneStoreBridge>;
  contacts: ReturnType<typeof useTeamMembersContactResources>;
}

export const useTeamMembersRuntimeResources = ({
  store,
  contacts,
}: UseTeamMembersRuntimeResourcesOptions) => {
  const {
    apiClient,
    sessions,
    remoteConnections,
    currentRemoteConnection,
    sessionChatState,
    selectRemoteConnection,
    updateSession,
  } = store;
  const {
    normalizedProjectId,
    ensureContactSession,
    conversation,
    summary,
    members,
  } = contacts;

  const runtimeContextRefreshNonce = useMemo(() => {
    if (!conversation.selectedProjectSession?.id) {
      return 0;
    }
    return sessionChatState?.[conversation.selectedProjectSession.id]?.runtimeContextRefreshNonce || 0;
  }, [conversation.selectedProjectSession?.id, sessionChatState]);
  const {
    selectedModelId: composerSelectedModelId,
    selectedModelName: composerSelectedModelName,
    selectedThinkingLevel: composerSelectedThinkingLevel,
    setSelectedModelId: handleComposerSelectedModelChange,
    setSelectedModelName: handleComposerSelectedModelNameChange,
    setSelectedThinkingLevel: handleComposerSelectedThinkingLevelChange,
    setModelRuntimeSelection: handleComposerModelRuntimeSelectionChange,
  } = useSessionRuntimeSettings({
    session: conversation.selectedProjectSession,
    updateSession,
  });

  const runtimeContext = useTeamMemberRuntimeContext({
    apiClient,
    sessions: sessions || [],
    normalizedProjectId,
    runtimeContextRefreshNonce,
    ensureContactSession,
    setSelectedContactId: conversation.setSelectedContactId,
  });

  const handleComposerRemoteConnectionChange = useCallback((connectionId: string | null) => {
    void selectRemoteConnection(connectionId, { activatePanel: false });
  }, [selectRemoteConnection]);

  useConversationSummariesRealtime({
    sessionId: conversation.selectedProjectSession?.id || null,
    enabled: Boolean(conversation.selectedProjectSession?.id),
    onEvent: async (payload) => {
      const selectedSessionId = conversation.selectedProjectSession?.id || null;
      if (!selectedSessionId) {
        return;
      }
      if (Array.isArray(payload?.items)) {
        summary.applyRealtimeSessionSummaries(selectedSessionId, payload);
        return;
      }
      summary.markSessionSummariesStale(selectedSessionId);
      if (!conversation.sessionSummaryPaneVisible) {
        return;
      }
      summary.hydrateSessionSummariesFromCache(selectedSessionId);
      await summary.loadSessionSummaries(selectedSessionId, { silent: true });
    },
  });

  const handleRemoveMember = useCallback(async (contact: ContactItem) => {
    const targetSessionId = members.projectContacts.find(
      (item) => item.contact.id === contact.id,
    )?.session?.id || null;
    const removed = await members.removeMemberFromManager(contact);
    if (!removed) {
      return;
    }
    if (conversation.selectedContactId === contact.id) {
      conversation.setSelectedContactId(null);
      summary.setSummaryPaneSessionId(null);
      summary.resetSummaryState();
    }
    if (targetSessionId && runtimeContext.runtimeContextSessionId === targetSessionId) {
      runtimeContext.setRuntimeContextOpen(false);
    }
  }, [
    conversation.selectedContactId,
    conversation.setSelectedContactId,
    members,
    runtimeContext,
    summary,
  ]);

  return {
    composer: {
      composerSelectedModelId,
      composerSelectedModelName,
      composerSelectedThinkingLevel,
      handleComposerSelectedModelChange,
      handleComposerSelectedModelNameChange,
      handleComposerSelectedThinkingLevelChange,
      handleComposerModelRuntimeSelectionChange,
      handleComposerRemoteConnectionChange,
      remoteConnections,
      currentRemoteConnection,
    },
    runtimeContext,
    handleRemoveMember,
  };
};
