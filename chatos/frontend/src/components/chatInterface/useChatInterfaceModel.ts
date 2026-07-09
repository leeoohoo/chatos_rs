// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useState } from 'react';

import { useSessionRuntimeSettings } from '../../features/sessionRuntime/useSessionRuntimeSettings';
import type { ChatInterfaceProps } from '../../types';
import { useChatInterfaceController } from './useChatInterfaceController';
import { useChatInterfaceDerivedState } from './useChatInterfaceDerivedState';
import { useChatStreamRealtimeBridge } from './useChatStreamRealtimeBridge';
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
  const [summaryPaneSessionId, setSummaryPaneSessionId] = useState<string | null>(null);

  useChatStreamRealtimeBridge();

  const runtimeSettings = useSessionRuntimeSettings({
    session: store.currentSession,
  });
  const effectiveSelectedModelId = runtimeSettings.selectedModelId;

  const derived = useChatInterfaceDerivedState({
    currentSession: store.currentSession,
    contacts: store.contacts,
    selectedModelId: effectiveSelectedModelId,
    aiModelConfigs: store.aiModelConfigs,
    activePanel: store.activePanel,
    currentProject: store.currentProject,
    currentTerminal: store.currentTerminal,
    currentRemoteConnection: (store.remoteConnections || []).find(
      (connection) => connection.id === runtimeSettings.remoteConnectionId,
    ) || null,
    sessionChatState: store.sessionChatState || {},
  });

  const resources = useChatInterfaceSessionResources({
    apiClient: store.apiClient,
    currentSession: store.currentSession,
    currentContactId: derived.currentContactId,
    currentProject: store.currentProject,
    projects: store.projects,
  });

  const controller = useChatInterfaceController({
    apiClient: store.apiClient,
    activePanel: store.activePanel,
    currentSession: store.currentSession,
    messages: store.messages,
    runtimeContextRefreshNonce: derived.runtimeContextRefreshNonce,
    summaryPaneSessionId,
    setSummaryPaneSessionId,
    onMessageSend,
    sendMessage: store.sendMessage,
    flushRuntimeSettings: runtimeSettings.flushRuntimeSettings,
    selectRemoteConnection: store.selectRemoteConnection,
    loadMessages: store.loadMessages,
    loadMoreMessages: store.loadMoreMessages,
    loadProjects: store.loadProjects,
    loadAiModelConfigs: store.loadAiModelConfigs,
    loadAgents: store.loadAgents,
    loadContactMemoryContext: resources.loadContactMemoryContext,
    loadSessionMemorySummaries: resources.loadSessionMemorySummaries,
    applyRealtimeSessionMemorySummaries: resources.applyRealtimeSessionMemorySummaries,
    hydrateContactMemoryContextFromCache: resources.hydrateContactMemoryContextFromCache,
    markContactMemoryContextStale: resources.markContactMemoryContextStale,
    resetMemoryState: resources.resetMemoryState,
    cancelPendingMemoryLoad: resources.cancelPendingMemoryLoad,
  });

  const conversation: ChatInterfaceConversationState = {
    currentSession: store.currentSession,
    sessionSummaryPaneVisible: controller.sessionSummaryPaneVisible,
    currentContactName: derived.currentContactName,
    currentContactId: derived.currentContactId,
    currentProjectNameForMemory: resources.currentProjectNameForMemory,
    currentProjectIdForMemory: resources.currentProjectIdForMemory,
    messages: store.messages,
    hasMoreMessages: store.hasMoreMessages,
    customRenderer,
    sessionMemorySummaries: resources.sessionMemorySummaries,
    agentRecalls: resources.agentRecalls,
    memoryLoading: resources.memoryLoading,
    memoryError: resources.memoryError,
    supportedFileTypes: derived.supportedFileTypes,
    supportsReasoning: derived.supportsReasoning,
    reasoningEnabled: runtimeSettings.reasoningEnabled,
    planModeEnabled: runtimeSettings.planModeEnabled,
    selectedModelId: effectiveSelectedModelId,
    selectedModelName: runtimeSettings.selectedModelName,
    selectedThinkingLevel: runtimeSettings.selectedThinkingLevel,
    aiModelConfigs: store.aiModelConfigs,
    composerAvailableProjects: resources.composerAvailableProjects,
    currentProject: store.currentProject,
    composerWorkspaceRoot: runtimeSettings.workspaceRoot,
    currentRemoteConnectionId: runtimeSettings.remoteConnectionId,
    remoteConnections: store.remoteConnections || [],
    reviewRepairRunning: controller.reviewRepairRunning,
    reviewRepairPendingCount: controller.reviewRepairPendingCount,
    reviewRepairDisabled: controller.reviewRepairDisabled,
  };

  const conversationActions: ChatInterfaceConversationActions = {
    handleLoadMore: controller.handleLoadMore,
    handleRefreshMemory: controller.handleRefreshMemory,
    handleCloseSummary: controller.handleCloseSummary,
    toggleSidebar: store.toggleSidebar,
    handleMessageSend: controller.handleMessageSend,
    updateReasoningEnabled: runtimeSettings.setReasoningEnabled,
    updatePlanModeEnabled: runtimeSettings.setPlanModeEnabled,
    setSelectedModel: runtimeSettings.setSelectedModelId,
    setSelectedModelName: runtimeSettings.setSelectedModelName,
    setSelectedThinkingLevel: runtimeSettings.setSelectedThinkingLevel,
    setModelRuntimeSelection: runtimeSettings.setModelRuntimeSelection,
    handleComposerProjectChange: resources.handleComposerProjectChange,
    handleComposerWorkspaceRootChange: runtimeSettings.setWorkspaceRoot,
    handleComposerRemoteConnectionChange: runtimeSettings.setRemoteConnectionId,
    handleRunReviewRepair: controller.handleRunReviewRepair,
  };

  const overlay: ChatInterfaceOverlayState = {
    runtimeContextOpen: controller.runtimeContextOpen,
    runtimeContextSessionId: controller.runtimeContextSessionId,
    runtimeContextLoading: controller.runtimeContextLoading,
    runtimeContextError: controller.runtimeContextError,
    runtimeContextData: controller.runtimeContextData,
  };

  const overlayActions: ChatInterfaceOverlayActions = {
    handleRefreshRuntimeContext: controller.handleRefreshRuntimeContext,
    setRuntimeContextOpen: controller.setRuntimeContextOpen,
  };

  const {
    sessionListProps,
    conversationPaneProps,
    runtimeContextProps,
  } = useChatInterfaceViewProps({
    conversation,
    conversationActions,
    overlay,
    overlayActions,
  });

  const handleClearSummaryPaneSelection = useCallback(() => {
    setSummaryPaneSessionId(null);
  }, []);

  const handleToggleSessionSummary = useCallback((sessionId: string) => {
    setSummaryPaneSessionId((prev) => (prev === sessionId ? null : sessionId));
  }, []);

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
    showTaskRunnerExternalMcpManager: controller.showTaskRunnerExternalMcpManager,
    setShowTaskRunnerExternalMcpManager: controller.setShowTaskRunnerExternalMcpManager,
    showAgentManager: controller.showAgentManager,
    setShowAgentManager: controller.setShowAgentManager,
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
    runtimeContextProps,
  };
};
