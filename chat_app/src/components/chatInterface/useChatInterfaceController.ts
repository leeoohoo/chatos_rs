// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import type ApiClient from '../../lib/api/client';
import type {
  SendMessageHandler,
  SendMessageRuntimeOptions,
  Session,
} from '../../types';
import { useConversationSummariesRealtime } from '../../lib/realtime/useConversationSummariesRealtime';
import type { RealtimeConversationSummariesUpdatedPayloadWrapper } from '../../lib/realtime/types';
import { useChatInterfaceOverlayState } from './useChatInterfaceOverlayState';
import { useChatSessionEffects } from './useChatSessionEffects';
import { useRuntimeContextState } from './useRuntimeContextState';

interface UseChatInterfaceControllerParams {
  apiClient: ApiClient;
  activePanel: string;
  currentSession: Session | null;
  runtimeContextRefreshNonce: number;
  currentRemoteConnectionId: string | null;
  summaryPaneSessionId: string | null;
  setSummaryPaneSessionId: (value: string | null | ((prev: string | null) => string | null)) => void;
  onMessageSend?: (content: string, attachments?: File[]) => void;
  sendMessage: SendMessageHandler;
  selectRemoteConnection: (
    connectionId: string | null,
    options?: { activatePanel?: boolean },
  ) => Promise<void>;
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
  runtimeContextRefreshNonce,
  currentRemoteConnectionId,
  summaryPaneSessionId,
  setSummaryPaneSessionId,
  onMessageSend,
  sendMessage,
  selectRemoteConnection,
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
    onMessageSend,
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
    handleMessageSend,
    handleComposerRemoteConnectionChange,
    handleLoadMore,
    handleRefreshMemory,
    handleCloseSummary,
    handleOpenRuntimeContext,
    handleRefreshRuntimeContext,
  };
};
