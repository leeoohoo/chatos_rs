import { useCallback, useEffect, useMemo, useState } from 'react';

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
  const [memoryModelConfigId, setMemoryModelConfigId] = useState<string | null>(null);
  const [memoryModelSettingsLoaded, setMemoryModelSettingsLoaded] = useState(false);

  useChatStreamRealtimeBridge();

  const runtimeSettings = useSessionRuntimeSettings({
    session: store.currentSession,
    updateSession: store.updateSession,
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
    currentRemoteConnection: store.currentRemoteConnection,
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
    runtimeContextRefreshNonce: derived.runtimeContextRefreshNonce,
    currentRemoteConnectionId: store.currentRemoteConnection?.id || null,
    summaryPaneSessionId,
    setSummaryPaneSessionId,
    onMessageSend,
    sendMessage: store.sendMessage,
    selectRemoteConnection: store.selectRemoteConnection,
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

  useEffect(() => {
    let cancelled = false;
    setMemoryModelSettingsLoaded(false);
    store.apiClient
      .getAiModelSettings()
      .then((settings) => {
        if (cancelled) {
          return;
        }
        setMemoryModelConfigId(settings.memory_summary_model_config_id?.trim() || null);
      })
      .catch((error) => {
        if (!cancelled) {
          console.error('Failed to load memory model settings:', error);
          setMemoryModelConfigId(null);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setMemoryModelSettingsLoaded(true);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    store.apiClient,
    store.aiModelConfigs,
    controller.showMemoryModelSettings,
  ]);

  const enabledConcreteModelIds = useMemo(() => new Set(
    store.aiModelConfigs
      .filter((item) => item.enabled && item.model_name.trim())
      .map((item) => item.id),
  ), [store.aiModelConfigs]);

  const memoryModelAttention = useMemo(() => {
    if (!memoryModelSettingsLoaded || enabledConcreteModelIds.size === 0) {
      return false;
    }
    return !memoryModelConfigId || !enabledConcreteModelIds.has(memoryModelConfigId);
  }, [enabledConcreteModelIds, memoryModelConfigId, memoryModelSettingsLoaded]);

  const taskModelAttention = useMemo(() => store.aiModelConfigs.some((item) => (
    item.enabled
    && Boolean(item.model_name.trim())
    && (!item.task_usage_scenario?.trim() || !item.task_thinking_level?.trim())
  )), [store.aiModelConfigs]);

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
    reasoningEnabled: store.chatConfig?.reasoningEnabled === true,
    planModeEnabled: store.chatConfig?.planModeEnabled === true,
    selectedModelId: effectiveSelectedModelId,
    selectedModelName: runtimeSettings.selectedModelName,
    selectedThinkingLevel: runtimeSettings.selectedThinkingLevel,
    aiModelConfigs: store.aiModelConfigs,
    composerAvailableProjects: resources.composerAvailableProjects,
    currentProject: store.currentProject,
    composerWorkspaceRoot: runtimeSettings.workspaceRoot,
    currentRemoteConnectionId: store.currentRemoteConnection?.id || null,
    remoteConnections: store.remoteConnections || [],
  };

  const conversationActions: ChatInterfaceConversationActions = {
    handleLoadMore: controller.handleLoadMore,
    handleRefreshMemory: controller.handleRefreshMemory,
    handleCloseSummary: controller.handleCloseSummary,
    toggleSidebar: store.toggleSidebar,
    handleMessageSend: controller.handleMessageSend,
    updateReasoningEnabled: (enabled: boolean) => store.updateChatConfig({ reasoningEnabled: enabled }),
    updatePlanModeEnabled: (enabled: boolean) => store.updateChatConfig({ planModeEnabled: enabled }),
    setSelectedModel: runtimeSettings.setSelectedModelId,
    setSelectedModelName: runtimeSettings.setSelectedModelName,
    setSelectedThinkingLevel: runtimeSettings.setSelectedThinkingLevel,
    setModelRuntimeSelection: runtimeSettings.setModelRuntimeSelection,
    handleComposerProjectChange: resources.handleComposerProjectChange,
    handleComposerWorkspaceRootChange: runtimeSettings.setWorkspaceRoot,
    handleComposerRemoteConnectionChange: controller.handleComposerRemoteConnectionChange,
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
    showAiModelManager: controller.showAiModelManager,
    setShowAiModelManager: controller.setShowAiModelManager,
    showMemoryModelSettings: controller.showMemoryModelSettings,
    setShowMemoryModelSettings: controller.setShowMemoryModelSettings,
    showTaskModelSettings: controller.showTaskModelSettings,
    setShowTaskModelSettings: controller.setShowTaskModelSettings,
    showTaskRunnerExternalMcpManager: controller.showTaskRunnerExternalMcpManager,
    setShowTaskRunnerExternalMcpManager: controller.setShowTaskRunnerExternalMcpManager,
    memoryModelAttention,
    taskModelAttention,
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
