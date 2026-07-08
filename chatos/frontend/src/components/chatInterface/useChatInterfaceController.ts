// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type ApiClient from '../../lib/api/client';
import { countPendingReviewRepairMessages } from '../../lib/domain/reviewRepair';
import type {
  Message,
  SendMessageHandler,
  SendMessageRuntimeOptions,
  Session,
} from '../../types';
import { useConversationSummariesRealtime } from '../../lib/realtime/useConversationSummariesRealtime';
import type { RealtimeConversationSummariesUpdatedPayloadWrapper } from '../../lib/realtime/types';
import { useReviewRepairRealtime } from '../../lib/realtime/useReviewRepairRealtime';
import { useChatInterfaceOverlayState } from './useChatInterfaceOverlayState';
import { useChatSessionEffects } from './useChatSessionEffects';
import { useRuntimeContextState } from './useRuntimeContextState';

interface UseChatInterfaceControllerParams {
  apiClient: ApiClient;
  activePanel: string;
  currentSession: Session | null;
  messages: Message[];
  runtimeContextRefreshNonce: number;
  summaryPaneSessionId: string | null;
  setSummaryPaneSessionId: (value: string | null | ((prev: string | null) => string | null)) => void;
  onMessageSend?: (content: string, attachments?: File[]) => void;
  sendMessage: SendMessageHandler;
  flushRuntimeSettings?: () => Promise<void>;
  selectRemoteConnection: (
    connectionId: string | null,
    options?: { activatePanel?: boolean },
  ) => Promise<void>;
  loadMessages: (sessionId: string) => Promise<void>;
  loadMoreMessages: (sessionId: string) => Promise<void>;
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
}

export const useChatInterfaceController = ({
  apiClient,
  activePanel,
  currentSession,
  messages,
  runtimeContextRefreshNonce,
  summaryPaneSessionId,
  setSummaryPaneSessionId,
  onMessageSend,
  sendMessage,
  flushRuntimeSettings,
  selectRemoteConnection,
  loadMessages,
  loadMoreMessages,
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
}: UseChatInterfaceControllerParams) => {
  const { t } = useI18n();
  const overlayState = useChatInterfaceOverlayState();

  const { sessionSummaryPaneVisible } = useChatSessionEffects({
    activePanel,
    currentSession,
    summaryPaneSessionId,
    loadProjects,
    loadAiModelConfigs,
    loadAgents,
    loadContactMemoryContext,
    hydrateContactMemoryContextFromCache,
    resetMemoryState,
    cancelPendingMemoryLoad,
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

  const {
    reviewRepairRunning,
    reviewRepairPendingCount,
    refreshReviewRepairStatus,
    markReviewRepairStarting,
  } = useReviewRepairRealtime({
    apiClient,
    sessionId: activePanel === 'chat' ? (currentSession?.id || null) : null,
    enabled: activePanel === 'chat',
    messageCountHint: activePanel === 'chat' && currentSession?.id ? messages.length : undefined,
    onCompleted: async () => {
      const sessionId = currentSession?.id || null;
      if (!sessionId) {
        return;
      }
      await loadMessages(sessionId);
      markContactMemoryContextStale(sessionId);
      hydrateContactMemoryContextFromCache(sessionId);
      if (sessionSummaryPaneVisible) {
        await loadSessionMemorySummaries(sessionId, true);
      }
    },
    onFailed: (errorMessage) => {
      console.error('Review repair failed:', errorMessage);
    },
  });

  const loadedReviewRepairPendingCount = activePanel === 'chat' && currentSession?.id
    ? countPendingReviewRepairMessages(messages, currentSession.id)
    : 0;
  const reviewRepairDisabled = !reviewRepairRunning
    && reviewRepairPendingCount === 0
    && loadedReviewRepairPendingCount === 0;

  const handleMessageSend = useCallback(async (
    content: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => {
    try {
      await flushRuntimeSettings?.();
      await sendMessage(content, attachments, runtimeOptions);
      if (currentSession?.id) {
        void refreshReviewRepairStatus(currentSession.id).catch((statusError) => {
          console.error('Failed to refresh review repair status after send:', statusError);
        });
      }
      onMessageSend?.(content, attachments);
    } catch (error) {
      console.error('Failed to send message:', error);
    }
  }, [
    flushRuntimeSettings,
    currentSession?.id,
    onMessageSend,
    refreshReviewRepairStatus,
    sendMessage,
  ]);

  const handleComposerRemoteConnectionChange = useCallback((connectionId: string | null) => {
    void selectRemoteConnection(connectionId, { activatePanel: false });
  }, [selectRemoteConnection]);

  const handleLoadMore = useCallback(async () => {
    if (currentSession) {
      await loadMoreMessages(currentSession.id);
    }
  }, [currentSession, loadMoreMessages]);

  const handleRefreshMemory = useCallback((sessionId: string) => {
    void loadContactMemoryContext(sessionId, true);
  }, [loadContactMemoryContext]);

  const handleRunReviewRepair = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    markReviewRepairStarting();
    try {
      const result = await apiClient.runConversationReviewRepair(sessionId);
      if (result?.success === false) {
        throw new Error(result.detail || result.error || t('taskWorkbar.reviewRepairFailed'));
      }
    } catch (error) {
      await refreshReviewRepairStatus(sessionId).catch((statusError) => {
        console.error('Failed to refresh review repair status after run error:', statusError);
      });
      console.error('Failed to run review repair:', error);
    }
  }, [
    apiClient,
    markReviewRepairStarting,
    refreshReviewRepairStatus,
    t,
  ]);

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

  const handleCloseSummary = useCallback(() => {
    setSummaryPaneSessionId(null);
  }, [setSummaryPaneSessionId]);

  return {
    ...overlayState,
    summaryPaneSessionId,
    setSummaryPaneSessionId,
    sessionSummaryPaneVisible,
    runtimeContextOpen,
    setRuntimeContextOpen,
    runtimeContextSessionId,
    runtimeContextData,
    runtimeContextLoading,
    runtimeContextError,
    reviewRepairRunning,
    reviewRepairPendingCount,
    reviewRepairDisabled,
    handleMessageSend,
    handleComposerRemoteConnectionChange,
    handleLoadMore,
    handleRefreshMemory,
    handleRunReviewRepair,
    handleCloseSummary,
    handleOpenRuntimeContext,
    handleRefreshRuntimeContext,
  };
};
