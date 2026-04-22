import React, { useMemo } from 'react';

import ChatComposerPanel from '../../chatInterface/ChatComposerPanel';
import { buildSupportedFileTypes, resolveModelSupportFlags } from '../../chatInterface/helpers';
import { MessageList } from '../../MessageList';
import type {
  Message,
  Project,
  RemoteConnection,
  Session,
  AiModelConfig,
  AgentConfig,
} from '../../../types';
import type {
  SendMessageRuntimeOptions,
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptPanelState,
  UiPromptResponsePayload,
} from '../../../lib/store/types';
import type { SessionSummaryItem } from '../../../features/sessionSummary/useSessionSummaryPanel';
import type { RuntimeGuidanceWorkbarItem, TaskWorkbarItem } from '../../TaskWorkbar';
import type { TaskOutcomeDraft } from '../../taskWorkbar/TaskOutcomeModal';
import type { ContactItem } from './types';
import TeamMemberSummaryView from './TeamMemberSummaryView';

interface TeamMemberWorkspaceProps {
  project: Project;
  selectedContact: ContactItem | null;
  currentAgent: AgentConfig | null;
  selectedProjectSession: Session | null;
  isSelectedSessionActive: boolean;
  sessionSummaryPaneVisible: boolean;
  summaryItems: SessionSummaryItem[];
  summaryLoading: boolean;
  summaryError: string | null;
  clearingSummaries: boolean;
  deletingSummaryId: string | null;
  messages: Message[];
  hasMoreMessages: boolean;
  chatIsLoading: boolean;
  chatIsStreaming: boolean;
  chatIsStopping: boolean;
  selectedModelId: string | null;
  aiModelConfigs: AiModelConfig[];
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  availableRemoteConnections: RemoteConnection[];
  currentRemoteConnectionId: string | null;
  onRemoteConnectionChange: (connectionId: string | null) => void;
  onLoadMore: () => void;
  onToggleTurnProcess: (userMessageId: string) => void;
  onClearSummaries: () => void;
  onRefreshSummaries: () => void;
  onCloseSummary: () => void;
  onDeleteSummary: (summaryId: string) => void;
  onSend: (
    content: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => void | Promise<void>;
  onGuide: (content: string) => void | Promise<void>;
  onStop: () => void;
  onModelChange: (modelId: string | null) => void;
  onReasoningToggle: (enabled: boolean) => void;
  onMcpEnabledChange: (enabled: boolean) => void;
  onEnabledMcpIdsChange: (ids: string[]) => void;
  mergedCurrentTurnTasks: TaskWorkbarItem[];
  workbarHistoryTasks: TaskWorkbarItem[];
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
  onOpenWorkbarHistory: (sessionId: string) => void;
  onCompleteTask: (task: TaskWorkbarItem) => void;
  onDeleteTask: (task: TaskWorkbarItem) => void;
  onEditTask: (task: TaskWorkbarItem) => void;
  onCloseTaskModal: () => void;
  onSubmitTaskModal: (draft: TaskOutcomeDraft) => void;
  activeUiPromptPanel: UiPromptPanelState | null;
  onUiPromptSubmit: (payload: UiPromptResponsePayload) => void;
  onUiPromptCancel: () => void;
  activeTaskReviewPanel: TaskReviewPanelState | null;
  onTaskReviewConfirm: (payload: TaskReviewDraft[]) => void;
  onTaskReviewCancel: () => void;
  runtimeGuidancePendingCount?: number;
  runtimeGuidanceAppliedCount?: number;
  runtimeGuidanceLastAppliedAt?: string | null;
  runtimeGuidanceItems?: RuntimeGuidanceWorkbarItem[];
}

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
  aiModelConfigs,
  supportsReasoning,
  reasoningEnabled,
  mcpEnabled,
  enabledMcpIds,
  availableRemoteConnections,
  currentRemoteConnectionId,
  onRemoteConnectionChange,
  onLoadMore,
  onToggleTurnProcess,
  onClearSummaries,
  onRefreshSummaries,
  onCloseSummary,
  onDeleteSummary,
  onSend,
  onGuide,
  onStop,
  onModelChange,
  onReasoningToggle,
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
  mergedCurrentTurnTasks,
  workbarHistoryTasks,
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
  onOpenWorkbarHistory,
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
      {!selectedContact ? (
        <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
          请选择一个团队成员开始对话
        </div>
      ) : !selectedProjectSession ? (
        <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
          正在准备会话...
        </div>
      ) : !isSelectedSessionActive ? (
        <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
          正在切换到 {selectedContact.name} 的会话...
        </div>
      ) : sessionSummaryPaneVisible ? (
        <TeamMemberSummaryView
          sessionId={selectedProjectSession.id}
          sessionTitle={selectedProjectSession.title}
          contactName={selectedContact.name}
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
      ) : (
        <MessageList
          key={`project-team-messages-${selectedProjectSession.id}`}
          sessionId={selectedProjectSession.id}
          messages={messages}
          isLoading={chatIsLoading}
          isStreaming={chatIsStreaming}
          isStopping={chatIsStopping}
          hasMore={hasMoreMessages}
          onLoadMore={onLoadMore}
          onToggleTurnProcess={onToggleTurnProcess}
        />
      )}
    </div>

    {selectedContact && selectedProjectSession ? (
      <ChatComposerPanel
        sessionId={selectedProjectSession.id}
        mergedCurrentTurnTasks={mergedCurrentTurnTasks}
        workbarHistoryTasks={workbarHistoryTasks}
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
        onRefreshWorkbarTasks={onRefreshWorkbarTasks}
        onOpenHistory={onOpenWorkbarHistory}
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
        inputDisabled={!isSelectedSessionActive || chatIsStopping}
        isStreaming={chatIsStreaming}
        isStopping={chatIsStopping}
        supportedFileTypes={supportedFileTypes}
        reasoningSupported={supportsReasoning}
        reasoningEnabled={reasoningEnabled}
        onReasoningToggle={onReasoningToggle}
        selectedModelId={selectedModelId}
        availableModels={aiModelConfigs}
        onModelChange={onModelChange}
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
        onMcpEnabledChange={onMcpEnabledChange}
        onEnabledMcpIdsChange={onEnabledMcpIdsChange}
        runtimeGuidancePendingCount={runtimeGuidancePendingCount}
        runtimeGuidanceAppliedCount={runtimeGuidanceAppliedCount}
        runtimeGuidanceLastAppliedAt={runtimeGuidanceLastAppliedAt}
        runtimeGuidanceItems={runtimeGuidanceItems}
      />
    ) : null}
    </div>
  );
};

export default TeamMemberWorkspace;
