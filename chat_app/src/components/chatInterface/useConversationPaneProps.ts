import { useMemo, type ComponentProps } from 'react';

import ChatConversationPane from './ChatConversationPane';
import type {
  ChatInterfaceConversationActions,
  ChatInterfaceConversationState,
} from './viewPropsTypes';

interface UseConversationPanePropsParams {
  conversation: ChatInterfaceConversationState;
  actions: ChatInterfaceConversationActions;
}

export const useConversationPaneProps = ({
  conversation,
  actions,
}: UseConversationPanePropsParams): ComponentProps<typeof ChatConversationPane> => useMemo(() => ({
  currentSession: conversation.currentSession,
  sessionSummaryPaneVisible: conversation.sessionSummaryPaneVisible,
  currentContactName: conversation.currentContactName,
  currentProjectNameForMemory: conversation.currentProjectNameForMemory,
  currentProjectIdForMemory: conversation.currentProjectIdForMemory || null,
  messages: conversation.messages,
  hasMoreMessages: conversation.hasMoreMessages,
  onLoadMore: actions.handleLoadMore,
  customRenderer: conversation.customRenderer,
  sessionMemorySummaries: conversation.sessionMemorySummaries,
  agentRecalls: conversation.agentRecalls,
  memoryLoading: conversation.memoryLoading,
  memoryError: conversation.memoryError,
  onRefreshMemory: actions.handleRefreshMemory,
  onCloseSummary: actions.handleCloseSummary,
  toggleSidebar: actions.toggleSidebar,
  onSend: actions.handleMessageSend,
  inputDisabled: !conversation.currentSession,
  supportedFileTypes: conversation.supportedFileTypes,
  supportsReasoning: conversation.supportsReasoning,
  reasoningEnabled: conversation.reasoningEnabled,
  onReasoningToggle: actions.updateReasoningEnabled,
  selectedModelId: conversation.selectedModelId,
  selectedModelName: conversation.selectedModelName,
  selectedThinkingLevel: conversation.selectedThinkingLevel,
  availableModels: conversation.aiModelConfigs,
  onModelChange: actions.setSelectedModel,
  onModelNameChange: actions.setSelectedModelName,
  onThinkingLevelChange: actions.setSelectedThinkingLevel,
  onModelRuntimeChange: actions.setModelRuntimeSelection,
  availableProjects: conversation.composerAvailableProjects,
  currentProject: conversation.currentProject,
  onProjectChange: actions.handleComposerProjectChange,
  workspaceRoot: conversation.composerWorkspaceRoot,
  onWorkspaceRootChange: actions.handleComposerWorkspaceRootChange,
  currentRemoteConnectionId: conversation.currentRemoteConnectionId,
  availableRemoteConnections: conversation.remoteConnections || [],
  onRemoteConnectionChange: actions.handleComposerRemoteConnectionChange,
}), [actions, conversation]);
