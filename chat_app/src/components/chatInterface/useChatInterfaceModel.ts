import { useCallback } from 'react';

import { useSessionRuntimeSettings } from '../../features/sessionRuntime/useSessionRuntimeSettings';
import type { ChatInterfaceProps } from '../../types';
import { useChatInterfaceController } from './useChatInterfaceController';
import { useChatInterfaceDerivedState } from './useChatInterfaceDerivedState';
import { useChatInterfaceSessionResources } from './useChatInterfaceSessionResources';
import { useChatInterfaceStoreBridge } from './useChatInterfaceStoreBridge';
import { useChatInterfaceViewProps } from './useChatInterfaceViewProps';
import type {
  ChatInterfaceConversationActions,
  ChatInterfaceConversationState,
  ChatInterfaceOverlayActions,
  ChatInterfaceOverlayState,
} from './viewPropsTypes';

interface UseChatInterfaceModelParams {
  onMessageSend?: ChatInterfaceProps['onMessageSend'];
  customRenderer?: ChatInterfaceProps['customRenderer'];
}

export const useChatInterfaceModel = ({
  onMessageSend,
  customRenderer,
}: UseChatInterfaceModelParams) => {
  const store = useChatInterfaceStoreBridge();

  const derived = useChatInterfaceDerivedState({
    currentSession: store.currentSession,
    contacts: store.contacts,
    agents: store.agents,
    selectedAgentId: store.selectedAgentId,
    selectedModelId: store.selectedModelId,
    aiModelConfigs: store.aiModelConfigs,
    activePanel: store.activePanel,
    currentProject: store.currentProject,
    currentTerminal: store.currentTerminal,
    currentRemoteConnection: store.currentRemoteConnection,
    sessionChatState: store.sessionChatState || {},
  });

  const runtimeSettings = useSessionRuntimeSettings({
    session: store.currentSession,
    updateSession: store.updateSession,
  });

  const resources = useChatInterfaceSessionResources({
    apiClient: store.apiClient,
    currentSession: store.currentSession,
    currentContactId: derived.currentContactId,
    currentChatStateActiveTurnId: derived.currentChatState?.activeTurnId || null,
    currentProject: store.currentProject,
    projects: store.projects,
    messages: store.messages,
    activePanel: store.activePanel,
    sessionRuntimeGuidanceState: store.sessionRuntimeGuidanceState || {},
    taskReviewPanelsBySession: store.taskReviewPanelsBySession || {},
    uiPromptPanelsBySession: store.uiPromptPanelsBySession || {},
    upsertTaskReviewPanel: store.upsertTaskReviewPanel,
    removeTaskReviewPanel: store.removeTaskReviewPanel,
    upsertUiPromptPanel: store.upsertUiPromptPanel,
    removeUiPromptPanel: store.removeUiPromptPanel,
  });

  const controller = useChatInterfaceController({
    apiClient: store.apiClient,
    activePanel: store.activePanel,
    currentSession: store.currentSession,
    runtimeContextRefreshNonce: derived.runtimeContextRefreshNonce,
    currentChatStateActiveTurnId: derived.currentChatState?.activeTurnId,
    activeConversationTurnId: resources.activeConversationTurnId,
    currentRemoteConnectionId: store.currentRemoteConnection?.id || null,
    onMessageSend,
    sendMessage: store.sendMessage,
    selectRemoteConnection: store.selectRemoteConnection,
    submitRuntimeGuidance: store.submitRuntimeGuidance,
    loadMoreMessages: store.loadMoreMessages,
    toggleTurnProcess: store.toggleTurnProcess,
    loadProjects: store.loadProjects,
    loadAiModelConfigs: store.loadAiModelConfigs,
    loadAgents: store.loadAgents,
    loadContactMemoryContext: resources.loadContactMemoryContext,
    resetMemoryState: resources.resetMemoryState,
    cancelPendingMemoryLoad: resources.cancelPendingMemoryLoad,
    loadUiPromptHistory: resources.loadUiPromptHistory,
    hydrateUiPromptHistoryFromCache: resources.hydrateUiPromptHistoryFromCache,
    resetUiPromptHistoryState: resources.resetUiPromptHistoryState,
    cancelPendingUiPromptHistoryLoad: resources.cancelPendingUiPromptHistoryLoad,
    upsertUiPromptPanel: store.upsertUiPromptPanel,
    resetAllWorkbarState: resources.resetAllWorkbarState,
    resetHistoryWorkbarState: resources.resetHistoryWorkbarState,
    handleOpenWorkbarHistory: resources.handleOpenWorkbarHistory,
  });

  const conversation: ChatInterfaceConversationState = {
    currentSession: store.currentSession,
    sessionSummaryPaneVisible: controller.sessionSummaryPaneVisible,
    currentContactName: derived.currentContactName,
    currentProjectNameForMemory: resources.currentProjectNameForMemory,
    currentProjectIdForMemory: resources.currentProjectIdForMemory,
    messages: store.messages,
    chatIsLoading: derived.chatIsLoading,
    chatIsStreaming: derived.chatIsStreaming,
    chatIsStopping: derived.chatIsStopping,
    chatStreamingPreviewText: derived.chatStreamingPreviewText,
    hasMoreMessages: store.hasMoreMessages,
    customRenderer,
    sessionMemorySummaries: resources.sessionMemorySummaries,
    agentRecalls: resources.agentRecalls,
    memoryLoading: resources.memoryLoading,
    memoryError: resources.memoryError,
    mergedCurrentTurnTasks: resources.mergedCurrentTurnTasks,
    workbarHistoryTasks: resources.workbarHistoryTasks,
    activeConversationTurnId: resources.activeConversationTurnId,
    workbarLoading: resources.workbarLoading,
    workbarHistoryLoading: resources.workbarHistoryLoading,
    workbarError: resources.workbarError,
    workbarHistoryError: resources.workbarHistoryError,
    workbarActionLoadingTaskId: resources.workbarActionLoadingTaskId,
    taskModalOpen: resources.taskModalOpen,
    taskModalMode: resources.taskModalMode,
    taskModalTask: resources.taskModalTask,
    taskModalError: resources.taskModalError,
    uiPromptHistoryItems: resources.uiPromptHistoryItems,
    uiPromptHistoryLoading: resources.uiPromptHistoryLoading,
    activeUiPromptPanel: resources.activeUiPromptPanel,
    activeTaskReviewPanel: resources.activeTaskReviewPanel,
    supportedFileTypes: derived.supportedFileTypes,
    supportsReasoning: derived.supportsReasoning,
    reasoningEnabled: store.chatConfig?.reasoningEnabled === true,
    selectedModelId: store.selectedModelId,
    currentAgent: derived.currentAgent,
    aiModelConfigs: store.aiModelConfigs,
    composerAvailableProjects: resources.composerAvailableProjects,
    currentProject: store.currentProject,
    composerWorkspaceRoot: runtimeSettings.workspaceRoot,
    currentRemoteConnectionId: store.currentRemoteConnection?.id || null,
    remoteConnections: store.remoteConnections || [],
    composerMcpEnabled: runtimeSettings.mcpEnabled,
    composerEnabledMcpIds: runtimeSettings.enabledMcpIds,
    runtimeGuidancePendingCount: resources.runtimeGuidancePendingCount,
    runtimeGuidanceAppliedCount: resources.runtimeGuidanceAppliedCount,
    runtimeGuidanceLastAppliedAt: resources.runtimeGuidanceLastAppliedAt,
    runtimeGuidanceItems: resources.runtimeGuidanceItems,
  };

  const conversationActions: ChatInterfaceConversationActions = {
    handleLoadMore: controller.handleLoadMore,
    handleToggleTurnProcess: controller.handleToggleTurnProcess,
    handleRefreshMemory: controller.handleRefreshMemory,
    handleCloseSummary: controller.handleCloseSummary,
    toggleSidebar: store.toggleSidebar,
    handleRefreshWorkbar: resources.handleRefreshWorkbar,
    handleOpenHistory: controller.handleOpenHistory,
    handleOpenUiPromptHistory: controller.handleOpenUiPromptHistory,
    handleWorkbarCompleteTask: resources.handleWorkbarCompleteTask,
    handleWorkbarDeleteTask: resources.handleWorkbarDeleteTask,
    handleWorkbarEditTask: resources.handleWorkbarEditTask,
    closeTaskModal: resources.closeTaskModal,
    submitTaskModal: resources.submitTaskModal,
    handleUiPromptSubmit: resources.handleUiPromptSubmit,
    handleUiPromptCancel: resources.handleUiPromptCancel,
    handleTaskReviewConfirm: resources.handleTaskReviewConfirm,
    handleTaskReviewCancel: resources.handleTaskReviewCancel,
    handleMessageSend: controller.handleMessageSend,
    handleRuntimeGuidanceSend: controller.handleRuntimeGuidanceSend,
    abortCurrentConversation: store.abortCurrentConversation,
    updateReasoningEnabled: (enabled: boolean) => store.updateChatConfig({ reasoningEnabled: enabled }),
    setSelectedModel: store.setSelectedModel,
    handleComposerProjectChange: resources.handleComposerProjectChange,
    handleComposerWorkspaceRootChange: runtimeSettings.setWorkspaceRoot,
    handleComposerRemoteConnectionChange: controller.handleComposerRemoteConnectionChange,
    handleComposerMcpEnabledChange: runtimeSettings.setMcpEnabled,
    handleComposerEnabledMcpIdsChange: runtimeSettings.setEnabledMcpIds,
  };

  const overlay: ChatInterfaceOverlayState = {
    currentSession: store.currentSession,
    currentSessionId: store.currentSession?.id || null,
    uiPromptHistoryOpen: controller.uiPromptHistoryOpen,
    uiPromptHistoryItems: resources.uiPromptHistoryItems,
    uiPromptHistoryLoading: resources.uiPromptHistoryLoading,
    uiPromptHistoryError: resources.uiPromptHistoryError,
    runtimeContextOpen: controller.runtimeContextOpen,
    runtimeContextSessionId: controller.runtimeContextSessionId,
    runtimeContextLoading: controller.runtimeContextLoading,
    runtimeContextError: controller.runtimeContextError,
    runtimeContextData: controller.runtimeContextData,
  };

  const overlayActions: ChatInterfaceOverlayActions = {
    loadUiPromptHistory: resources.loadUiPromptHistory,
    setUiPromptHistoryOpen: controller.setUiPromptHistoryOpen,
    handleRefreshRuntimeContext: controller.handleRefreshRuntimeContext,
    setRuntimeContextOpen: controller.setRuntimeContextOpen,
  };

  const {
    sessionListProps,
    conversationPaneProps,
    uiPromptHistoryProps,
    runtimeContextProps,
  } = useChatInterfaceViewProps({
    conversation,
    conversationActions,
    overlay,
    overlayActions,
  });

  const handleClearSummaryPaneSelection = useCallback(() => {
    controller.setSummaryPaneSessionId(null);
  }, [controller]);

  const handleToggleSessionSummary = useCallback((sessionId: string) => {
    controller.setSummaryPaneSessionId((prev) => (prev === sessionId ? null : sessionId));
  }, [controller]);

  return {
    user: store.user,
    logout: store.logout,
    error: store.error,
    clearError: store.clearError,
    headerTitle: derived.headerTitle,
    sidebarOpen: store.sidebarOpen,
    toggleSidebar: store.toggleSidebar,
    currentProject: store.currentProject,
    activePanel: store.activePanel,
    showSystemContextEditor: controller.showSystemContextEditor,
    setShowSystemContextEditor: controller.setShowSystemContextEditor,
    showMcpManager: controller.showMcpManager,
    setShowMcpManager: controller.setShowMcpManager,
    showAiModelManager: controller.showAiModelManager,
    setShowAiModelManager: controller.setShowAiModelManager,
    showNotepadPanel: controller.showNotepadPanel,
    setShowNotepadPanel: controller.setShowNotepadPanel,
    showUserSettings: controller.showUserSettings,
    setShowUserSettings: controller.setShowUserSettings,
    showApplicationsPanel: controller.showApplicationsPanel,
    setShowApplicationsPanel: controller.setShowApplicationsPanel,
    summaryPaneSessionId: controller.summaryPaneSessionId,
    runtimeContextOpen: controller.runtimeContextOpen,
    runtimeContextSessionId: controller.runtimeContextSessionId,
    handleOpenRuntimeContext: controller.handleOpenRuntimeContext,
    handleClearSummaryPaneSelection,
    handleToggleSessionSummary,
    sessionListProps,
    conversationPaneProps,
    uiPromptHistoryProps,
    runtimeContextProps,
  };
};
