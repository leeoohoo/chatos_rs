// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
    loadMessages: state.loadMessages,
    loadMoreMessages: state.loadMoreMessages,
    sendMessage: state.sendMessage,
    stopMessage: state.stopMessage,
    selectRemoteConnection: state.selectRemoteConnection,
    updateSession: state.updateSession,
    clearError: state.clearError,
    sidebarOpen: state.sidebarOpen,
    toggleSidebar: state.toggleSidebar,
    aiModelConfigs: state.aiModelConfigs,
    selectedModelId: state.selectedModelId,
    loadAiModelConfigs: state.loadAiModelConfigs,
    loadAgents: state.loadAgents,
    chatConfig: state.chatConfig,
    updateChatConfig: state.updateChatConfig,
    sessionChatState: state.sessionChatState,
    sessionMessagePaginationState: state.sessionMessagePaginationState,
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
