import { useCallback, useMemo } from 'react';

import { resolveSessionProjectScopeId } from '../../../features/contactSession/sessionResolver';
import { useSessionRuntimeSettings } from '../../../features/sessionRuntime/useSessionRuntimeSettings';
import { useSessionWorkbarPanels } from '../../chatInterface/useSessionWorkbarPanels';
import type { ContactItem } from './types';
import { useTeamMemberRuntimeContext } from './useTeamMemberRuntimeContext';
import { useTeamMembersContactResources } from './useTeamMembersContactResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';

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
    currentSession,
    sessions,
    remoteConnections,
    currentRemoteConnection,
    messages,
    sessionChatState,
    selectRemoteConnection,
    updateSession,
    submitRuntimeGuidance,
    sessionRuntimeGuidanceState,
    taskReviewPanelsBySession,
    uiPromptPanelsBySession,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
  } = store;

  const {
    normalizedProjectId,
    ensureContactSession,
    conversation,
    summary,
    members,
  } = contacts;

  const selectedSessionActiveTurnId = useMemo(() => {
    if (!conversation.selectedProjectSession?.id) {
      return null;
    }
    const raw = sessionChatState?.[conversation.selectedProjectSession.id]?.activeTurnId;
    const normalized = typeof raw === 'string' ? raw.trim() : '';
    return normalized || null;
  }, [conversation.selectedProjectSession?.id, sessionChatState]);

  const runtimeContextRefreshNonce = useMemo(() => {
    if (!conversation.selectedProjectSession?.id) {
      return 0;
    }
    return sessionChatState?.[conversation.selectedProjectSession.id]?.runtimeContextRefreshNonce || 0;
  }, [conversation.selectedProjectSession?.id, sessionChatState]);

  const runtimeSourceSession = conversation.selectedProjectSession || currentSession;
  const {
    mcpEnabled: composerMcpEnabled,
    enabledMcpIds: composerEnabledMcpIds,
    setMcpEnabled: handleComposerMcpEnabledChange,
    setEnabledMcpIds: handleComposerEnabledMcpIdsChange,
  } = useSessionRuntimeSettings({
    session: runtimeSourceSession,
    updateSession,
  });

  const loadWorkbarSummaries = useCallback(async (sessionId: string, _force = false) => {
    if (!sessionId) {
      return;
    }
    await summary.loadSessionSummaries(sessionId, { silent: true });
  }, [summary.loadSessionSummaries]);

  const workbar = useSessionWorkbarPanels({
    apiClient,
    session: conversation.isSelectedSessionActive ? conversation.selectedProjectSession : null,
    enabled: Boolean(conversation.isSelectedSessionActive && conversation.selectedProjectSession?.id),
    messages,
    selectedSessionActiveTurnId,
    sessionRuntimeGuidanceState,
    taskReviewPanelsBySession,
    uiPromptPanelsBySession,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    loadWorkbarSummaries,
  });

  const handleOpenTeamWorkbarHistory = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    summary.setSummaryPaneSessionId(sessionId);
    workbar.handleOpenWorkbarHistory(sessionId, { forceHistory: true, forceSummaries: true });
  }, [summary.setSummaryPaneSessionId, workbar.handleOpenWorkbarHistory]);

  const runtimeContext = useTeamMemberRuntimeContext({
    apiClient,
    sessions: sessions || [],
    normalizedProjectId,
    runtimeContextRefreshNonce,
    ensureContactSession,
    setSelectedContactId: conversation.setSelectedContactId,
  });

  const handleRuntimeGuidanceSend = useCallback(async (content: string) => {
    if (!conversation.selectedProjectSession) {
      return;
    }
    if (resolveSessionProjectScopeId(conversation.selectedProjectSession) !== normalizedProjectId) {
      console.warn('Blocked runtime guidance for cross-project session in team pane.');
      return;
    }

    const sessionId = conversation.selectedProjectSession.id;
    const turnId = String(sessionChatState?.[sessionId]?.activeTurnId || '').trim();
    if (!sessionId || !turnId) {
      return;
    }
    try {
      await submitRuntimeGuidance(content, {
        conversationId: sessionId,
        turnId,
        projectId: normalizedProjectId,
      });
    } catch (error) {
      console.error('Failed to submit runtime guidance in team pane:', error);
    }
  }, [
    conversation.selectedProjectSession,
    normalizedProjectId,
    sessionChatState,
    submitRuntimeGuidance,
  ]);

  const handleComposerRemoteConnectionChange = useCallback((connectionId: string | null) => {
    void selectRemoteConnection(connectionId, { activatePanel: false });
  }, [selectRemoteConnection]);

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
      composerMcpEnabled,
      composerEnabledMcpIds,
      handleComposerMcpEnabledChange,
      handleComposerEnabledMcpIdsChange,
      handleComposerRemoteConnectionChange,
      handleRuntimeGuidanceSend,
      remoteConnections,
      currentRemoteConnection,
    },
    workbar: {
      ...workbar,
      handleOpenTeamWorkbarHistory,
    },
    runtimeContext,
    handleRemoveMember,
  };
};
