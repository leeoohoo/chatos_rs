import { useMemo } from 'react';
import { shallow } from 'zustand/shallow';

import { apiClient as globalApiClient } from '../../lib/api/client';
import { useAuthStore } from '../../lib/auth/authStore';
import {
  useChatApiClientFromContext,
  useChatStoreSelector,
} from '../../lib/store/ChatStoreContext';

export const useChatInterfaceStoreBridge = () => {
  const store = useChatStoreSelector((state) => ({
    currentSession: state.currentSession,
    sessions: state.sessions,
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
    setError: state.setError,
    sidebarOpen: state.sidebarOpen,
    toggleSidebar: state.toggleSidebar,
    aiModelConfigs: state.aiModelConfigs,
    agents: state.agents,
    selectedModelId: state.selectedModelId,
    selectedAgentId: state.selectedAgentId,
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
  const auth = useAuthStore();

  return {
    ...store,
    apiClient,
    user: auth.user,
    logout: auth.logout,
  };
};
