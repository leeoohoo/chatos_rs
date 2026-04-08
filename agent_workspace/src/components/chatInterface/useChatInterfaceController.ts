import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type ApiClient from '../../lib/api/client';
import { readSessionImConversationId } from '../../lib/store/helpers/sessionRuntime';
import type { TaskReviewPanelState, UiPromptPanelState } from '../../lib/store/types';
import type { Session } from '../../types';
import { useImConversationSync } from './useImConversationSync';

interface UseChatInterfaceControllerParams {
  apiClient: ApiClient;
  activePanel: string;
  currentSession: Session | null;
  currentRuntimeSessionId: string | null;
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
    },
  ) => Promise<void>;
  selectRemoteConnection: (
    connectionId: string | null,
    options?: { activatePanel?: boolean },
  ) => Promise<void>;
  submitRuntimeGuidance: (
    content: string,
    options: { sessionId: string; turnId: string; projectId?: string | null },
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
  taskReviewPanelsBySession: Record<string, TaskReviewPanelState[] | undefined>;
  uiPromptPanelsBySession: Record<string, UiPromptPanelState[] | undefined>;
  upsertTaskReviewPanel: (panel: TaskReviewPanelState) => void;
  removeTaskReviewPanel: (reviewId: string, sessionId?: string) => void;
  upsertUiPromptPanel: (panel: UiPromptPanelState) => void;
  removeUiPromptPanel: (promptId: string, sessionId?: string) => void;
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
  currentRuntimeSessionId,
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
  taskReviewPanelsBySession,
  uiPromptPanelsBySession,
  upsertTaskReviewPanel,
  removeTaskReviewPanel,
  upsertUiPromptPanel,
  removeUiPromptPanel,
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

  const didInitRef = useRef(false);
  const lastHydratedChatSessionRef = useRef<string | null>(null);

  const currentSessionIdForUiPrompts = currentRuntimeSessionId;
  const currentImConversationId = useMemo(
    () => readSessionImConversationId(currentSession?.metadata),
    [currentSession?.metadata],
  );
  const currentTaskReviewPanels = useMemo(
    () => (
      currentSessionIdForUiPrompts
        ? (taskReviewPanelsBySession?.[currentSessionIdForUiPrompts] || [])
        : []
    ),
    [currentSessionIdForUiPrompts, taskReviewPanelsBySession],
  );
  const currentUiPromptPanels = useMemo(
    () => (
      currentSessionIdForUiPrompts
        ? (uiPromptPanelsBySession?.[currentSessionIdForUiPrompts] || [])
        : []
    ),
    [currentSessionIdForUiPrompts, uiPromptPanelsBySession],
  );
  const sessionSummaryPaneVisible = useMemo(
    () => Boolean(
      activePanel === 'chat'
      && currentRuntimeSessionId
      && summaryPaneSessionId === currentRuntimeSessionId
    ),
    [activePanel, currentRuntimeSessionId, summaryPaneSessionId],
  );

  const fallbackTurnId = useMemo(
    () => (
      activeConversationTurnId
      || currentChatStateActiveTurnId
      || ''
    ).trim(),
    [activeConversationTurnId, currentChatStateActiveTurnId],
  );

  useImConversationSync({
    apiClient,
    activePanel,
    currentSessionId: currentRuntimeSessionId,
    currentImConversationId,
    fallbackTurnId,
    taskReviewPanels: currentTaskReviewPanels,
    uiPromptPanels: currentUiPromptPanels,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
  });

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
    if (!currentSession || !currentRuntimeSessionId || activePanel !== 'chat') {
      cancelPendingMemoryLoad();
      cancelPendingUiPromptHistoryLoad();
      lastHydratedChatSessionRef.current = null;
      resetAllWorkbarState();
      resetMemoryState();
      resetUiPromptHistoryState();
      setUiPromptHistoryOpen(false);
      return;
    }

    const sessionChanged = lastHydratedChatSessionRef.current !== currentRuntimeSessionId;
    if (sessionChanged) {
      lastHydratedChatSessionRef.current = currentRuntimeSessionId;
      cancelPendingMemoryLoad();
      cancelPendingUiPromptHistoryLoad();
      resetHistoryWorkbarState();
      resetMemoryState();
      hydrateUiPromptHistoryFromCache(currentRuntimeSessionId);
    }

    if (sessionSummaryPaneVisible) {
      void loadContactMemoryContext(currentRuntimeSessionId);
    }
    if (uiPromptHistoryOpen) {
      void loadUiPromptHistory(currentRuntimeSessionId);
    }
  }, [
    activePanel,
    cancelPendingMemoryLoad,
    cancelPendingUiPromptHistoryLoad,
    currentSession,
    currentRuntimeSessionId,
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
    const sessionId = currentRuntimeSessionId;
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
      await submitRuntimeGuidance(content, { sessionId, turnId, projectId });
    } catch (error) {
      console.error('Failed to submit runtime guidance:', error);
    }
  }, [
    activeConversationTurnId,
    currentChatStateActiveTurnId,
    currentRuntimeSessionId,
    currentSession,
    submitRuntimeGuidance,
  ]);

  const handleLoadMore = useCallback(() => {
    if (currentRuntimeSessionId) {
      void loadMoreMessages(currentRuntimeSessionId);
    }
  }, [currentRuntimeSessionId, loadMoreMessages]);

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
    handleMessageSend,
    handleComposerRemoteConnectionChange,
    handleRuntimeGuidanceSend,
    handleLoadMore,
    handleToggleTurnProcess,
    handleRefreshMemory,
    handleCloseSummary,
    handleOpenHistory,
    handleOpenUiPromptHistory,
  };
};
