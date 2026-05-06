import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { resolveSessionProjectScopeId } from '../../../features/contactSession/sessionResolver';
import { countPendingReviewRepairMessages } from '../../../lib/domain/reviewRepair';
import { useSessionRuntimeSettings } from '../../../features/sessionRuntime/useSessionRuntimeSettings';
import { useSessionWorkbarPanels } from '../../chatInterface/useSessionWorkbarPanels';
import type { ContactItem } from './types';
import { useTeamMemberRuntimeContext } from './useTeamMemberRuntimeContext';
import { useTeamMembersContactResources } from './useTeamMembersContactResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';
import { useReviewRepairRealtime } from '../../../lib/realtime/useReviewRepairRealtime';
import { useConversationSummariesRealtime } from '../../../lib/realtime/useConversationSummariesRealtime';
import {
  syncTaskReviewPanelsSnapshot,
  syncUiPromptPanelsSnapshot,
} from '../../chatInterface/helpers';
import {
  loadPendingTaskReviewPanels,
  peekPendingTaskReviewCacheEntry,
} from '../../chatInterface/pendingTaskReviewCache';
import {
  loadPendingUiPromptPanels,
  peekPendingUiPromptCacheEntry,
} from '../../chatInterface/pendingUiPromptCache';

interface UseTeamMembersRuntimeResourcesOptions {
  store: ReturnType<typeof useTeamMembersPaneStoreBridge>;
  contacts: ReturnType<typeof useTeamMembersContactResources>;
}

