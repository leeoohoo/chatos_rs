import React, { type ComponentProps } from 'react';

import { MessageList } from '../MessageList';
import ChatComposerPanel from './ChatComposerPanel';
import SummaryPane from './SummaryPane';
import type { RuntimeGuidanceWorkbarItem } from '../TaskWorkbar';
import type {
  ChatInterfaceProps,
  Message,
  Project,
  RemoteConnection,
  Session,
} from '../../types';

type SummaryPaneProps = ComponentProps<typeof SummaryPane>;
type ChatComposerPanelProps = ComponentProps<typeof ChatComposerPanel>;

interface ChatConversationPaneProps {
  currentSession: Session | null;
  sessionSummaryPaneVisible: boolean;
  currentContactName: string;
  currentProjectNameForMemory: string;
  currentProjectIdForMemory: string | null;
  messages: Message[];
  chatIsLoading: boolean;
  chatIsStreaming: boolean;
  chatIsStopping: boolean;
  hasMoreMessages: boolean;
  onLoadMore: () => void;
  onToggleTurnProcess: (userMessageId: string) => void;
  customRenderer?: ChatInterfaceProps['customRenderer'];
  sessionMemorySummaries: SummaryPaneProps['sessionSummaries'];
  agentRecalls: SummaryPaneProps['agentRecalls'];
  memoryLoading: boolean;
  memoryError: string | null;
  onRefreshMemory: (sessionId: string) => void;
  onCloseSummary: () => void;
  toggleSidebar: () => void;
  mergedCurrentTurnTasks: ChatComposerPanelProps['mergedCurrentTurnTasks'];
  workbarHistoryTasks: ChatComposerPanelProps['workbarHistoryTasks'];
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
  onConfirmTask: ChatComposerPanelProps['onConfirmTask'];
  onPauseTask: ChatComposerPanelProps['onPauseTask'];
  onResumeTask: ChatComposerPanelProps['onResumeTask'];
  onCompleteTask: ChatComposerPanelProps['onCompleteTask'];
  onDeleteTask: ChatComposerPanelProps['onDeleteTask'];
  onEditTask: ChatComposerPanelProps['onEditTask'];
  activeUiPromptPanel: ChatComposerPanelProps['activeUiPromptPanel'];
  onUiPromptSubmit: ChatComposerPanelProps['onUiPromptSubmit'];
  onUiPromptCancel: () => void;
  activeTaskReviewPanel: ChatComposerPanelProps['activeTaskReviewPanel'];
  onTaskReviewConfirm: ChatComposerPanelProps['onTaskReviewConfirm'];
  onTaskReviewCancel: ChatComposerPanelProps['onTaskReviewCancel'];
  onSend: (content: string, attachments?: File[], runtimeOptions?: {
    mcpEnabled?: boolean;
    remoteConnectionId?: string | null;
    projectId?: string | null;
    projectRoot?: string | null;
    workspaceRoot?: string | null;
    enabledMcpIds?: string[];
  }) => void;
  onGuide: (content: string) => void;
  onStop: () => void;
  inputDisabled: boolean;
  isStreaming: boolean;
  isStopping: boolean;
  supportedFileTypes: string[];
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
  onReasoningToggle: (enabled: boolean) => void;
  selectedModelId: string | null;
  availableModels: ChatComposerPanelProps['availableModels'];
  onModelChange: (modelId: string | null) => void;
  availableProjects: Project[];
  currentProject: Project | null;
  onProjectChange: (projectId: string | null) => void;
  workspaceRoot: string | null;
  onWorkspaceRootChange: (path: string | null) => void;
  currentRemoteConnectionId?: string | null;
  availableRemoteConnections?: RemoteConnection[];
  onRemoteConnectionChange?: (connectionId: string | null) => void;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  onMcpEnabledChange: (enabled: boolean) => void;
  onEnabledMcpIdsChange: (ids: string[]) => void;
  runtimeGuidancePendingCount?: number;
  runtimeGuidanceAppliedCount?: number;
  runtimeGuidanceLastAppliedAt?: string | null;
  runtimeGuidanceItems?: RuntimeGuidanceWorkbarItem[];
}

