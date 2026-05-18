import React from 'react';
import { InputArea } from '../InputArea';
import TaskDraftPanel from '../TaskDraftPanel';
import TaskWorkbar, { type RuntimeGuidanceWorkbarItem, type TaskWorkbarItem } from '../TaskWorkbar';
import UiPromptPanel from '../UiPromptPanel';
import type { TaskOutcomeDraft } from '../taskWorkbar/TaskOutcomeModal';
import type {
  AgentConfig,
  AiModelConfig,
  GuideMessageHandler,
  Project,
  RemoteConnection,
  SendMessageHandler,
} from '../../types';
import type {
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptPanelState,
  UiPromptResponsePayload,
} from '../../lib/store/types';
import { useI18n } from '../../i18n/I18nProvider';

interface ChatComposerPanelProps {
  sessionId: string;
  mergedCurrentTurnTasks: TaskWorkbarItem[];
  workbarHistoryTasks: TaskWorkbarItem[];
  taskHistoryOpen?: boolean;
  activeConversationTurnId: string | null;
  workbarLoading: boolean;
  workbarHistoryLoading: boolean;
  workbarError: string | null;
  workbarHistoryError: string | null;
  workbarActionLoadingTaskId: string | null;
  taskModalOpen: boolean;
  taskModalMode: 'complete' | 'edit';
  taskModalTask: TaskWorkbarItem | null;
  taskModalError: string | null;
  onRefreshWorkbarTasks: () => void;
  onOpenHistory: (sessionId: string) => void;
  onTaskHistoryOpenChange?: (value: boolean) => void;
  onOpenUiPromptHistory?: (sessionId: string) => void;
  uiPromptHistoryCount?: number;
  uiPromptHistoryLoading?: boolean;
  onCompleteTask: (task: TaskWorkbarItem) => void;
  onDeleteTask: (task: TaskWorkbarItem) => void;
  onEditTask: (task: TaskWorkbarItem) => void;
  onCloseTaskModal: () => void;
  onSubmitTaskModal: (draft: TaskOutcomeDraft) => void;
  activeUiPromptPanel: UiPromptPanelState | null;
  onUiPromptSubmit: (payload: UiPromptResponsePayload) => void;
  onUiPromptCancel: () => void;
  activeTaskReviewPanel: TaskReviewPanelState | null;
  onTaskReviewConfirm: (drafts: TaskReviewDraft[]) => void;
  onTaskReviewCancel: () => void;
  onSend: SendMessageHandler;
  onGuide: GuideMessageHandler;
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
  showProjectSelector?: boolean;
  showProjectFileButton?: boolean;
  reviewRepairAvailable?: boolean;
  reviewRepairRunning?: boolean;
  reviewRepairDisabled?: boolean;
  onReviewRepair?: () => void | Promise<void>;
  workspaceRoot?: string | null;
  onWorkspaceRootChange?: (path: string | null) => void;
  currentRemoteConnectionId?: string | null;
  currentAgent?: AgentConfig | null;
  availableRemoteConnections?: RemoteConnection[];
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  showWorkspaceRootPicker?: boolean;
  mcpEnabled: boolean;
  enabledMcpIds?: string[];
  onMcpEnabledChange: (enabled: boolean) => void;
  onEnabledMcpIdsChange: (ids: string[]) => void;
  runtimeGuidancePendingCount?: number;
  runtimeGuidanceAppliedCount?: number;
  runtimeGuidanceLastAppliedAt?: string | null;
  runtimeGuidanceItems?: RuntimeGuidanceWorkbarItem[];
}

const ChatComposerPanel: React.FC<ChatComposerPanelProps> = ({
  sessionId,
  mergedCurrentTurnTasks,
  workbarHistoryTasks,
  taskHistoryOpen = false,
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
  onRefreshWorkbarTasks,
  onOpenHistory,
  onTaskHistoryOpenChange,
  onOpenUiPromptHistory,
  uiPromptHistoryCount,
  uiPromptHistoryLoading,
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
  onSend,
  onGuide,
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
  showProjectSelector = true,
  showProjectFileButton = true,
  reviewRepairAvailable = false,
  reviewRepairRunning = false,
  reviewRepairDisabled = false,
  onReviewRepair,
  workspaceRoot = null,
  onWorkspaceRootChange,
  currentRemoteConnectionId = null,
  currentAgent = null,
  availableRemoteConnections = [],
  onRemoteConnectionChange,
  showWorkspaceRootPicker = false,
  mcpEnabled,
  enabledMcpIds,
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
  runtimeGuidancePendingCount = 0,
  runtimeGuidanceAppliedCount = 0,
  runtimeGuidanceLastAppliedAt = null,
  runtimeGuidanceItems = [],
}) => {
  const { t } = useI18n();

  return (
  <div className="border-t border-border">
    <TaskWorkbar
      tasks={mergedCurrentTurnTasks}
      historyTasks={workbarHistoryTasks}
      historyOpen={taskHistoryOpen}
      currentTurnId={activeConversationTurnId}
      isLoading={workbarLoading}
      historyLoading={workbarHistoryLoading}
      error={workbarError}
      historyError={workbarHistoryError}
      actionLoadingTaskId={workbarActionLoadingTaskId}
      onRefresh={onRefreshWorkbarTasks}
      onOpenHistory={() => onOpenHistory(sessionId)}
      onHistoryOpenChange={onTaskHistoryOpenChange}
      onOpenUiPromptHistory={onOpenUiPromptHistory ? () => onOpenUiPromptHistory(sessionId) : undefined}
      onReviewRepair={reviewRepairAvailable ? onReviewRepair : undefined}
      reviewRepairRunning={reviewRepairRunning}
      reviewRepairDisabled={reviewRepairDisabled}
      uiPromptHistoryCount={uiPromptHistoryCount}
      uiPromptHistoryLoading={uiPromptHistoryLoading}
      runtimeGuidancePendingCount={runtimeGuidancePendingCount}
      runtimeGuidanceAppliedCount={runtimeGuidanceAppliedCount}
      runtimeGuidanceLastAppliedAt={runtimeGuidanceLastAppliedAt}
      runtimeGuidanceItems={runtimeGuidanceItems}
      onCompleteTask={onCompleteTask}
      onDeleteTask={onDeleteTask}
      onEditTask={onEditTask}
      taskModalOpen={taskModalOpen}
      taskModalMode={taskModalMode}
      taskModalTask={taskModalTask}
      taskModalError={taskModalError}
      onCloseTaskModal={onCloseTaskModal}
      onSubmitTaskModal={onSubmitTaskModal}
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
      onGuide={onGuide}
      onStop={onStop}
      disabled={inputDisabled}
      isStreaming={isStreaming}
      isStopping={isStopping}
      placeholder={t('chat.inputPlaceholder')}
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
      showProjectSelector={showProjectSelector}
      showProjectFileButton={showProjectFileButton}
      workspaceRoot={workspaceRoot}
      onWorkspaceRootChange={onWorkspaceRootChange}
      currentRemoteConnectionId={currentRemoteConnectionId}
      currentAgent={currentAgent}
      availableRemoteConnections={availableRemoteConnections}
      onRemoteConnectionChange={onRemoteConnectionChange}
      showWorkspaceRootPicker={showWorkspaceRootPicker}
      mcpEnabled={mcpEnabled}
      enabledMcpIds={enabledMcpIds}
      onMcpEnabledChange={onMcpEnabledChange}
      onEnabledMcpIdsChange={onEnabledMcpIdsChange}
    />
  </div>
  );
};

export default ChatComposerPanel;