export const useTeamMembersRuntimeResources = ({
  store,
  contacts,
}: UseTeamMembersRuntimeResourcesOptions) => {
  const [taskHistoryOpen, setTaskHistoryOpen] = useState(false);
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
    clearError,
    setError,
    sessionRuntimeGuidanceState,
    taskReviewPanelsBySession,
    uiPromptPanelsBySession,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
  } = store;
  const taskReviewPanelsBySessionRef = useRef(taskReviewPanelsBySession);
  const uiPromptPanelsBySessionRef = useRef(uiPromptPanelsBySession);

  useEffect(() => {
    taskReviewPanelsBySessionRef.current = taskReviewPanelsBySession;
  }, [taskReviewPanelsBySession]);

  useEffect(() => {
    uiPromptPanelsBySessionRef.current = uiPromptPanelsBySession;
  }, [uiPromptPanelsBySession]);

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
    await summary.loadSessionSummaries(sessionId, { silent: true, force: _force });
  }, [summary.loadSessionSummaries]);

  useEffect(() => {
    setTaskHistoryOpen(false);
  }, [conversation.isSelectedSessionActive, conversation.selectedProjectSession?.id]);

  useEffect(() => {
    const sessionIds = members.projectContacts
      .map((item) => String(item.session?.id || '').trim())
      .filter((sessionId, index, arr) => sessionId.length > 0 && arr.indexOf(sessionId) === index);
    if (sessionIds.length === 0) {
      return;
    }

    let cancelled = false;
    sessionIds.forEach((sessionId) => {
      const cachedTaskReviewPanels = peekPendingTaskReviewCacheEntry(apiClient, sessionId);
      if (cachedTaskReviewPanels && !cachedTaskReviewPanels.stale) {
        syncTaskReviewPanelsSnapshot({
          sessionId,
          panels: cachedTaskReviewPanels.panels,
          existingPanels: taskReviewPanelsBySessionRef.current?.[sessionId],
          upsertTaskReviewPanel,
          removeTaskReviewPanel,
        });
      } else {
        void loadPendingTaskReviewPanels(apiClient, sessionId, { limit: 50 })
          .then((panels) => {
            if (cancelled) {
              return;
            }
            syncTaskReviewPanelsSnapshot({
              sessionId,
              panels,
              existingPanels: taskReviewPanelsBySessionRef.current?.[sessionId],
              upsertTaskReviewPanel,
              removeTaskReviewPanel,
            });
          })
          .catch(() => {});
      }

      const cachedUiPromptPanels = peekPendingUiPromptCacheEntry(apiClient, sessionId);
      if (cachedUiPromptPanels && !cachedUiPromptPanels.stale) {
        syncUiPromptPanelsSnapshot({
          sessionId,
          panels: cachedUiPromptPanels.panels,
          existingPanels: uiPromptPanelsBySessionRef.current?.[sessionId],
          upsertUiPromptPanel,
          removeUiPromptPanel,
        });
      } else {
        void loadPendingUiPromptPanels(apiClient, sessionId, { limit: 50 })
          .then((panels) => {
            if (cancelled) {
              return;
            }
            syncUiPromptPanelsSnapshot({
              sessionId,
              panels,
              existingPanels: uiPromptPanelsBySessionRef.current?.[sessionId],
              upsertUiPromptPanel,
              removeUiPromptPanel,
            });
          })
          .catch(() => {});
      }
    });

    return () => {
      cancelled = true;
    };
  }, [
    apiClient,
    members.projectContacts,
    removeTaskReviewPanel,
    removeUiPromptPanel,
    upsertTaskReviewPanel,
    upsertUiPromptPanel,
  ]);

  const workbar = useSessionWorkbarPanels({
    apiClient,
    session: conversation.isSelectedSessionActive ? conversation.selectedProjectSession : null,
    enabled: Boolean(conversation.isSelectedSessionActive && conversation.selectedProjectSession?.id),
    messages,
    selectedSessionActiveTurnId,
    taskHistoryOpen,
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
    setTaskHistoryOpen(true);
    workbar.handleOpenWorkbarHistory(sessionId, { forceHistory: false, forceSummaries: false });
  }, [workbar.handleOpenWorkbarHistory]);

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

  useConversationSummariesRealtime({
    sessionId: conversation.selectedProjectSession?.id || null,
    enabled: Boolean(conversation.selectedProjectSession?.id),
    onEvent: async () => {
      const selectedSessionId = conversation.selectedProjectSession?.id || null;
      if (!selectedSessionId) {
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

  const {
    reviewRepairRunning,
    reviewRepairPendingCount,
    refreshReviewRepairStatus,
    markReviewRepairStarting,
  } = useReviewRepairRealtime({
    apiClient,
    sessionId: conversation.selectedProjectSession?.id || null,
    messageCountHint: conversation.selectedProjectSession?.id
      ? messages.length
      : undefined,
    onFailed: (errorMessage) => {
      setError?.(errorMessage);
    },
    onCompleted: async () => {
      const selectedSessionId = conversation.selectedProjectSession?.id || null;
      if (!selectedSessionId) {
        return;
      }
      await store.loadMessages(selectedSessionId);
    },
  });

  const loadedReviewRepairPendingCount = useMemo(() => {
    const selectedSessionId = conversation.selectedProjectSession?.id || null;
    if (!selectedSessionId) {
      return 0;
    }
    return countPendingReviewRepairMessages(messages, selectedSessionId);
  }, [conversation.selectedProjectSession?.id, messages]);
  const reviewRepairDisabled = !reviewRepairRunning
    && reviewRepairPendingCount === 0
    && loadedReviewRepairPendingCount === 0;

  const handleRunReviewRepair = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    markReviewRepairStarting();
    try {
      clearError?.();
      const result = await apiClient.runConversationReviewRepair(sessionId);
      if (result?.success === false) {
        throw new Error(result.detail || result.error || '执行复盘失败');
      }
    } catch (error) {
      await refreshReviewRepairStatus(sessionId).catch((statusError) => {
        console.error('Failed to refresh team review repair status after run error:', statusError);
      });
      setError?.(error instanceof Error ? error.message : '执行复盘失败');
      throw error;
    }
  }, [
    apiClient,
    clearError,
    markReviewRepairStarting,
    refreshReviewRepairStatus,
    setError,
  ]);

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
      taskHistoryOpen,
      setTaskHistoryOpen,
      handleOpenTeamWorkbarHistory,
      handleRunReviewRepair,
      reviewRepairRunning,
      reviewRepairPendingCount,
      reviewRepairDisabled,
    },
    runtimeContext,
    handleRemoveMember,
  };
};
