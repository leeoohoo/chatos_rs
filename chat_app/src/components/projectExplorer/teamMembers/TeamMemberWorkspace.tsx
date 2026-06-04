import React, { useMemo } from 'react';

import { buildSupportedFileTypes, resolveModelSupportFlags } from '../../chatInterface/viewHelpers';
import TurnProcessModal from '../../TurnProcessModal';
import { TeamMemberWorkspaceComposer } from './TeamMemberWorkspaceComposer';
import { TeamMemberWorkspaceContent } from './TeamMemberWorkspaceContent';
import type { TeamMemberWorkspaceProps } from './TeamMemberWorkspaceTypes';

const TeamMemberWorkspace: React.FC<TeamMemberWorkspaceProps> = ({
  project,
  selectedContact,
  currentAgent,
  selectedProjectSession,
  isSelectedSessionActive,
  sessionSummaryPaneVisible,
  summaryItems,
  summaryLoading,
  summaryError,
  clearingSummaries,
  deletingSummaryId,
  messages,
  hasMoreMessages,
  chatIsLoading,
  chatIsStreaming,
  chatIsStopping,
  selectedModelId,
  selectedModelName,
  selectedThinkingLevel,
  aiModelConfigs,
  supportsReasoning,
  reasoningEnabled,
  mcpEnabled,
  enabledMcpIds,
  autoCreateTask,
  availableRemoteConnections,
  currentRemoteConnectionId,
  onRemoteConnectionChange,
  onLoadMore,
  onToggleTurnProcess,
  turnProcessViewerOpen,
  turnProcessViewerSessionId,
  turnProcessViewerUserMessageId,
  turnProcessViewerTurnId,
  turnProcessViewerCachedMessages,
  turnProcessApiClient,
  onCloseTurnProcessViewer,
  onClearSummaries,
  onRefreshSummaries,
  onCloseSummary,
  onDeleteSummary,
  onSend,
  onGuide,
  onStop,
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
  onReasoningToggle,
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
  onAutoCreateTaskChange,
  mergedCurrentTurnTasks,
  workbarHistoryTasks,
  taskHistoryOpen,
  activeConversationTurnId,
  workbarLoading,
  workbarHistoryLoading,
  workbarError,
  workbarHistoryError,
  workbarActionLoadingTaskId,
  taskModalOpen,
  taskModalMode,
  taskModalTask,
  taskModalError,
  reviewRepairRunning,
  reviewRepairPendingCount,
  reviewRepairDisabled,
  onRefreshWorkbarTasks,
  onOpenWorkbarHistory,
  onTaskHistoryOpenChange,
  onRunReviewRepair,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  onCloseTaskModal,
  onSubmitTaskModal,
  activeUiPromptPanel,
  onUiPromptSubmit,
  onUiPromptCancel,
  activeTaskReviewPanel,
  onTaskReviewConfirm,
  onTaskReviewCancel,
  runtimeGuidancePendingCount = 0,
  runtimeGuidanceAppliedCount = 0,
  runtimeGuidanceLastAppliedAt = null,
  runtimeGuidanceItems = [],
}) => {
  const { supportsImages } = useMemo(
    () => resolveModelSupportFlags(selectedModelId, aiModelConfigs),
    [aiModelConfigs, selectedModelId],
  );

  const supportedFileTypes = useMemo(
    () => buildSupportedFileTypes(supportsImages),
    [supportsImages],
  );

  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <div className="flex-1 overflow-hidden">
        <TeamMemberWorkspaceContent
          selectedContact={selectedContact}
          selectedProjectSession={selectedProjectSession}
          isSelectedSessionActive={isSelectedSessionActive}
          sessionSummaryPaneVisible={sessionSummaryPaneVisible}
          summaryItems={summaryItems}
          summaryLoading={summaryLoading}
          summaryError={summaryError}
          clearingSummaries={clearingSummaries}
          deletingSummaryId={deletingSummaryId}
          messages={messages}
          hasMoreMessages={hasMoreMessages}
          chatIsLoading={chatIsLoading}
          chatIsStreaming={chatIsStreaming}
          chatIsStopping={chatIsStopping}
          onLoadMore={onLoadMore}
          onToggleTurnProcess={onToggleTurnProcess}
          onClearSummaries={onClearSummaries}
          onRefreshSummaries={onRefreshSummaries}
          onCloseSummary={onCloseSummary}
          onDeleteSummary={onDeleteSummary}
        />
      </div>

      <TeamMemberWorkspaceComposer
        project={project}
        selectedContact={selectedContact}
        currentAgent={currentAgent}
        selectedProjectSession={selectedProjectSession}
        isSelectedSessionActive={isSelectedSessionActive}
        chatIsStreaming={chatIsStreaming}
        chatIsStopping={chatIsStopping}
        selectedModelId={selectedModelId}
        selectedModelName={selectedModelName}
        selectedThinkingLevel={selectedThinkingLevel}
        aiModelConfigs={aiModelConfigs}
        supportsReasoning={supportsReasoning}
        reasoningEnabled={reasoningEnabled}
        mcpEnabled={mcpEnabled}
        enabledMcpIds={enabledMcpIds}
        autoCreateTask={autoCreateTask}
        availableRemoteConnections={availableRemoteConnections}
        currentRemoteConnectionId={currentRemoteConnectionId}
        onRemoteConnectionChange={onRemoteConnectionChange}
        onSend={onSend}
        onGuide={onGuide}
        onStop={onStop}
        onModelChange={onModelChange}
        onModelNameChange={onModelNameChange}
        onThinkingLevelChange={onThinkingLevelChange}
        onModelRuntimeChange={onModelRuntimeChange}
        onReasoningToggle={onReasoningToggle}
        onMcpEnabledChange={onMcpEnabledChange}
        onEnabledMcpIdsChange={onEnabledMcpIdsChange}
        onAutoCreateTaskChange={onAutoCreateTaskChange}
        mergedCurrentTurnTasks={mergedCurrentTurnTasks}
        workbarHistoryTasks={workbarHistoryTasks}
        taskHistoryOpen={taskHistoryOpen}
        activeConversationTurnId={activeConversationTurnId}
        workbarLoading={workbarLoading}
        workbarHistoryLoading={workbarHistoryLoading}
        workbarError={workbarError}
        workbarHistoryError={workbarHistoryError}
        workbarActionLoadingTaskId={workbarActionLoadingTaskId}
        taskModalOpen={taskModalOpen}
        taskModalMode={taskModalMode}
        taskModalTask={taskModalTask}
        taskModalError={taskModalError}
        reviewRepairRunning={reviewRepairRunning}
        reviewRepairPendingCount={reviewRepairPendingCount}
        reviewRepairDisabled={reviewRepairDisabled}
        onRefreshWorkbarTasks={onRefreshWorkbarTasks}
        onOpenWorkbarHistory={onOpenWorkbarHistory}
        onTaskHistoryOpenChange={onTaskHistoryOpenChange}
        onRunReviewRepair={onRunReviewRepair}
        onCompleteTask={onCompleteTask}
        onDeleteTask={onDeleteTask}
        onEditTask={onEditTask}
        onCloseTaskModal={onCloseTaskModal}
        onSubmitTaskModal={onSubmitTaskModal}
        activeUiPromptPanel={activeUiPromptPanel}
        onUiPromptSubmit={onUiPromptSubmit}
        onUiPromptCancel={onUiPromptCancel}
        activeTaskReviewPanel={activeTaskReviewPanel}
        onTaskReviewConfirm={onTaskReviewConfirm}
        onTaskReviewCancel={onTaskReviewCancel}
        supportedFileTypes={supportedFileTypes}
        runtimeGuidancePendingCount={runtimeGuidancePendingCount}
        runtimeGuidanceAppliedCount={runtimeGuidanceAppliedCount}
        runtimeGuidanceLastAppliedAt={runtimeGuidanceLastAppliedAt}
        runtimeGuidanceItems={runtimeGuidanceItems}
      />

      <TurnProcessModal
        open={turnProcessViewerOpen}
        sessionId={turnProcessViewerSessionId}
        userMessageId={turnProcessViewerUserMessageId}
        turnId={turnProcessViewerTurnId}
        messages={messages}
        cachedProcessMessages={turnProcessViewerCachedMessages}
        apiClient={turnProcessApiClient}
        onClose={onCloseTurnProcessViewer}
      />
    </div>
  );
};

export default TeamMemberWorkspace;
