// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { shallow } from 'zustand/shallow';

import { useApiClient } from '../../../lib/api/ApiClientContext';
import {
  useChatStoreSelector,
} from '../../../lib/store/ChatStoreContext';
import type { ChatActions, ChatState } from '../../../lib/store/createChatStoreWithBackend';

const selectTeamMembersPaneStore = (state: ChatState & ChatActions) => ({
  currentSession: state.currentSession,
  currentSessionId: state.currentSessionId,
  sessions: state.sessions,
  contacts: state.contacts,
  agents: state.agents,
  remoteConnections: state.remoteConnections,
  currentRemoteConnection: state.currentRemoteConnection,
  loadContacts: state.loadContacts,
  loadMessages: state.loadMessages,
  messages: state.messages,
  hasMoreMessages: state.hasMoreMessages,
  sessionMessagePaginationState: state.sessionMessagePaginationState,
  sessionChatState: state.sessionChatState,
  sendMessage: state.sendMessage,
  selectRemoteConnection: state.selectRemoteConnection,
  loadMoreMessages: state.loadMoreMessages,
  createSession: state.createSession,
  selectSession: state.selectSession,
  updateSession: state.updateSession,
  aiModelConfigs: state.aiModelConfigs,
  selectedModelId: state.selectedModelId,
  setSelectedModel: state.setSelectedModel,
  chatConfig: state.chatConfig,
  updateChatConfig: state.updateChatConfig,
});

export const useTeamMembersPaneStoreBridge = () => {
  const store = useChatStoreSelector(selectTeamMembersPaneStore, shallow);
  const apiClient = useApiClient();

  return {
    ...store,
    apiClient,
  };
};
