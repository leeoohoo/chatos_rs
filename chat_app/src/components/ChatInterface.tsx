import React, { useMemo } from 'react';
import { shallow } from 'zustand/shallow';
import { useChatApiClientFromContext, useChatStoreSelector } from '../lib/store/ChatStoreContext';
import SystemContextEditor from './SystemContextEditor';
import ChatInterfaceErrorBanner from './chatInterface/ChatInterfaceErrorBanner';
import HeaderBar from './chatInterface/HeaderBar';
import ChatInterfaceMainContent from './chatInterface/ChatInterfaceMainContent';
import ChatInterfaceOverlays from './chatInterface/ChatInterfaceOverlays';
import {
  buildSupportedFileTypes,
  formatSummaryCreatedAt,
  resolveModelSupportFlags,
} from './chatInterface/helpers';
import { useChatInterfaceController } from './chatInterface/useChatInterfaceController';
import { useSessionHeaderMeta } from './chatInterface/useSessionHeaderMeta';
import { useSessionWorkbarPanels } from './chatInterface/useSessionWorkbarPanels';
import { apiClient as globalApiClient } from '../lib/api/client';
import { cn } from '../lib/utils';
import type { ChatInterfaceProps } from '../types';
import { useAuthStore } from '../lib/auth/authStore';
import { useSessionRuntimeSettings } from '../features/sessionRuntime/useSessionRuntimeSettings';
import { useContactMemoryContext } from './chatInterface/useContactMemoryContext';
import { useUiPromptHistory } from './chatInterface/useUiPromptHistory';
import { useContactProjectScope } from './chatInterface/useContactProjectScope';

