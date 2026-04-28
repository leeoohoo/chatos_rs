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
  chatIsLoading: conversation.chatIsLoading,
  chatIsStreaming: conversation.chatIsStreaming,
  chatIsStopping: conversation.chatIsStopping,
  chatStreamingPreviewText: conversation.chatStreamingPreviewText,
  hasMoreMessages: conversation.hasMoreMessages,
  onLoadMore: actions.handleLoadMore,
  onToggleTurnProcess: actions.handleToggleTurnProcess,
  customRenderer: conversation.customRenderer,
  sessionMemorySummaries: conversation.sessionMemorySummaries,
  agentRecalls: conversation.agentRecalls,
  memoryLoading: conversation.memoryLoading,
  memoryError: conversation.memoryError,
  onRefreshMemory: actions.handleRefreshMemory,
  onCloseSummary: actions.handleCloseSummary,
  toggleSidebar: actions.toggleSidebar,
  mergedCurrentTurnTasks: conversation.mergedCurrentTurnTasks,
  workbarHistoryTasks: conversation.workbarHistoryTasks,
  activeConversationTurnId: conversation.activeConversationTurnId,
  workbarLoading: conversation.workbarLoading,
  workbarHistoryLoading: conversation.workbarHistoryLoading,
  workbarError: conversation.workbarError,
  workbarHistoryError: conversation.workbarHistoryError,
  workbarActionLoadingTaskId: conversation.workbarActionLoadingTaskId,
  taskModalOpen: conversation.taskModalOpen,
  taskModalMode: conversation.taskModalMode,
  taskModalTask: conversation.taskModalTask,
  taskModalError: conversation.taskModalError,
  onRefreshWorkbarTasks: actions.handleRefreshWorkbar,
  onOpenHistory: actions.handleOpenHistory,
  onOpenUiPromptHistory: actions.handleOpenUiPromptHistory,
  uiPromptHistoryCount: conversation.uiPromptHistoryItems.length,
  uiPromptHistoryLoading: conversation.uiPromptHistoryLoading,
  onCompleteTask: (task) => {
    void actions.handleWorkbarCompleteTask(task);
  },
  onDeleteTask: (task) => {
    void actions.handleWorkbarDeleteTask(task);
  },
  onEditTask: (task) => {
    void actions.handleWorkbarEditTask(task);
  },
  onCloseTaskModal: actions.closeTaskModal,
  onSubmitTaskModal: (draft) => {
    void actions.submitTaskModal(draft);
  },
  activeUiPromptPanel: conversation.activeUiPromptPanel,
  onUiPromptSubmit: actions.handleUiPromptSubmit,
  onUiPromptCancel: actions.handleUiPromptCancel,
  activeTaskReviewPanel: conversation.activeTaskReviewPanel,
  onTaskReviewConfirm: actions.handleTaskReviewConfirm,
  onTaskReviewCancel: actions.handleTaskReviewCancel,
  onSend: actions.handleMessageSend,
  onGuide: actions.handleRuntimeGuidanceSend,
  onStop: actions.abortCurrentConversation,
  inputDisabled: conversation.chatIsStopping || !conversation.currentSession,
  isStreaming: conversation.chatIsStreaming,
  isStopping: conversation.chatIsStopping,
  supportedFileTypes: conversation.supportedFileTypes,
  supportsReasoning: conversation.supportsReasoning,
  reasoningEnabled: conversation.reasoningEnabled,
  onReasoningToggle: actions.updateReasoningEnabled,
  selectedModelId: conversation.selectedModelId,
  currentAgent: conversation.currentAgent,
  availableModels: conversation.aiModelConfigs,
  onModelChange: actions.setSelectedModel,
  availableProjects: conversation.composerAvailableProjects,
  currentProject: conversation.currentProject,
  onProjectChange: actions.handleComposerProjectChange,
  workspaceRoot: conversation.composerWorkspaceRoot,
  onWorkspaceRootChange: actions.handleComposerWorkspaceRootChange,
  currentRemoteConnectionId: conversation.currentRemoteConnectionId,
  availableRemoteConnections: conversation.remoteConnections || [],
  onRemoteConnectionChange: actions.handleComposerRemoteConnectionChange,
  mcpEnabled: conversation.composerMcpEnabled,
  enabledMcpIds: conversation.composerEnabledMcpIds,
  onMcpEnabledChange: actions.handleComposerMcpEnabledChange,
  onEnabledMcpIdsChange: actions.handleComposerEnabledMcpIdsChange,
  runtimeGuidancePendingCount: conversation.runtimeGuidancePendingCount,
  runtimeGuidanceAppliedCount: conversation.runtimeGuidanceAppliedCount,
  runtimeGuidanceLastAppliedAt: conversation.runtimeGuidanceLastAppliedAt,
  runtimeGuidanceItems: conversation.runtimeGuidanceItems,
}), [actions, conversation]);
