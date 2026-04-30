import { useCallback, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type {
  SendMessageHandler,
  SendMessageRuntimeOptions,
  Session,
} from '../../types';
import { useChatSessionEffects } from './useChatSessionEffects';
import { useRuntimeContextState } from './useRuntimeContextState';
import { useReviewRepairRealtime } from '../../lib/realtime/useReviewRepairRealtime';
import { useConversationSummariesRealtime } from '../../lib/realtime/useConversationSummariesRealtime';

interface UseChatInterfaceControllerParams {
  apiClient: ApiClient;
  activePanel: string;
  currentSession: Session | null;
  runtimeContextRefreshNonce: number;
  currentChatStateActiveTurnId: string | null | undefined;
  activeConversationTurnId: string | null | undefined;
  currentRemoteConnectionId: string | null;
  uiPromptHistoryOpen: boolean;
  setUiPromptHistoryOpen: (value: boolean) => void;
  summaryPaneSessionId: string | null;
  setSummaryPaneSessionId: (value: string | null | ((prev: string | null) => string | null)) => void;
  setTaskHistoryOpen?: (value: boolean) => void;
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
  loadSessionMemorySummaries: (sessionId: string, force?: boolean) => Promise<unknown>;
  hydrateContactMemoryContextFromCache: (sessionId: string) => void;
  markContactMemoryContextStale: (sessionId: string) => void;
  resetMemoryState: () => void;
  cancelPendingMemoryLoad: () => void;
  loadUiPromptHistory: (sessionId: string, force?: boolean) => Promise<unknown>;
  hydrateUiPromptHistoryFromCache: (sessionId: string) => void;
  resetUiPromptHistoryState: () => void;
  cancelPendingUiPromptHistoryLoad: () => void;
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
  uiPromptHistoryOpen,
  setUiPromptHistoryOpen,
  summaryPaneSessionId,
  setSummaryPaneSessionId,
  setTaskHistoryOpen,
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
  loadSessionMemorySummaries,
  hydrateContactMemoryContextFromCache,
  markContactMemoryContextStale,
  resetMemoryState,
  cancelPendingMemoryLoad,
  loadUiPromptHistory,
  hydrateUiPromptHistoryFromCache,
  resetUiPromptHistoryState,
  cancelPendingUiPromptHistoryLoad,
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

  const { sessionSummaryPaneVisible } = useChatSessionEffects({
    activePanel,
    currentSession,
    uiPromptHistoryOpen,
    summaryPaneSessionId,
    setTaskHistoryOpen,
    loadProjects,
    loadAiModelConfigs,
    loadAgents,
    loadContactMemoryContext,
    hydrateContactMemoryContextFromCache,
    resetMemoryState,
    cancelPendingMemoryLoad,
    loadUiPromptHistory,
    hydrateUiPromptHistoryFromCache,
    resetUiPromptHistoryState,
    cancelPendingUiPromptHistoryLoad,
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

  useConversationSummariesRealtime({
    sessionId: activePanel === 'chat' ? (currentSession?.id || null) : null,
    enabled: activePanel === 'chat',
    onEvent: async () => {
      const sessionId = currentSession?.id || null;
      if (!sessionId) {
        return;
      }
      markContactMemoryContextStale(sessionId);
      if (!sessionSummaryPaneVisible) {
        return;
      }
      hydrateContactMemoryContextFromCache(sessionId);
      await loadSessionMemorySummaries(sessionId);
    },
  });

  const {
    reviewRepairRunning,
    reviewRepairPendingCount,
    refreshReviewRepairStatus,
    markReviewRepairStarting,
  } = useReviewRepairRealtime({
    apiClient,
    sessionId: activePanel === 'chat' ? (currentSession?.id || null) : null,
    enabled: activePanel === 'chat',
  });

  const handleRunReviewRepair = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    markReviewRepairStarting();
    try {
      const result = await apiClient.runConversationReviewRepair(sessionId);
      if (result?.success === false) {
        throw new Error(result.detail || result.error || '执行复盘失败');
      }
    } catch (error) {
      await refreshReviewRepairStatus(sessionId).catch((statusError) => {
        console.error('Failed to refresh review repair status after run error:', statusError);
      });
      throw error;
    }
  }, [
    apiClient,
    markReviewRepairStarting,
    refreshReviewRepairStatus,
  ]);

  const handleCloseSummary = useCallback(() => {
    setSummaryPaneSessionId(null);
  }, []);

  const handleOpenHistory = useCallback((sessionId: string) => {
    handleOpenWorkbarHistory(sessionId, { forceHistory: false, forceSummaries: false });
  }, [handleOpenWorkbarHistory]);

  const handleOpenUiPromptHistory = useCallback((sessionId: string) => {
    setUiPromptHistoryOpen(true);
    void loadUiPromptHistory(sessionId);
  }, [loadUiPromptHistory]);

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
