import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { resolveSessionProjectScopeId } from '../../../features/contactSession/sessionResolver';
import { useSessionRuntimeSettings } from '../../../features/sessionRuntime/useSessionRuntimeSettings';
import { useSessionWorkbarPanels } from '../../chatInterface/useSessionWorkbarPanels';
import type { ContactItem } from './types';
import { useTeamMemberRuntimeContext } from './useTeamMemberRuntimeContext';
import { useTeamMembersContactResources } from './useTeamMembersContactResources';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';

const REVIEW_REPAIR_POLL_INTERVAL_MS = 1200;
const REVIEW_REPAIR_RETRY_INTERVAL_MS = 2000;

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
  const [reviewRepairRunning, setReviewRepairRunning] = useState(false);
  const [reviewRepairPendingCount, setReviewRepairPendingCount] = useState<number | null>(null);
  const reviewRepairPollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearReviewRepairPollTimer = useCallback(() => {
    if (reviewRepairPollTimerRef.current) {
      clearTimeout(reviewRepairPollTimerRef.current);
      reviewRepairPollTimerRef.current = null;
    }
  }, []);

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

  const loadSessionSummaries = summary.loadSessionSummaries;

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

  const refreshReviewRepairStatus = useCallback(async (
    sessionId: string,
  ): Promise<{ running: boolean; pendingCount: number | null }> => {
    if (!sessionId) {
      setReviewRepairRunning(false);
      setReviewRepairPendingCount(null);
      return { running: false, pendingCount: null };
    }
    const result = await apiClient.getConversationReviewRepairStatus(sessionId);
    if (result?.success === false) {
      throw new Error(result.detail || result.error || '获取复盘状态失败');
    }
    const running = result?.result?.running === true;
    const pendingCount = typeof result?.result?.pending_message_count === 'number'
      ? result.result.pending_message_count
      : null;
    setReviewRepairRunning(running);
    setReviewRepairPendingCount(pendingCount);
    return { running, pendingCount };
  }, [apiClient]);

  const pollReviewRepairStatusUntilSettled = useCallback(async (sessionId: string) => {
    clearReviewRepairPollTimer();
    const poll = async () => {
      try {
        const status = await refreshReviewRepairStatus(sessionId);
        if (status.running) {
          reviewRepairPollTimerRef.current = setTimeout(() => {
            void poll();
          }, REVIEW_REPAIR_POLL_INTERVAL_MS);
          return;
        }
        await loadSessionSummaries(sessionId, { silent: true });
      } catch (error) {
        console.error('Failed to poll team review repair status:', error);
        reviewRepairPollTimerRef.current = setTimeout(() => {
          void poll();
        }, REVIEW_REPAIR_RETRY_INTERVAL_MS);
      }
    };
    await poll();
  }, [clearReviewRepairPollTimer, loadSessionSummaries, refreshReviewRepairStatus]);

  const handleRunReviewRepair = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    clearReviewRepairPollTimer();
    setReviewRepairRunning(true);
    try {
      clearError?.();
      const result = await apiClient.runConversationReviewRepair(sessionId);
      if (result?.success === false) {
        throw new Error(result.detail || result.error || '执行复盘失败');
      }
      await pollReviewRepairStatusUntilSettled(sessionId);
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
    clearReviewRepairPollTimer,
    pollReviewRepairStatusUntilSettled,
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

  useEffect(() => {
    const sessionId = conversation.selectedProjectSession?.id || null;
    clearReviewRepairPollTimer();
    if (!sessionId) {
      setReviewRepairRunning(false);
      setReviewRepairPendingCount(null);
      return undefined;
    }

    void refreshReviewRepairStatus(sessionId)
      .then((status) => {
        if (status.running) {
          void pollReviewRepairStatusUntilSettled(sessionId);
        }
      })
      .catch((error) => {
        console.error('Failed to load team review repair status:', error);
      });

    return () => {
      clearReviewRepairPollTimer();
    };
  }, [
    clearReviewRepairPollTimer,
    conversation.selectedProjectSession?.id,
    pollReviewRepairStatusUntilSettled,
    refreshReviewRepairStatus,
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
      handleRunReviewRepair,
      reviewRepairRunning,
      reviewRepairPendingCount,
    },
    runtimeContext,
    handleRemoveMember,
  };
};
