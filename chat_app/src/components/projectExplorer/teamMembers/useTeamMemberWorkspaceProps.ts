import { useMemo } from 'react';
import type { ComponentProps } from 'react';

import TeamMemberWorkspace from './TeamMemberWorkspace';
import type { UseTeamMembersPaneViewPropsOptions } from './teamMembersPaneViewPropTypes';

export const useTeamMemberWorkspaceProps = ({
  project,
  store,
  resources,
}: UseTeamMembersPaneViewPropsOptions): ComponentProps<typeof TeamMemberWorkspace> => useMemo(() => {
  const selectedSessionId = resources.conversation.selectedProjectSession?.id || null;
  const hasMoreMessages = Boolean(
    selectedSessionId
    && store.sessionMessagePaginationState?.[selectedSessionId]?.nextBefore
  );

  return ({
  project,
  selectedContact: resources.conversation.selectedContact,
  currentAgent: resources.conversation.selectedContactAgent,
  selectedProjectSession: resources.conversation.selectedProjectSession,
  isSelectedSessionActive: resources.conversation.isSelectedSessionActive,
  sessionSummaryPaneVisible: resources.conversation.sessionSummaryPaneVisible,
  summaryItems: resources.summary.summaryItems,
  summaryLoading: resources.summary.summaryLoading,
  summaryError: resources.summary.summaryError,
  clearingSummaries: resources.summary.clearingSummaries,
  deletingSummaryId: resources.summary.deletingSummaryId,
  messages: store.messages,
  hasMoreMessages,
  chatIsLoading: resources.conversation.chatIsLoading,
  chatIsStreaming: resources.conversation.chatIsStreaming,
  chatIsStopping: resources.conversation.chatIsStopping,
  selectedModelId: store.selectedModelId,
  aiModelConfigs: store.aiModelConfigs,
  supportsReasoning: resources.conversation.supportsReasoning,
  reasoningEnabled: store.chatConfig?.reasoningEnabled === true,
  mcpEnabled: resources.composer.composerMcpEnabled,
  enabledMcpIds: resources.composer.composerEnabledMcpIds,
  autoCreateTask: resources.composer.composerAutoCreateTask,
  availableRemoteConnections: resources.composer.remoteConnections || [],
  currentRemoteConnectionId: resources.composer.currentRemoteConnection?.id || null,
  onRemoteConnectionChange: resources.composer.handleComposerRemoteConnectionChange,
  onLoadMore: resources.conversation.handleLoadMore,
  onToggleTurnProcess: resources.conversation.handleToggleTurnProcess,
  turnProcessViewerOpen: store.turnProcessViewer.open,
  turnProcessViewerSessionId: store.turnProcessViewer.sessionId,
  turnProcessViewerUserMessageId: store.turnProcessViewer.userMessageId,
  turnProcessViewerTurnId: store.turnProcessViewer.turnId,
  turnProcessViewerCachedMessages: store.turnProcessViewer.sessionId
    ? (store.sessionTurnProcessCache?.[store.turnProcessViewer.sessionId] || null)
    : null,
  turnProcessApiClient: store.apiClient,
  onCloseTurnProcessViewer: store.closeTurnProcessViewer,
  onClearSummaries: () => {
    void resources.conversation.handleClearSummaries();
  },
  onRefreshSummaries: () => {
    const sessionId = resources.conversation.selectedProjectSession?.id;
    if (!sessionId) {
      return;
    }
    void resources.summary.loadSessionSummaries(sessionId, { force: true });
  },
  onCloseSummary: () => {
    resources.summary.setSummaryPaneSessionId(null);
  },
  onDeleteSummary: (summaryId) => {
    void resources.conversation.handleDeleteSummary(summaryId);
  },
  onSend: resources.conversation.handleSendMessage,
  onGuide: resources.composer.handleRuntimeGuidanceSend,
  onStop: store.abortCurrentConversation,
  onModelChange: store.setSelectedModel,
  onReasoningToggle: (enabled) => store.updateChatConfig({ reasoningEnabled: enabled }),
  onMcpEnabledChange: resources.composer.handleComposerMcpEnabledChange,
  onEnabledMcpIdsChange: resources.composer.handleComposerEnabledMcpIdsChange,
  onAutoCreateTaskChange: resources.composer.handleComposerAutoCreateTaskChange,
  mergedCurrentTurnTasks: resources.workbar.mergedCurrentTurnTasks,
  workbarHistoryTasks: resources.workbar.workbarHistoryTasks,
  taskHistoryOpen: resources.workbar.taskHistoryOpen,
  activeConversationTurnId: resources.workbar.activeConversationTurnId,
  workbarLoading: resources.workbar.workbarLoading,
  workbarHistoryLoading: resources.workbar.workbarHistoryLoading,
  workbarError: resources.workbar.workbarError,
  workbarHistoryError: resources.workbar.workbarHistoryError,
  workbarActionLoadingTaskId: resources.workbar.workbarActionLoadingTaskId,
  taskModalOpen: resources.workbar.taskModalOpen,
  taskModalMode: resources.workbar.taskModalMode,
  taskModalTask: resources.workbar.taskModalTask,
  taskModalError: resources.workbar.taskModalError,
  reviewRepairRunning: resources.workbar.reviewRepairRunning,
  reviewRepairPendingCount: resources.workbar.reviewRepairPendingCount,
  reviewRepairDisabled: resources.workbar.reviewRepairDisabled,
  onRefreshWorkbarTasks: resources.workbar.handleRefreshWorkbar,
  onOpenWorkbarHistory: resources.workbar.handleOpenTeamWorkbarHistory,
  onTaskHistoryOpenChange: resources.workbar.setTaskHistoryOpen,
  onRunReviewRepair: resources.workbar.handleRunReviewRepair,
  onCompleteTask: (task) => {
    void resources.workbar.handleWorkbarCompleteTask(task);
  },
  onDeleteTask: (task) => {
    void resources.workbar.handleWorkbarDeleteTask(task);
  },
  onEditTask: (task) => {
    void resources.workbar.handleWorkbarEditTask(task);
  },
  onCloseTaskModal: resources.workbar.closeTaskModal,
  onSubmitTaskModal: (draft) => {
    void resources.workbar.submitTaskModal(draft);
  },
  activeUiPromptPanel: resources.workbar.activeUiPromptPanel,
  onUiPromptSubmit: resources.workbar.handleUiPromptSubmit,
  onUiPromptCancel: resources.workbar.handleUiPromptCancel,
  activeTaskReviewPanel: resources.workbar.activeTaskReviewPanel,
  onTaskReviewConfirm: resources.workbar.handleTaskReviewConfirm,
  onTaskReviewCancel: resources.workbar.handleTaskReviewCancel,
  runtimeGuidancePendingCount: resources.workbar.runtimeGuidancePendingCount,
  runtimeGuidanceAppliedCount: resources.workbar.runtimeGuidanceAppliedCount,
  runtimeGuidanceLastAppliedAt: resources.workbar.runtimeGuidanceLastAppliedAt,
  runtimeGuidanceItems: resources.workbar.runtimeGuidanceItems,
  });
}, [
  project,
  resources.composer.composerEnabledMcpIds,
  resources.composer.composerAutoCreateTask,
  resources.composer.composerMcpEnabled,
  resources.composer.handleComposerAutoCreateTaskChange,
  resources.composer.currentRemoteConnection?.id,
  resources.composer.handleComposerEnabledMcpIdsChange,
  resources.composer.handleComposerMcpEnabledChange,
  resources.composer.handleComposerRemoteConnectionChange,
  resources.composer.handleRuntimeGuidanceSend,
  resources.composer.remoteConnections,
  resources.conversation.chatIsLoading,
  resources.conversation.chatIsStopping,
  resources.conversation.chatIsStreaming,
  resources.conversation.handleClearSummaries,
  resources.conversation.handleDeleteSummary,
  resources.conversation.handleLoadMore,
  resources.conversation.handleSendMessage,
  resources.conversation.handleToggleTurnProcess,
  resources.conversation.isSelectedSessionActive,
  resources.conversation.selectedContact,
  resources.conversation.selectedContactAgent,
  resources.conversation.selectedProjectSession,
  resources.conversation.sessionSummaryPaneVisible,
  resources.conversation.supportsReasoning,
  resources.summary.clearingSummaries,
  resources.summary.deletingSummaryId,
  resources.summary.loadSessionSummaries,
  resources.summary.setSummaryPaneSessionId,
  resources.summary.summaryError,
  resources.summary.summaryItems,
  resources.summary.summaryLoading,
  resources.workbar.activeConversationTurnId,
  resources.workbar.activeTaskReviewPanel,
  resources.workbar.activeUiPromptPanel,
  resources.workbar.closeTaskModal,
  resources.workbar.handleOpenTeamWorkbarHistory,
  resources.workbar.handleRunReviewRepair,
  resources.workbar.handleRefreshWorkbar,
  resources.workbar.handleTaskReviewCancel,
  resources.workbar.handleTaskReviewConfirm,
  resources.workbar.handleUiPromptCancel,
  resources.workbar.handleUiPromptSubmit,
  resources.workbar.handleWorkbarCompleteTask,
  resources.workbar.handleWorkbarDeleteTask,
  resources.workbar.handleWorkbarEditTask,
  resources.workbar.mergedCurrentTurnTasks,
  resources.workbar.reviewRepairRunning,
  resources.workbar.reviewRepairPendingCount,
  resources.workbar.reviewRepairDisabled,
  resources.workbar.setTaskHistoryOpen,
  resources.workbar.runtimeGuidanceAppliedCount,
  resources.workbar.runtimeGuidanceItems,
  resources.workbar.runtimeGuidanceLastAppliedAt,
  resources.workbar.runtimeGuidancePendingCount,
  resources.workbar.submitTaskModal,
  resources.workbar.taskModalError,
  resources.workbar.taskModalMode,
  resources.workbar.taskModalOpen,
  resources.workbar.taskModalTask,
  resources.workbar.taskHistoryOpen,
  resources.workbar.workbarActionLoadingTaskId,
  resources.workbar.workbarError,
  resources.workbar.workbarHistoryError,
  resources.workbar.workbarHistoryLoading,
  resources.workbar.workbarHistoryTasks,
  resources.workbar.workbarLoading,
  store.abortCurrentConversation,
  store.apiClient,
  store.aiModelConfigs,
  store.chatConfig?.reasoningEnabled,
  store.closeTurnProcessViewer,
  store.messages,
  store.sessionMessagePaginationState,
  store.selectedModelId,
  store.turnProcessViewer,
  store.setSelectedModel,
  store.updateChatConfig,
]);