export const ChatInterface: React.FC<ChatInterfaceProps> = ({
  className,
  onMessageSend,
  customRenderer,
}) => {
  const {
    currentSession,
    contacts,
    currentProject,
    currentTerminal,
    currentRemoteConnection,
    remoteConnections,
    projects,
    activePanel,
    messages,
    hasMoreMessages,
    error,
    loadProjects,
    loadMoreMessages,
    toggleTurnProcess,
    sendMessage,
    selectRemoteConnection,
    updateSession,
    clearError,
    sidebarOpen,
    toggleSidebar,
    aiModelConfigs,
    selectedModelId,
    setSelectedModel,
    loadAiModelConfigs,
    loadAgents,
    chatConfig,
    updateChatConfig,
    abortCurrentConversation,
    sessionChatState = {},
    sessionRuntimeGuidanceState = {},
    taskReviewPanelsBySession = {},
    uiPromptPanelsBySession = {},
    submitRuntimeGuidance,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    // applications,  // 涓嶅啀鍦ㄦ缁勪欢涓娇鐢?
    // selectedApplicationId,  // 涓嶅啀鐢ㄤ簬鑷姩鏄剧ず
  } = useChatStoreSelector((state) => ({
    currentSession: state.currentSession,
    contacts: state.contacts,
    currentProject: state.currentProject,
    currentTerminal: state.currentTerminal,
    currentRemoteConnection: state.currentRemoteConnection,
    remoteConnections: state.remoteConnections,
    projects: state.projects,
    activePanel: state.activePanel,
    messages: state.messages,
    hasMoreMessages: state.hasMoreMessages,
    error: state.error,
    loadProjects: state.loadProjects,
    loadMoreMessages: state.loadMoreMessages,
    toggleTurnProcess: state.toggleTurnProcess,
    sendMessage: state.sendMessage,
    selectRemoteConnection: state.selectRemoteConnection,
    updateSession: state.updateSession,
    clearError: state.clearError,
    sidebarOpen: state.sidebarOpen,
    toggleSidebar: state.toggleSidebar,
    aiModelConfigs: state.aiModelConfigs,
    selectedModelId: state.selectedModelId,
    setSelectedModel: state.setSelectedModel,
    loadAiModelConfigs: state.loadAiModelConfigs,
    loadAgents: state.loadAgents,
    chatConfig: state.chatConfig,
    updateChatConfig: state.updateChatConfig,
    abortCurrentConversation: state.abortCurrentConversation,
    sessionChatState: state.sessionChatState,
    sessionRuntimeGuidanceState: state.sessionRuntimeGuidanceState,
    taskReviewPanelsBySession: state.taskReviewPanelsBySession,
    uiPromptPanelsBySession: state.uiPromptPanelsBySession,
    submitRuntimeGuidance: state.submitRuntimeGuidance,
    upsertTaskReviewPanel: state.upsertTaskReviewPanel,
    removeTaskReviewPanel: state.removeTaskReviewPanel,
    upsertUiPromptPanel: state.upsertUiPromptPanel,
    removeUiPromptPanel: state.removeUiPromptPanel,
  }), shallow);

  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);
  const { user, logout } = useAuthStore();

  const { supportsImages, supportsReasoning } = useMemo(
    () => resolveModelSupportFlags(selectedModelId, aiModelConfigs as any[]),
    [aiModelConfigs, selectedModelId],
  );
  const supportedFileTypes = useMemo(
    () => buildSupportedFileTypes(supportsImages),
    [supportsImages],
  );
  const currentChatState = useMemo(() => (
    currentSession ? sessionChatState[currentSession.id] : undefined
  ), [currentSession, sessionChatState]);
  const {
    currentContactName,
    currentContactId,
    headerTitle,
  } = useSessionHeaderMeta({
    currentSession,
    contacts: contacts as any[],
    activePanel,
    currentProject,
    currentTerminal,
    currentRemoteConnection,
  });
  const chatIsLoading = currentChatState?.isLoading ?? false;
  const chatIsStreaming = currentChatState?.isStreaming ?? false;
  const chatIsStopping = currentChatState?.isStopping ?? false;
  const chatStreamingPreviewText = currentChatState?.streamingPreviewText || '';

  const {
    workspaceRoot: composerWorkspaceRoot,
    mcpEnabled: composerMcpEnabled,
    enabledMcpIds: composerEnabledMcpIds,
    setWorkspaceRoot: handleComposerWorkspaceRootChange,
    setMcpEnabled: handleComposerMcpEnabledChange,
    setEnabledMcpIds: handleComposerEnabledMcpIdsChange,
  } = useSessionRuntimeSettings({
    session: currentSession,
    updateSession,
  });
  const {
    currentProjectIdForMemory,
    currentProjectNameForMemory,
    composerAvailableProjects,
    handleComposerProjectChange,
  } = useContactProjectScope({
    apiClient,
    currentSession: currentSession as any,
    currentContactId,
    projects: projects as any[],
  });
  const {
    sessionMemorySummaries,
    agentRecalls,
    memoryLoading,
    memoryError,
    loadContactMemoryContext,
    resetMemoryState,
    cancelPendingMemoryLoad,
  } = useContactMemoryContext({
    apiClient,
    currentSessionId: currentSession?.id || null,
    currentContactId,
    currentProjectIdForMemory,
  });

  const currentSessionIdForUiPrompts = currentSession?.id || null;
  const {
    uiPromptHistoryItems,
    uiPromptHistoryLoading,
    uiPromptHistoryError,
    loadUiPromptHistory,
    resetUiPromptHistoryState,
    hydrateUiPromptHistoryFromCache,
    cancelPendingUiPromptHistoryLoad,
  } = useUiPromptHistory({
    apiClient,
    currentSessionId: currentSessionIdForUiPrompts,
  });

  const {
    activeConversationTurnId,
    activeTaskReviewPanel,
    activeUiPromptPanel,
    handleOpenWorkbarHistory,
    handleRefreshWorkbar,
    handleTaskReviewCancel,
    handleTaskReviewConfirm,
    handleUiPromptCancel,
    handleUiPromptSubmit,
    handleWorkbarCompleteTask,
    handleWorkbarDeleteTask,
    handleWorkbarEditTask,
    mergedCurrentTurnTasks,
    resetAllWorkbarState,
    resetHistoryWorkbarState,
    runtimeGuidanceAppliedCount,
    runtimeGuidanceItems,
    runtimeGuidanceLastAppliedAt,
    runtimeGuidancePendingCount,
    workbarActionLoadingTaskId,
    workbarError,
    workbarHistoryError,
    workbarHistoryLoading,
    workbarHistoryTasks,
    workbarLoading,
  } = useSessionWorkbarPanels({
    apiClient,
    session: currentSession,
    enabled: activePanel === 'chat',
    messages: messages as any[],
    selectedSessionActiveTurnId: currentChatState?.activeTurnId || null,
    sessionRuntimeGuidanceState,
    taskReviewPanelsBySession,
    uiPromptPanelsBySession,
    upsertTaskReviewPanel,
    removeTaskReviewPanel,
    upsertUiPromptPanel,
    removeUiPromptPanel,
    loadWorkbarSummaries: loadContactMemoryContext,
    loadUiPromptHistory,
  });
  const {
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
  } = useChatInterfaceController({
    apiClient,
    activePanel,
    currentSession,
    currentChatStateActiveTurnId: currentChatState?.activeTurnId,
    activeConversationTurnId,
    currentRemoteConnectionId: currentRemoteConnection?.id || null,
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
  });

  const sessionListProps = {
    onSelectSession: () => undefined,
    onOpenSessionSummary: (_sessionId: string) => undefined,
    onOpenSessionRuntimeContext: (_sessionId: string) => undefined,
    activeSummarySessionId: null,
    activeRuntimeContextSessionId: null,
  };

  const conversationPaneProps = {
    currentSession,
    sessionSummaryPaneVisible,
    currentContactName,
    currentProjectNameForMemory,
    currentProjectIdForMemory: currentProjectIdForMemory || null,
    messages,
    chatIsLoading,
    chatIsStreaming,
    chatIsStopping,
    chatStreamingPreviewText,
    hasMoreMessages,
    onLoadMore: handleLoadMore,
    onToggleTurnProcess: handleToggleTurnProcess,
    customRenderer,
    sessionMemorySummaries,
    agentRecalls,
    memoryLoading,
    memoryError,
    onRefreshMemory: handleRefreshMemory,
    onCloseSummary: handleCloseSummary,
    toggleSidebar,
    mergedCurrentTurnTasks,
    workbarHistoryTasks,
    activeConversationTurnId,
    workbarLoading,
    workbarHistoryLoading,
    workbarError,
    workbarHistoryError,
    workbarActionLoadingTaskId,
    onRefreshWorkbarTasks: handleRefreshWorkbar,
    onOpenHistory: handleOpenHistory,
    onOpenUiPromptHistory: handleOpenUiPromptHistory,
    uiPromptHistoryCount: uiPromptHistoryItems.length,
    uiPromptHistoryLoading,
    onCompleteTask: (task: any) => {
      void handleWorkbarCompleteTask(task);
    },
    onDeleteTask: (task: any) => {
      void handleWorkbarDeleteTask(task);
    },
    onEditTask: (task: any) => {
      void handleWorkbarEditTask(task);
    },
    activeUiPromptPanel,
    onUiPromptSubmit: handleUiPromptSubmit,
    onUiPromptCancel: handleUiPromptCancel,
    activeTaskReviewPanel,
    onTaskReviewConfirm: handleTaskReviewConfirm,
    onTaskReviewCancel: handleTaskReviewCancel,
    onSend: handleMessageSend,
    onGuide: handleRuntimeGuidanceSend,
    onStop: abortCurrentConversation,
    inputDisabled: chatIsStopping || !currentSession,
    isStreaming: chatIsStreaming,
    isStopping: chatIsStopping,
    supportedFileTypes,
    supportsReasoning,
    reasoningEnabled: chatConfig?.reasoningEnabled === true,
    onReasoningToggle: (enabled: boolean) => updateChatConfig({ reasoningEnabled: enabled }),
    selectedModelId,
    availableModels: aiModelConfigs,
    onModelChange: setSelectedModel,
    availableProjects: composerAvailableProjects,
    currentProject,
    onProjectChange: handleComposerProjectChange,
    workspaceRoot: composerWorkspaceRoot,
    onWorkspaceRootChange: handleComposerWorkspaceRootChange,
    currentRemoteConnectionId: currentRemoteConnection?.id || null,
    availableRemoteConnections: remoteConnections || [],
    onRemoteConnectionChange: handleComposerRemoteConnectionChange,
    mcpEnabled: composerMcpEnabled,
    enabledMcpIds: composerEnabledMcpIds,
    onMcpEnabledChange: handleComposerMcpEnabledChange,
    onEnabledMcpIdsChange: handleComposerEnabledMcpIdsChange,
    runtimeGuidancePendingCount,
    runtimeGuidanceAppliedCount,
    runtimeGuidanceLastAppliedAt,
    runtimeGuidanceItems,
  };

  const uiPromptHistoryProps = {
    open: uiPromptHistoryOpen,
    items: uiPromptHistoryItems,
    loading: uiPromptHistoryLoading,
    error: uiPromptHistoryError,
    refreshDisabled: !currentSession || uiPromptHistoryLoading,
    onRefresh: () => {
      if (!currentSession) {
        return;
      }
      void loadUiPromptHistory(currentSession.id, true);
    },
    onClose: () => setUiPromptHistoryOpen(false),
    formatCreatedAt: formatSummaryCreatedAt,
  };

  const runtimeContextProps = {
    open: runtimeContextOpen,
    sessionId: runtimeContextSessionId,
    loading: runtimeContextLoading,
    error: runtimeContextError,
    data: runtimeContextData,
    onRefresh: handleRefreshRuntimeContext,
    onClose: () => setRuntimeContextOpen(false),
  };

  if (showSystemContextEditor) {
    return (
      <SystemContextEditor onClose={() => setShowSystemContextEditor(false)} />
    );
  }

  return (
    <div className={cn(
      'flex flex-col h-screen bg-background text-foreground',
      className
    )}>
      <HeaderBar
        headerTitle={headerTitle}
        sidebarOpen={sidebarOpen}
        onToggleSidebar={toggleSidebar}
        onOpenNotepad={() => setShowNotepadPanel(true)}
        onOpenApplications={() => setShowApplicationsPanel(true)}
        onOpenMcpManager={() => setShowMcpManager(true)}
        onOpenAiModelManager={() => setShowAiModelManager(true)}
        onOpenSystemContextEditor={() => setShowSystemContextEditor(true)}
        onOpenUserSettings={() => setShowUserSettings(true)}
        onLogout={logout}
        user={user}
      />

      <ChatInterfaceErrorBanner error={error} onClear={clearError} />

      <ChatInterfaceMainContent
        activePanel={activePanel}
        sidebarOpen={sidebarOpen}
        summaryPaneSessionId={summaryPaneSessionId}
        runtimeContextOpen={runtimeContextOpen}
        runtimeContextSessionId={runtimeContextSessionId}
        currentProject={currentProject}
        onToggleSidebar={toggleSidebar}
        onSelectSession={() => setSummaryPaneSessionId(null)}
        onToggleSessionSummary={(sessionId) => {
          setSummaryPaneSessionId((prev) => (prev === sessionId ? null : sessionId));
        }}
        onOpenSessionRuntimeContext={handleOpenRuntimeContext}
        sessionListProps={sessionListProps}
        conversationPaneProps={conversationPaneProps}
      />

      <ChatInterfaceOverlays
        uiPromptHistoryProps={uiPromptHistoryProps}
        runtimeContextProps={runtimeContextProps}
        showMcpManager={showMcpManager}
        setShowMcpManager={setShowMcpManager}
        showNotepadPanel={showNotepadPanel}
        setShowNotepadPanel={setShowNotepadPanel}
        showAiModelManager={showAiModelManager}
        setShowAiModelManager={setShowAiModelManager}
        showUserSettings={showUserSettings}
        setShowUserSettings={setShowUserSettings}
        showApplicationsPanel={showApplicationsPanel}
        setShowApplicationsPanel={setShowApplicationsPanel}
      />
    </div>
  );
};

export default ChatInterface;
