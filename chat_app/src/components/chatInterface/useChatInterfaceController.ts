import { useCallback } from 'react';

import type ApiClient from '../../lib/api/client';
import { countPendingReviewRepairMessages } from '../../lib/domain/reviewRepair';
import type {
  SendMessageHandler,
  SendMessageRuntimeOptions,
  Message,
  Session,
} from '../../types';
import { useChatSessionEffects } from './useChatSessionEffects';
import { useRuntimeContextState } from './useRuntimeContextState';
import { useChatInterfaceOverlayState } from './useChatInterfaceOverlayState';
import { useReviewRepairRealtime } from '../../lib/realtime/useReviewRepairRealtime';
import { useConversationSummariesRealtime } from '../../lib/realtime/useConversationSummariesRealtime';
import type { RealtimeConversationSummariesUpdatedPayloadWrapper } from '../../lib/realtime/types';

interface UseChatInterfaceControllerParams {
  apiClient: ApiClient;
  activePanel: string;
  currentSession: Session | null;
  messages: Message[];
  currentMessageCount: number;
  currentSessionHasMoreMessages: boolean;
  runtimeContextRefreshNonce: number;
  currentChatStateActiveTurnId: string | null | undefined;
  activeConversationTurnId: string | null | undefined;
  currentRemoteConnectionId: string | null;
  uiPromptHistoryOpen: boolean;
  setUiPromptHistoryOpen: (value: boolean) => void;
  summaryPaneSessionId: string | null;
  setSummaryPaneSessionId: (value: string | null | ((prev: string | null) => string | null)) => void;
  closeTurnProcessViewer: () => void;
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
  openTurnProcessViewer: (
    sessionId: string,
    userMessageId: string,
    options?: { turnId?: string | null },
  ) => void;
  loadProjects: () => Promise<unknown>;
  loadAiModelConfigs: () => Promise<void>;
  loadAgents: () => Promise<void>;
  loadContactMemoryContext: (sessionId: string, force?: boolean) => Promise<unknown>;
  loadSessionMemorySummaries: (sessionId: string, force?: boolean) => Promise<unknown>;
  applyRealtimeSessionMemorySummaries: (
    sessionId: string,
    payload: RealtimeConversationSummariesUpdatedPayloadWrapper,
  ) => void;
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
  messages,
  currentMessageCount,
  currentSessionHasMoreMessages,
  runtimeContextRefreshNonce,
  currentChatStateActiveTurnId,
  activeConversationTurnId,
  currentRemoteConnectionId,
  uiPromptHistoryOpen,
  setUiPromptHistoryOpen,
  summaryPaneSessionId,
  setSummaryPaneSessionId,
  closeTurnProcessViewer,
  setTaskHistoryOpen,
  onMessageSend,
  sendMessage,
  selectRemoteConnection,
  submitRuntimeGuidance,
  loadMoreMessages,
  openTurnProcessViewer,
  loadProjects,
  loadAiModelConfigs,
  loadAgents,
  loadContactMemoryContext,
  loadSessionMemorySummaries,
  applyRealtimeSessionMemorySummaries,
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
  const overlayState = useChatInterfaceOverlayState();

  const { sessionSummaryPaneVisible } = useChatSessionEffects({
    activePanel,
    currentSession,
    uiPromptHistoryOpen,
    summaryPaneSessionId,
    closeTurnProcessViewer,
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

  const reviewRepairAutoLoad = Boolean(
    activePanel === 'chat'
    && currentSession?.id
    && summaryPaneSessionId === currentSession.id,
  );

  const {
    reviewRepairRunning,
    reviewRepairPendingCount,
    refreshReviewRepairStatus,
    markReviewRepairStarting,
  } = useReviewRepairRealtime({
    apiClient,
    sessionId: activePanel === 'chat' ? (currentSession?.id || null) : null,
    enabled: activePanel === 'chat',
    autoLoad: reviewRepairAutoLoad,
    messageCountHint: activePanel === 'chat' && currentSession?.id
      ? currentMessageCount
      : undefined,
    onCompleted: async () => {
      if (!currentSession?.id) {
        return;
      }
      await loadMoreMessages(currentSession.id);
      await refreshReviewRepairStatus(currentSession.id).catch((error) => {
        console.error('Failed to refresh review repair status after completion:', error);
      });
    },
  });

  const loadedReviewRepairPendingCount = activePanel === 'chat' && currentSession?.id
    ? countPendingReviewRepairMessages(messages, currentSession.id)
    : 0;
  const effectiveReviewRepairPendingCount = reviewRepairPendingCount ?? 0;
  const reviewRepairDisabled = !reviewRepairRunning
    && effectiveReviewRepairPendingCount === 0
    && loadedReviewRepairPendingCount === 0
    && !currentSessionHasMoreMessages;

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
  }, [
    currentRemoteConnectionId,
    currentSession?.id,
    onMessageSend,
    sendMessage,
  ]);

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
    const sessionId = currentSession?.id || '';
    const normalizedUserMessageId = typeof userMessageId === 'string' ? userMessageId.trim() : '';
    if (!sessionId || !normalizedUserMessageId) {
      return;
    }
    const userMessage = messages.find((message) => (
      message.role === 'user' && message.id === normalizedUserMessageId
    ));
    const turnId = typeof userMessage?.metadata?.conversation_turn_id === 'string'
      ? userMessage.metadata.conversation_turn_id.trim()
      : (typeof userMessage?.metadata?.historyProcess?.turnId === 'string'
        ? userMessage.metadata.historyProcess.turnId.trim()
        : '');

    openTurnProcessViewer(sessionId, normalizedUserMessageId, { turnId: turnId || null });
  }, [currentSession?.id, messages, openTurnProcessViewer]);

  const handleRefreshMemory = useCallback((sessionId: string) => {
    void loadContactMemoryContext(sessionId, true);
  }, [loadContactMemoryContext]);

  useConversationSummariesRealtime({
    sessionId: activePanel === 'chat' ? (currentSession?.id || null) : null,
    enabled: activePanel === 'chat',
    onEvent: async (payload) => {
      const sessionId = currentSession?.id || null;
      if (!sessionId) {
        return;
      }
      if (Array.isArray(payload?.items)) {
        applyRealtimeSessionMemorySummaries(sessionId, payload);
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
    ...overlayState,
    summaryPaneSessionId,
    setSummaryPaneSessionId,
    sessionSummaryPaneVisible,
    uiPromptHistoryOpen,
    setUiPromptHistoryOpen,
    reviewRepairRunning,
    reviewRepairPendingCount,
    reviewRepairDisabled,
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
