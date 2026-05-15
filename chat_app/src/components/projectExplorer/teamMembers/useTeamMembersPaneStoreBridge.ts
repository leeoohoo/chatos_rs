import { useMemo } from 'react';
import { shallow } from 'zustand/shallow';

import { apiClient as globalApiClient } from '../../../lib/api/client';
import {
  useChatApiClientFromContext,
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
  messages: state.messages,
  hasMoreMessages: state.hasMoreMessages,
  sessionMessagePaginationState: state.sessionMessagePaginationState,
  sessionChatState: state.sessionChatState,
  sendMessage: state.sendMessage,
  selectRemoteConnection: state.selectRemoteConnection,
  abortCurrentConversation: state.abortCurrentConversation,
  clearError: state.clearError,
  setError: state.setError,
  loadMessages: state.loadMessages,
  loadMoreMessages: state.loadMoreMessages,
  toggleTurnProcess: state.toggleTurnProcess,
  createSession: state.createSession,
  selectSession: state.selectSession,
  updateSession: state.updateSession,
  aiModelConfigs: state.aiModelConfigs,
  selectedModelId: state.selectedModelId,
  setSelectedModel: state.setSelectedModel,
  chatConfig: state.chatConfig,
  updateChatConfig: state.updateChatConfig,
  submitRuntimeGuidance: state.submitRuntimeGuidance,
  sessionRuntimeGuidanceState: state.sessionRuntimeGuidanceState,
  taskReviewPanelsBySession: state.taskReviewPanelsBySession,
  uiPromptPanelsBySession: state.uiPromptPanelsBySession,
  upsertTaskReviewPanel: state.upsertTaskReviewPanel,
  removeTaskReviewPanel: state.removeTaskReviewPanel,
  upsertUiPromptPanel: state.upsertUiPromptPanel,
  removeUiPromptPanel: state.removeUiPromptPanel,
});

export const useTeamMembersPaneStoreBridge = () => {
  const store = useChatStoreSelector(selectTeamMembersPaneStore, shallow);
  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(
    () => apiClientFromContext || globalApiClient,
    [apiClientFromContext],
  );

  return {
    ...store,
    apiClient,
  };
};
