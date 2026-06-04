import React from 'react';

import ChatComposerPanel from '../../chatInterface/ChatComposerPanel';
import type { TeamMemberWorkspaceProps } from './TeamMemberWorkspaceTypes';

type TeamMemberWorkspaceComposerProps = Pick<
  TeamMemberWorkspaceProps,
  | 'project'
  | 'selectedContact'
  | 'currentAgent'
  | 'selectedProjectSession'
  | 'isSelectedSessionActive'
  | 'chatIsStreaming'
  | 'chatIsStopping'
  | 'selectedModelId'
  | 'selectedModelName'
  | 'selectedThinkingLevel'
  | 'aiModelConfigs'
  | 'supportsReasoning'
  | 'reasoningEnabled'
  | 'mcpEnabled'
  | 'enabledMcpIds'
  | 'autoCreateTask'
  | 'availableRemoteConnections'
  | 'currentRemoteConnectionId'
  | 'onRemoteConnectionChange'
  | 'onSend'
  | 'onGuide'
  | 'onStop'
  | 'onModelChange'
  | 'onModelNameChange'
  | 'onThinkingLevelChange'
  | 'onModelRuntimeChange'
  | 'onReasoningToggle'
  | 'onMcpEnabledChange'
  | 'onEnabledMcpIdsChange'
  | 'onAutoCreateTaskChange'
  | 'mergedCurrentTurnTasks'
  | 'workbarHistoryTasks'
  | 'taskHistoryOpen'
  | 'activeConversationTurnId'
  | 'workbarLoading'
  | 'workbarHistoryLoading'
  | 'workbarError'
  | 'workbarHistoryError'
  | 'workbarActionLoadingTaskId'
  | 'taskModalOpen'
  | 'taskModalMode'
  | 'taskModalTask'
  | 'taskModalError'
  | 'reviewRepairRunning'
  | 'reviewRepairPendingCount'
  | 'reviewRepairDisabled'
  | 'onRefreshWorkbarTasks'
  | 'onOpenWorkbarHistory'
  | 'onTaskHistoryOpenChange'
  | 'onRunReviewRepair'
  | 'onCompleteTask'
  | 'onDeleteTask'
  | 'onEditTask'
  | 'onCloseTaskModal'
  | 'onSubmitTaskModal'
  | 'activeUiPromptPanel'
  | 'onUiPromptSubmit'
  | 'onUiPromptCancel'
  | 'activeTaskReviewPanel'
  | 'onTaskReviewConfirm'
  | 'onTaskReviewCancel'
> & {
  runtimeGuidancePendingCount: number;
  runtimeGuidanceAppliedCount: number;
  runtimeGuidanceLastAppliedAt: string | null;
  runtimeGuidanceItems: NonNullable<TeamMemberWorkspaceProps['runtimeGuidanceItems']>;
  supportedFileTypes: React.ComponentProps<typeof ChatComposerPanel>['supportedFileTypes'];
};

export const TeamMemberWorkspaceComposer: React.FC<TeamMemberWorkspaceComposerProps> = ({
  project,
  selectedContact,
  currentAgent,
  selectedProjectSession,
  isSelectedSessionActive,
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
  runtimeGuidancePendingCount,
  runtimeGuidanceAppliedCount,
  runtimeGuidanceLastAppliedAt,
  runtimeGuidanceItems,
  supportedFileTypes,
}) => {
  if (!selectedContact || !selectedProjectSession) {
    return null;
  }

  return (
    <ChatComposerPanel
      sessionId={selectedProjectSession.id}
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
      reviewRepairAvailable={true}
      reviewRepairRunning={reviewRepairRunning}
      reviewRepairDisabled={reviewRepairDisabled}
      onReviewRepair={() => onRunReviewRepair(selectedProjectSession.id)}
      onRefreshWorkbarTasks={onRefreshWorkbarTasks}
      onOpenHistory={onOpenWorkbarHistory}
      onTaskHistoryOpenChange={onTaskHistoryOpenChange}
      uiPromptHistoryCount={0}
      uiPromptHistoryLoading={false}
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
      onSend={onSend}
      onGuide={onGuide}
      onStop={onStop}
      inputDisabled={!isSelectedSessionActive || chatIsStopping || reviewRepairRunning}
      isStreaming={chatIsStreaming}
      isStopping={chatIsStopping}
      supportedFileTypes={supportedFileTypes}
      reasoningSupported={supportsReasoning}
      reasoningEnabled={reasoningEnabled}
      onReasoningToggle={onReasoningToggle}
      selectedModelId={selectedModelId}
      selectedModelName={selectedModelName}
      selectedThinkingLevel={selectedThinkingLevel}
      availableModels={aiModelConfigs}
      onModelChange={onModelChange}
      onModelNameChange={onModelNameChange}
      onThinkingLevelChange={onThinkingLevelChange}
      onModelRuntimeChange={onModelRuntimeChange}
      availableProjects={[project]}
      currentProject={project}
      selectedProjectId={project.id}
      onProjectChange={() => {}}
      showProjectSelector={false}
      showProjectFileButton={false}
      showWorkspaceRootPicker={false}
      currentAgent={currentAgent}
      availableRemoteConnections={availableRemoteConnections}
      currentRemoteConnectionId={currentRemoteConnectionId}
      onRemoteConnectionChange={onRemoteConnectionChange}
      mcpEnabled={mcpEnabled}
      enabledMcpIds={enabledMcpIds}
      autoCreateTask={autoCreateTask}
      onMcpEnabledChange={onMcpEnabledChange}
      onEnabledMcpIdsChange={onEnabledMcpIdsChange}
      onAutoCreateTaskChange={onAutoCreateTaskChange}
      runtimeGuidancePendingCount={runtimeGuidancePendingCount}
      runtimeGuidanceAppliedCount={runtimeGuidanceAppliedCount}
      runtimeGuidanceLastAppliedAt={runtimeGuidanceLastAppliedAt}
      runtimeGuidanceItems={runtimeGuidanceItems}
    />
  );
};
