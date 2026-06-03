import { shallow } from 'zustand/shallow';

import { useApiClient } from '../../lib/api/ApiClientContext';
import { useAuthStoreFromContext } from '../../lib/auth/authStore';
import {
  useChatStoreSelector,
} from '../../lib/store/ChatStoreContext';

export const useChatInterfaceStoreBridge = () => {
  const store = useChatStoreSelector((state) => ({
    currentSession: state.currentSession,
    currentSessionId: state.currentSessionId,
    sessions: state.sessions,
    contacts: state.contacts,
    currentProject: state.currentProject,
    currentTerminal: state.currentTerminal,
    currentRemoteConnection: state.currentRemoteConnection,
    remoteConnections: state.remoteConnections,
    projects: state.projects,
    activePanel: state.activePanel,
    messages: state.messages,
    hasMoreMessages: Boolean(
      state.currentSessionId
      && state.sessionMessagePaginationState?.[state.currentSessionId]?.nextBefore
    ),
    error: state.error,
    loadProjects: state.loadProjects,
    loadMoreMessages: state.loadMoreMessages,
    openTurnProcessViewer: state.openTurnProcessViewer,
    closeTurnProcessViewer: state.closeTurnProcessViewer,
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
    sessionMessagePaginationState: state.sessionMessagePaginationState,
    sessionTurnProcessCache: state.sessionTurnProcessCache,
    turnProcessViewer: state.turnProcessViewer,
    sessionRuntimeGuidanceState: state.sessionRuntimeGuidanceState,
    taskReviewPanelsBySession: state.taskReviewPanelsBySession,
    uiPromptPanelsBySession: state.uiPromptPanelsBySession,
    submitRuntimeGuidance: state.submitRuntimeGuidance,
    upsertTaskReviewPanel: state.upsertTaskReviewPanel,
    removeTaskReviewPanel: state.removeTaskReviewPanel,
    upsertUiPromptPanel: state.upsertUiPromptPanel,
    removeUiPromptPanel: state.removeUiPromptPanel,
  }), shallow);

  const apiClient = useApiClient();
  const auth = useAuthStoreFromContext();

  return {
    ...store,
    apiClient,
    user: auth.user,
    logout: auth.logout,
  };
};
