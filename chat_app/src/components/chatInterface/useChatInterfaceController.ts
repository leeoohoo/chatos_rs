import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import type { TurnRuntimeSnapshotLookupResponse } from '../../lib/api/client/types';
import type { UiPromptPanelState } from '../../lib/store/types';
import type { Session } from '../../types';
import { toUiPromptPanelFromRecord } from './helpers';

interface UseChatInterfaceControllerParams {
  apiClient: ApiClient;
  activePanel: string;
  currentSession: Session | null;
  currentChatStateActiveTurnId: string | null | undefined;
  activeConversationTurnId: string | null | undefined;
  currentRemoteConnectionId: string | null;
  onMessageSend?: (content: string, attachments?: File[]) => void;
  sendMessage: (
    content: string,
    attachments?: File[],
	    runtimeOptions?: {
	      mcpEnabled?: boolean;
	      remoteConnectionId?: string | null;
	      projectId?: string | null;
	      projectRoot?: string | null;
	      workspaceRoot?: string | null;
	      enabledMcpIds?: string[];
	      skillsEnabled?: boolean;
	      selectedSkillIds?: string[];
	    },
	  ) => Promise<void>;
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
  const [runtimeContextOpen, setRuntimeContextOpen] = useState(false);
  const [runtimeContextSessionId, setRuntimeContextSessionId] = useState<string | null>(null);
  const [runtimeContextData, setRuntimeContextData] =
    useState<TurnRuntimeSnapshotLookupResponse | null>(null);
  const [runtimeContextLoading, setRuntimeContextLoading] = useState(false);
  const [runtimeContextError, setRuntimeContextError] = useState<string | null>(null);

  const didInitRef = useRef(false);
  const lastHydratedChatSessionRef = useRef<string | null>(null);

  const currentSessionIdForUiPrompts = currentSession?.id || null;
  const sessionSummaryPaneVisible = useMemo(
    () => Boolean(activePanel === 'chat' && currentSession && summaryPaneSessionId === currentSession.id),
    [activePanel, currentSession, summaryPaneSessionId],
  );

  useEffect(() => {
    if (!currentSessionIdForUiPrompts || activePanel !== 'chat') {
      return;
    }

    let cancelled = false;
    void apiClient
      .getPendingUiPrompts(currentSessionIdForUiPrompts, { limit: 50 })
      .then((records) => {
        if (cancelled || !Array.isArray(records)) {
          return;
        }
        records.forEach((record) => {
          const panel = toUiPromptPanelFromRecord(record);
          if (panel) {
            upsertUiPromptPanel(panel);
          }
        });
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [activePanel, apiClient, currentSessionIdForUiPrompts, upsertUiPromptPanel]);

  useEffect(() => {
    if (didInitRef.current) {
      return;
    }
    didInitRef.current = true;

    void loadProjects();
    void loadAiModelConfigs();
    void loadAgents();
  }, [loadProjects, loadAiModelConfigs, loadAgents]);

  useEffect(() => {
    if (!currentSession || activePanel !== 'chat') {
      cancelPendingMemoryLoad();
      cancelPendingUiPromptHistoryLoad();
      lastHydratedChatSessionRef.current = null;
      resetAllWorkbarState();
      resetMemoryState();
      resetUiPromptHistoryState();
      setUiPromptHistoryOpen(false);
      return;
    }

    const sessionChanged = lastHydratedChatSessionRef.current !== currentSession.id;
    if (sessionChanged) {
      lastHydratedChatSessionRef.current = currentSession.id;
      cancelPendingMemoryLoad();
      cancelPendingUiPromptHistoryLoad();
      resetHistoryWorkbarState();
      resetMemoryState();
      hydrateUiPromptHistoryFromCache(currentSession.id);
    }

    if (sessionSummaryPaneVisible) {
      void loadContactMemoryContext(currentSession.id);
    }
    if (uiPromptHistoryOpen) {
      void loadUiPromptHistory(currentSession.id);
    }
  }, [
    activePanel,
    cancelPendingMemoryLoad,
    cancelPendingUiPromptHistoryLoad,
    currentSession,
    hydrateUiPromptHistoryFromCache,
    loadContactMemoryContext,
    loadUiPromptHistory,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
    resetMemoryState,
    resetUiPromptHistoryState,
    sessionSummaryPaneVisible,
    uiPromptHistoryOpen,
  ]);

  const handleMessageSend = useCallback(async (
    content: string,
    attachments?: File[],
	    runtimeOptions?: {
	      mcpEnabled?: boolean;
	      remoteConnectionId?: string | null;
	      projectId?: string | null;
	      projectRoot?: string | null;
	      workspaceRoot?: string | null;
	      enabledMcpIds?: string[];
	      skillsEnabled?: boolean;
	      selectedSkillIds?: string[];
	    },
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

  const loadLatestRuntimeContext = useCallback(async (sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setRuntimeContextLoading(true);
    setRuntimeContextError(null);
    try {
      const payload = await apiClient.getConversationLatestTurnRuntimeContext(sessionId);
      setRuntimeContextData(payload);
    } catch (error) {
      console.error('Failed to load turn runtime context:', error);
      setRuntimeContextError(error instanceof Error ? error.message : '加载上下文失败');
    } finally {
      setRuntimeContextLoading(false);
    }
  }, [apiClient]);

  const handleOpenRuntimeContext = useCallback((sessionId: string) => {
    if (!sessionId) {
      return;
    }
    setRuntimeContextOpen(true);
    setRuntimeContextSessionId(sessionId);
    setRuntimeContextData(null);
    void loadLatestRuntimeContext(sessionId);
  }, [loadLatestRuntimeContext]);

  const handleRefreshRuntimeContext = useCallback(() => {
    if (!runtimeContextSessionId) {
      return;
    }
    void loadLatestRuntimeContext(runtimeContextSessionId);
  }, [loadLatestRuntimeContext, runtimeContextSessionId]);

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
    handleCloseSummary,
    handleOpenHistory,
    handleOpenUiPromptHistory,
    handleOpenRuntimeContext,
    handleRefreshRuntimeContext,
  };
};
