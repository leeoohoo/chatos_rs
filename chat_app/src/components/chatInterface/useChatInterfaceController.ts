import { useCallback, useEffect, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type { UiPromptPanelState } from '../../lib/store/types';
import type {
  SendMessageHandler,
  SendMessageRuntimeOptions,
  Session,
} from '../../types';
import { useChatSessionEffects } from './useChatSessionEffects';
import { useRuntimeContextState } from './useRuntimeContextState';

interface UseChatInterfaceControllerParams {
  apiClient: ApiClient;
  activePanel: string;
  currentSession: Session | null;
  runtimeContextRefreshNonce: number;
  currentChatStateActiveTurnId: string | null | undefined;
  activeConversationTurnId: string | null | undefined;
  currentRemoteConnectionId: string | null;
  onMessageSend?: (content: string, attachments?: File[]) => void;
  sendMessage: SendMessageHandler;
  selectRemoteConnection: (
    connectionId: string | null,
    options?: { activatePanel?: boolean },
  ) => Promise<void>;
  submitRuntimeGuidance: (
    content: string,
    options: { conversationId: string; turnId: string; projectId?: string | null },
  ) => Promise<unknown>;
  loadMoreMessages: (sessionId: string) => Promise<void>;
  toggleTurnProcess: (
    userMessageId: string,
    options?: { forceExpand?: boolean; forceCollapse?: boolean },
  ) => Promise<void>;
  loadProjects: () => Promise<unknown>;
  loadAiModelConfigs: () => Promise<void>;
  loadAgents: () => Promise<void>;
  loadContactMemoryContext: (sessionId: string, force?: boolean) => Promise<unknown>;
  resetMemoryState: () => void;
  cancelPendingMemoryLoad: () => void;
  loadUiPromptHistory: (sessionId: string, force?: boolean) => Promise<unknown>;
  hydrateUiPromptHistoryFromCache: (sessionId: string) => void;
  resetUiPromptHistoryState: () => void;
  cancelPendingUiPromptHistoryLoad: () => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  resetAllWorkbarState: () => void;
  resetHistoryWorkbarState: () => void;
  handleOpenWorkbarHistory: (
    sessionId: string,
    options?: { forceHistory?: boolean; forceSummaries?: boolean },
  ) => void;
}

export const useChatInterfaceController = ({
  apiClient,
  activePanel,
  currentSession,
  runtimeContextRefreshNonce,
  currentChatStateActiveTurnId,
  activeConversationTurnId,
  currentRemoteConnectionId,
  onMessageSend,
  sendMessage,
  selectRemoteConnection,
  submitRuntimeGuidance,
  loadMoreMessages,
  toggleTurnProcess,
  loadProjects,
  loadAiModelConfigs,
  loadAgents,
  loadContactMemoryContext,
  resetMemoryState,
  cancelPendingMemoryLoad,
  loadUiPromptHistory,
  hydrateUiPromptHistoryFromCache,
  resetUiPromptHistoryState,
  cancelPendingUiPromptHistoryLoad,
  upsertUiPromptPanel,
  resetAllWorkbarState,
  resetHistoryWorkbarState,
  handleOpenWorkbarHistory,
}: UseChatInterfaceControllerParams) => {
  const [showMcpManager, setShowMcpManager] = useState(false);
  const [showAiModelManager, setShowAiModelManager] = useState(false);
  const [showSystemContextEditor, setShowSystemContextEditor] = useState(false);
  const [showApplicationsPanel, setShowApplicationsPanel] = useState(false);
  const [showNotepadPanel, setShowNotepadPanel] = useState(false);
  const [showUserSettings, setShowUserSettings] = useState(false);
  const [summaryPaneSessionId, setSummaryPaneSessionId] = useState<string | null>(null);
  const [uiPromptHistoryOpen, setUiPromptHistoryOpen] = useState(false);
  const [reviewRepairRunning, setReviewRepairRunning] = useState(false);
  const [reviewRepairPendingCount, setReviewRepairPendingCount] = useState<number | null>(null);
  const reviewRepairPollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearReviewRepairPollTimer = useCallback(() => {
    if (reviewRepairPollTimerRef.current) {
      clearTimeout(reviewRepairPollTimerRef.current);
      reviewRepairPollTimerRef.current = null;
    }
  }, []);

  const currentSessionIdForUiPrompts = currentSession?.id || null;
  const { sessionSummaryPaneVisible } = useChatSessionEffects({
    apiClient,
    activePanel,
    currentSession,
    currentSessionIdForUiPrompts,
    uiPromptHistoryOpen,
    summaryPaneSessionId,
    loadProjects,
    loadAiModelConfigs,
    loadAgents,
    loadContactMemoryContext,
    resetMemoryState,
    cancelPendingMemoryLoad,
    loadUiPromptHistory,
    hydrateUiPromptHistoryFromCache,
    resetUiPromptHistoryState,
    cancelPendingUiPromptHistoryLoad,
    upsertUiPromptPanel,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
    setUiPromptHistoryOpen,
  });
  const {
    runtimeContextOpen,
    setRuntimeContextOpen,
    runtimeContextSessionId,
    runtimeContextData,
    runtimeContextLoading,
    runtimeContextError,
    handleOpenRuntimeContext,
    handleRefreshRuntimeContext,
  } = useRuntimeContextState({
    apiClient,
    currentSession,
    runtimeContextRefreshNonce,
  });

  const handleMessageSend = useCallback(async (
    content: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => {
    try {
      const hasRemoteConnectionIdOverride = Boolean(
        runtimeOptions
        && Object.prototype.hasOwnProperty.call(runtimeOptions, 'remoteConnectionId'),
      );
      const remoteConnectionIdOverride = hasRemoteConnectionIdOverride
        ? (typeof runtimeOptions?.remoteConnectionId === 'string'
          ? runtimeOptions.remoteConnectionId
          : null)
        : undefined;
      await sendMessage(content, attachments, {
        ...runtimeOptions,
        remoteConnectionId: hasRemoteConnectionIdOverride
          ? (remoteConnectionIdOverride ?? null)
          : (currentRemoteConnectionId || null),
      });
      onMessageSend?.(content, attachments);
    } catch (error) {
      console.error('Failed to send message:', error);
    }
  }, [currentRemoteConnectionId, onMessageSend, sendMessage]);

  const handleComposerRemoteConnectionChange = useCallback((connectionId: string | null) => {
    void selectRemoteConnection(connectionId, { activatePanel: false });
  }, [selectRemoteConnection]);

  const handleRuntimeGuidanceSend = useCallback(async (content: string) => {
    const sessionId = currentSession?.id;
    const projectId = currentSession?.projectId || currentSession?.project_id || null;
    const turnId = (
      currentChatStateActiveTurnId
      || activeConversationTurnId
      || ''
    ).trim();
    if (!sessionId || !turnId) {
      return;
    }
    try {
      await submitRuntimeGuidance(content, { conversationId: sessionId, turnId, projectId });
    } catch (error) {
      console.error('Failed to submit runtime guidance:', error);
    }
  }, [
    activeConversationTurnId,
    currentChatStateActiveTurnId,
    currentSession,
    submitRuntimeGuidance,
  ]);

  const handleLoadMore = useCallback(() => {
    if (currentSession) {
      void loadMoreMessages(currentSession.id);
    }
  }, [currentSession, loadMoreMessages]);

  const handleToggleTurnProcess = useCallback((userMessageId: string) => {
    if (!userMessageId) {
      return;
    }
    void toggleTurnProcess(userMessageId).catch((error) => {
      console.error('Failed to toggle turn process messages:', error);
    });
  }, [toggleTurnProcess]);

  const handleRefreshMemory = useCallback((sessionId: string) => {
    void loadContactMemoryContext(sessionId, true);
  }, [loadContactMemoryContext]);

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
          }, 1200);
          return;
        }
        await loadContactMemoryContext(sessionId, true);
      } catch (error) {
        console.error('Failed to poll review repair status:', error);
        reviewRepairPollTimerRef.current = setTimeout(() => {
          void poll();
        }, 2000);
      }
    };
    await poll();
  }, [clearReviewRepairPollTimer, loadContactMemoryContext, refreshReviewRepairStatus]);

  const handleRunReviewRepair = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    clearReviewRepairPollTimer();
    setReviewRepairRunning(true);
    try {
      const result = await apiClient.runConversationReviewRepair(sessionId);
      if (result?.success === false) {
        throw new Error(result.detail || result.error || '执行复盘失败');
      }
      await pollReviewRepairStatusUntilSettled(sessionId);
    } catch (error) {
      await refreshReviewRepairStatus(sessionId).catch((statusError) => {
        console.error('Failed to refresh review repair status after run error:', statusError);
      });
      throw error;
    }
  }, [
    apiClient,
    clearReviewRepairPollTimer,
    pollReviewRepairStatusUntilSettled,
    refreshReviewRepairStatus,
  ]);

  const handleCloseSummary = useCallback(() => {
    setSummaryPaneSessionId(null);
  }, []);

  const handleOpenHistory = useCallback((sessionId: string) => {
    setSummaryPaneSessionId(sessionId);
    handleOpenWorkbarHistory(sessionId, { forceHistory: false, forceSummaries: true });
  }, [handleOpenWorkbarHistory]);

  const handleOpenUiPromptHistory = useCallback((sessionId: string) => {
    setUiPromptHistoryOpen(true);
    void loadUiPromptHistory(sessionId);
  }, [loadUiPromptHistory]);

  useEffect(() => {
    if (activePanel !== 'chat') {
      clearReviewRepairPollTimer();
      setReviewRepairRunning(false);
      setReviewRepairPendingCount(null);
      return undefined;
    }

    const sessionId = currentSession?.id || null;
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
        console.error('Failed to load review repair status:', error);
      });

    return () => {
      clearReviewRepairPollTimer();
    };
  }, [
    activePanel,
    clearReviewRepairPollTimer,
    currentSession?.id,
    pollReviewRepairStatusUntilSettled,
    refreshReviewRepairStatus,
  ]);

  return {
    showMcpManager,
    setShowMcpManager,
    showAiModelManager,
    setShowAiModelManager,
    showSystemContextEditor,
    setShowSystemContextEditor,
    showApplicationsPanel,
    setShowApplicationsPanel,
    showNotepadPanel,
    setShowNotepadPanel,
    showUserSettings,
    setShowUserSettings,
    summaryPaneSessionId,
    setSummaryPaneSessionId,
    sessionSummaryPaneVisible,
    uiPromptHistoryOpen,
    setUiPromptHistoryOpen,
    reviewRepairRunning,
    reviewRepairPendingCount,
    runtimeContextOpen,
    setRuntimeContextOpen,
    runtimeContextSessionId,
    runtimeContextData,
    runtimeContextLoading,
    runtimeContextError,
    handleMessageSend,
    handleComposerRemoteConnectionChange,
    handleRuntimeGuidanceSend,
    handleLoadMore,
    handleToggleTurnProcess,
    handleRefreshMemory,
    handleRunReviewRepair,
    handleCloseSummary,
    handleOpenHistory,
    handleOpenUiPromptHistory,
    handleOpenRuntimeContext,
    handleRefreshRuntimeContext,
  };
};