interface ChatMessagesPaneProps {
  currentSession: Session | null;
  sessionSummaryPaneVisible: boolean;
  currentContactName: string;
  currentProjectNameForMemory: string;
  currentProjectIdForMemory: string | null;
  messages: Message[];
  chatIsLoading: boolean;
  chatIsStreaming: boolean;
  chatIsStopping: boolean;
  hasMoreMessages: boolean;
  onLoadMore: () => void;
  onToggleTurnProcess: (userMessageId: string) => void;
  customRenderer?: ChatInterfaceProps['customRenderer'];
  sessionMemorySummaries: SummaryPaneProps['sessionSummaries'];
  agentRecalls: SummaryPaneProps['agentRecalls'];
  memoryLoading: boolean;
  memoryError: string | null;
  onRefreshMemory: (sessionId: string) => void;
  onCloseSummary: () => void;
  toggleSidebar: () => void;
}

const ChatMessagesPane: React.FC<ChatMessagesPaneProps> = React.memo(({
  currentSession,
  sessionSummaryPaneVisible,
  currentContactName,
  currentProjectNameForMemory,
  currentProjectIdForMemory,
  messages,
  chatIsLoading,
  chatIsStreaming,
  chatIsStopping,
  hasMoreMessages,
  onLoadMore,
  onToggleTurnProcess,
  customRenderer,
  sessionMemorySummaries,
  agentRecalls,
  memoryLoading,
  memoryError,
  onRefreshMemory,
  onCloseSummary,
  toggleSidebar,
}) => {
  if (!currentSession) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-muted-foreground mb-2">
            欢迎使用 AI 聊天
          </h2>
          <p className="text-muted-foreground mb-4">
            点击左上角按钮选择联系人，或先添加联系人开始对话
          </p>
          <button
            onClick={toggleSidebar}
            className="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
          >
            展开联系人列表
          </button>
        </div>
      </div>
    );
  }

  if (sessionSummaryPaneVisible) {
    return (
      <SummaryPane
        sessionId={currentSession.id}
        sessionTitle={currentSession.title}
        contactName={currentContactName}
        projectName={currentProjectNameForMemory}
        projectId={currentProjectIdForMemory || null}
        messages={messages}
        isLoading={chatIsLoading}
        isStreaming={chatIsStreaming}
        isStopping={chatIsStopping}
        hasMore={hasMoreMessages}
        onLoadMore={onLoadMore}
        onToggleTurnProcess={onToggleTurnProcess}
        customRenderer={customRenderer}
        sessionSummaries={sessionMemorySummaries}
        agentRecalls={agentRecalls}
        memoryLoading={memoryLoading}
        memoryError={memoryError}
        onRefresh={() => onRefreshMemory(currentSession.id)}
        onClose={onCloseSummary}
      />
    );
  }

  return (
    <MessageList
      key={`messages-${currentSession?.id || 'none'}-chat`}
      sessionId={currentSession?.id}
      messages={messages}
      isLoading={chatIsLoading}
      isStreaming={chatIsStreaming}
      isStopping={chatIsStopping}
      hasMore={hasMoreMessages}
      onLoadMore={onLoadMore}
      onToggleTurnProcess={onToggleTurnProcess}
      customRenderer={customRenderer}
    />
  );
});

ChatMessagesPane.displayName = 'ChatMessagesPane';

