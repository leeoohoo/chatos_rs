import React from 'react';
import { InputArea } from '../InputArea';
import TaskDraftPanel from '../TaskDraftPanel';
import TaskWorkbar, { type TaskWorkbarItem } from '../TaskWorkbar';
import UiPromptPanel from '../UiPromptPanel';
import type { AiModelConfig, Project } from '../../types';
import type {
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptPanelState,
  UiPromptResponsePayload,
} from '../../lib/store/types';

interface ChatComposerPanelProps {
  sessionId: string;
  mergedCurrentTurnTasks: TaskWorkbarItem[];
  workbarHistoryTasks: TaskWorkbarItem[];
  activeConversationTurnId: string | null;
  workbarLoading: boolean;
  workbarHistoryLoading: boolean;
  workbarError: string | null;
  workbarHistoryError: string | null;
  workbarActionLoadingTaskId: string | null;
  onRefreshWorkbarTasks: () => void;
  onOpenHistory: (sessionId: string) => void;
  onOpenUiPromptHistory: (sessionId: string) => void;
  uiPromptHistoryCount: number;
  uiPromptHistoryLoading: boolean;
  onCompleteTask: (task: TaskWorkbarItem) => void;
  onDeleteTask: (task: TaskWorkbarItem) => void;
  onEditTask: (task: TaskWorkbarItem) => void;
  activeUiPromptPanel: UiPromptPanelState | null;
  onUiPromptSubmit: (payload: UiPromptResponsePayload) => void;
  onUiPromptCancel: () => void;
  activeTaskReviewPanel: TaskReviewPanelState | null;
  onTaskReviewConfirm: (drafts: TaskReviewDraft[]) => void;
  onTaskReviewCancel: () => void;
  onSend: (
    content: string,
    attachments?: File[],
    runtimeOptions?: {
      mcpEnabled?: boolean;
      projectId?: string | null;
      projectRoot?: string | null;
      enabledMcpIds?: string[];
    },
  ) => void;
  onStop: () => void;
  inputDisabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  supportedFileTypes: string[];
  reasoningSupported: boolean;
  reasoningEnabled: boolean;
  onReasoningToggle: (enabled: boolean) => void;
  selectedModelId: string | null;
  availableModels: AiModelConfig[];
  onModelChange: (modelId: string | null) => void;
  availableProjects: Project[];
  currentProject: Project | null;
  selectedProjectId: string | null;
  onProjectChange: (projectId: string | null) => void;
  mcpEnabled: boolean;
  enabledMcpIds?: string[];
  onMcpEnabledChange: (enabled: boolean) => void;
  onEnabledMcpIdsChange: (ids: string[]) => void;
}

const ChatComposerPanel: React.FC<ChatComposerPanelProps> = ({
  sessionId,
  mergedCurrentTurnTasks,
  workbarHistoryTasks,
  activeConversationTurnId,
  workbarLoading,
  workbarHistoryLoading,
  workbarError,
  workbarHistoryError,
  workbarActionLoadingTaskId,
  onRefreshWorkbarTasks,
  onOpenHistory,
  onOpenUiPromptHistory,
  uiPromptHistoryCount,
  uiPromptHistoryLoading,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  activeUiPromptPanel,
  onUiPromptSubmit,
  onUiPromptCancel,
  activeTaskReviewPanel,
  onTaskReviewConfirm,
  onTaskReviewCancel,
  onSend,
  onStop,
  inputDisabled,
  isStreaming,
  isStopping,
  supportedFileTypes,
  reasoningSupported,
  reasoningEnabled,
  onReasoningToggle,
  selectedModelId,
  availableModels,
  onModelChange,
  availableProjects,
  currentProject,
  selectedProjectId,
  onProjectChange,
  mcpEnabled,
  enabledMcpIds,
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
}) => (
  <div className="border-t border-border">
    <TaskWorkbar
      tasks={mergedCurrentTurnTasks}
      historyTasks={workbarHistoryTasks}
      currentTurnId={activeConversationTurnId}
      isLoading={workbarLoading}
      historyLoading={workbarHistoryLoading}
      error={workbarError}
      historyError={workbarHistoryError}
      actionLoadingTaskId={workbarActionLoadingTaskId}
      onRefresh={onRefreshWorkbarTasks}
      onOpenHistory={() => onOpenHistory(sessionId)}
      onOpenUiPromptHistory={() => onOpenUiPromptHistory(sessionId)}
      uiPromptHistoryCount={uiPromptHistoryCount}
      uiPromptHistoryLoading={uiPromptHistoryLoading}
      onCompleteTask={onCompleteTask}
      onDeleteTask={onDeleteTask}
      onEditTask={onEditTask}
    />
    {activeUiPromptPanel ? (
      <UiPromptPanel
        panel={activeUiPromptPanel}
        onSubmit={onUiPromptSubmit}
        onCancel={onUiPromptCancel}
      />
    ) : null}
    {activeTaskReviewPanel ? (
      <TaskDraftPanel
        panel={activeTaskReviewPanel}
        onConfirm={onTaskReviewConfirm}
        onCancel={onTaskReviewCancel}
      />
    ) : null}
    <InputArea
      onSend={onSend}
      onStop={onStop}
      disabled={inputDisabled}
      isStreaming={isStreaming}
      isStopping={isStopping}
      placeholder="输入消息..."
      allowAttachments={true}
      supportedFileTypes={supportedFileTypes}
      reasoningSupported={reasoningSupported}
      reasoningEnabled={reasoningEnabled}
      onReasoningToggle={onReasoningToggle}
      showModelSelector={true}
      selectedModelId={selectedModelId}
      availableModels={availableModels}
      onModelChange={onModelChange}
      availableProjects={availableProjects}
      currentProject={currentProject}
      selectedProjectId={selectedProjectId}
      onProjectChange={onProjectChange}
      mcpEnabled={mcpEnabled}
      enabledMcpIds={enabledMcpIds}
      onMcpEnabledChange={onMcpEnabledChange}
      onEnabledMcpIdsChange={onEnabledMcpIdsChange}
    />
  </div>
);

export default ChatComposerPanel;