const ChatConversationPane: React.FC<ChatConversationPaneProps> = ({
  currentSession,
  sessionSummaryPaneVisible,
  currentContactName,
  currentProjectNameForMemory,
  currentProjectIdForMemory,
  messages,
  chatIsLoading,
  chatIsStreaming,
  chatIsStopping,
  hasMoreMessages,
  onLoadMore,
  onToggleTurnProcess,
  customRenderer,
  sessionMemorySummaries,
  agentRecalls,
  memoryLoading,
  memoryError,
  onRefreshMemory,
  onCloseSummary,
  toggleSidebar,
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
  onConfirmTask,
  onPauseTask,
  onResumeTask,
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
  onGuide,
  onStop,
  inputDisabled,
  isStreaming,
  isStopping,
  supportedFileTypes,
  supportsReasoning,
  reasoningEnabled,
  onReasoningToggle,
  selectedModelId,
  availableModels,
  onModelChange,
  availableProjects,
  currentProject,
  onProjectChange,
  workspaceRoot,
  onWorkspaceRootChange,
  currentRemoteConnectionId,
  availableRemoteConnections,
  onRemoteConnectionChange,
  mcpEnabled,
  enabledMcpIds,
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
  runtimeGuidancePendingCount = 0,
  runtimeGuidanceAppliedCount = 0,
  runtimeGuidanceLastAppliedAt = null,
  runtimeGuidanceItems = [],
}) => {
  const fixedMcpProfile = true;

  return (
    <div className="flex-1 min-h-0 flex overflow-hidden">
      <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
        <div className="flex-1 overflow-hidden">
          <ChatMessagesPane
          currentSession={currentSession}
          sessionSummaryPaneVisible={sessionSummaryPaneVisible}
          currentContactName={currentContactName}
          currentProjectNameForMemory={currentProjectNameForMemory}
          currentProjectIdForMemory={currentProjectIdForMemory}
          messages={messages}
          chatIsLoading={chatIsLoading}
          chatIsStreaming={chatIsStreaming}
          chatIsStopping={chatIsStopping}
          hasMoreMessages={hasMoreMessages}
          onLoadMore={onLoadMore}
          onToggleTurnProcess={onToggleTurnProcess}
          customRenderer={customRenderer}
          sessionMemorySummaries={sessionMemorySummaries}
          agentRecalls={agentRecalls}
          memoryLoading={memoryLoading}
          memoryError={memoryError}
          onRefreshMemory={onRefreshMemory}
          onCloseSummary={onCloseSummary}
          toggleSidebar={toggleSidebar}
        />
        </div>

        {currentSession && (
          <ChatComposerPanel
          sessionId={currentSession.id}
          mergedCurrentTurnTasks={mergedCurrentTurnTasks}
          workbarHistoryTasks={workbarHistoryTasks}
          activeConversationTurnId={activeConversationTurnId}
          workbarLoading={workbarLoading}
          workbarHistoryLoading={workbarHistoryLoading}
          workbarError={workbarError}
          workbarHistoryError={workbarHistoryError}
          workbarActionLoadingTaskId={workbarActionLoadingTaskId}
          onRefreshWorkbarTasks={onRefreshWorkbarTasks}
          onOpenHistory={onOpenHistory}
          onOpenUiPromptHistory={onOpenUiPromptHistory}
          uiPromptHistoryCount={uiPromptHistoryCount}
          uiPromptHistoryLoading={uiPromptHistoryLoading}
          onConfirmTask={onConfirmTask}
          onPauseTask={onPauseTask}
          onResumeTask={onResumeTask}
          onCompleteTask={onCompleteTask}
          onDeleteTask={onDeleteTask}
          onEditTask={onEditTask}
          activeUiPromptPanel={activeUiPromptPanel}
          onUiPromptSubmit={onUiPromptSubmit}
          onUiPromptCancel={onUiPromptCancel}
          activeTaskReviewPanel={activeTaskReviewPanel}
          onTaskReviewConfirm={onTaskReviewConfirm}
          onTaskReviewCancel={onTaskReviewCancel}
          onSend={onSend}
          onGuide={onGuide}
          onStop={onStop}
          inputDisabled={inputDisabled}
          isStreaming={isStreaming}
          isStopping={isStopping}
          supportedFileTypes={supportedFileTypes}
          reasoningSupported={supportsReasoning}
          reasoningEnabled={reasoningEnabled}
          onReasoningToggle={onReasoningToggle}
          selectedModelId={selectedModelId}
          availableModels={availableModels}
          onModelChange={onModelChange}
          availableProjects={availableProjects}
          currentProject={currentProject}
          selectedProjectId={null}
          onProjectChange={onProjectChange}
          showProjectSelector={false}
          showProjectFileButton={false}
          workspaceRoot={workspaceRoot}
          onWorkspaceRootChange={onWorkspaceRootChange}
          currentRemoteConnectionId={currentRemoteConnectionId}
          availableRemoteConnections={availableRemoteConnections}
          onRemoteConnectionChange={onRemoteConnectionChange}
          showWorkspaceRootPicker={true}
          mcpEnabled={mcpEnabled}
          enabledMcpIds={enabledMcpIds}
          onMcpEnabledChange={onMcpEnabledChange}
          onEnabledMcpIdsChange={onEnabledMcpIdsChange}
          fixedMcpProfile={fixedMcpProfile}
          runtimeGuidancePendingCount={runtimeGuidancePendingCount}
          runtimeGuidanceAppliedCount={runtimeGuidanceAppliedCount}
          runtimeGuidanceLastAppliedAt={runtimeGuidanceLastAppliedAt}
          runtimeGuidanceItems={runtimeGuidanceItems}
        />
        )}
      </div>
    </div>
  );
};

export default ChatConversationPane;
