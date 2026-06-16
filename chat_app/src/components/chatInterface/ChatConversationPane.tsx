import React, { useState, type ComponentProps } from 'react';

import { MessageList } from '../MessageList';
import { MessageTaskDrawer } from '../messageTasks/MessageTaskDrawer';
import ConversationUserMessagesSidebar from '../userMessages/ConversationUserMessagesSidebar';
import { useUserMessageHistoryAnchor } from '../userMessages/useUserMessageHistoryAnchor';
import ChatComposerPanel from './ChatComposerPanel';
import SummaryPane from './SummaryPane';
import type {
  ChatInterfaceProps,
  Message,
  Project,
  RemoteConnection,
  SendMessageHandler,
  Session,
} from '../../types';
import { useI18n } from '../../i18n/I18nProvider';

type SummaryPaneProps = ComponentProps<typeof SummaryPane>;
type ChatComposerPanelProps = ComponentProps<typeof ChatComposerPanel>;

interface ChatConversationPaneProps {
  currentSession: Session | null;
  sessionSummaryPaneVisible: boolean;
  currentContactName: string;
  currentContactId: string | null;
  currentProjectNameForMemory: string;
  currentProjectIdForMemory: string | null;
  messages: Message[];
  hasMoreMessages: boolean;
  onLoadMore: () => void | Promise<void>;
  customRenderer?: ChatInterfaceProps['customRenderer'];
  sessionMemorySummaries: SummaryPaneProps['sessionSummaries'];
  agentRecalls: SummaryPaneProps['agentRecalls'];
  memoryLoading: boolean;
  memoryError: string | null;
  onRefreshMemory: (sessionId: string) => void;
  onCloseSummary: () => void;
  runtimeContextOpen?: boolean;
  runtimeContextSessionId?: string | null;
  onToggleSessionSummary?: (sessionId: string) => void;
  onOpenSessionRuntimeContext?: (sessionId: string) => void;
  toggleSidebar: () => void;
  onSend: SendMessageHandler;
  inputDisabled: boolean;
  supportedFileTypes: string[];
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
  onReasoningToggle: (enabled: boolean) => void;
  selectedModelId: string | null;
  selectedModelName?: string | null;
  selectedThinkingLevel?: string | null;
  availableModels: ChatComposerPanelProps['availableModels'];
  onModelChange: (modelId: string | null) => void;
  onModelNameChange?: (modelName: string | null) => void;
  onThinkingLevelChange?: (level: string | null) => void;
  onModelRuntimeChange?: (selection: {
    selectedModelId?: string | null;
    selectedModelName?: string | null;
    selectedThinkingLevel?: string | null;
  }) => void;
  availableProjects: Project[];
  currentProject: Project | null;
  onProjectChange: (projectId: string | null) => void;
  workspaceRoot: string | null;
  onWorkspaceRootChange: (path: string | null) => void;
  currentRemoteConnectionId?: string | null;
  availableRemoteConnections?: RemoteConnection[];
  onRemoteConnectionChange?: (connectionId: string | null) => void;
}

interface ChatMessagesPaneProps {
  currentSession: Session | null;
  sessionSummaryPaneVisible: boolean;
  currentContactName: string;
  userMessageSidebarVisible: boolean;
  currentProjectNameForMemory: string;
  currentProjectIdForMemory: string | null;
  messages: Message[];
  hasMoreMessages: boolean;
  onLoadMore: () => void | Promise<void>;
  anchorMessageId?: string | null;
  anchorRequestKey?: number;
  onAnchorClear?: () => void;
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
  userMessageSidebarVisible,
  currentProjectNameForMemory,
  currentProjectIdForMemory,
  messages,
  hasMoreMessages,
  onLoadMore,
  anchorMessageId,
  anchorRequestKey,
  onAnchorClear,
  customRenderer,
  sessionMemorySummaries,
  agentRecalls,
  memoryLoading,
  memoryError,
  onRefreshMemory,
  onCloseSummary,
  toggleSidebar,
}) => {
  const { t } = useI18n();

  if (!currentSession) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-muted-foreground mb-2">
            {t('chat.welcomeTitle')}
          </h2>
          <p className="text-muted-foreground mb-4">
            {t('chat.welcomeDescription')}
          </p>
          <button
            onClick={toggleSidebar}
            className="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
          >
            {t('chat.expandSidebar')}
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
        hasMore={hasMoreMessages}
        onLoadMore={onLoadMore}
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
      isLoading={false}
      isStreaming={false}
      isStopping={false}
      streamingPhase={null}
      streamingPreviewText=""
      assistantContactName={currentContactName}
      anchorMessageId={anchorMessageId}
      anchorRequestKey={anchorRequestKey}
      autoScrollToLatest={!userMessageSidebarVisible}
      onAnchorClear={onAnchorClear}
      hasMore={hasMoreMessages}
      onLoadMore={onLoadMore}
      customRenderer={customRenderer}
    />
  );
});

ChatMessagesPane.displayName = 'ChatMessagesPane';

const ChatConversationPane: React.FC<ChatConversationPaneProps> = ({
  currentSession,
  sessionSummaryPaneVisible,
  currentContactName,
  currentContactId,
  currentProjectNameForMemory,
  currentProjectIdForMemory,
  messages,
  hasMoreMessages,
  onLoadMore,
  customRenderer,
  sessionMemorySummaries,
  agentRecalls,
  memoryLoading,
  memoryError,
  onRefreshMemory,
  onCloseSummary,
  runtimeContextOpen = false,
  runtimeContextSessionId = null,
  onToggleSessionSummary,
  onOpenSessionRuntimeContext,
  toggleSidebar,
  onSend,
  inputDisabled,
  supportedFileTypes,
  supportsReasoning,
  reasoningEnabled,
  onReasoningToggle,
  selectedModelId,
  selectedModelName,
  selectedThinkingLevel,
  availableModels,
  onModelChange,
  onModelNameChange,
  onThinkingLevelChange,
  onModelRuntimeChange,
  availableProjects,
  currentProject,
  onProjectChange,
  workspaceRoot,
  onWorkspaceRootChange,
  currentRemoteConnectionId,
  availableRemoteConnections,
  onRemoteConnectionChange,
}) => {
  const [taskMessage, setTaskMessage] = useState<Message | null>(null);
  const userMessageSidebarVisible = Boolean(currentSession?.id && currentContactId);
  const {
    anchorMessageId,
    anchorRequestKey,
    handleSelectUserMessage,
    handleLoadMoreUserMessagesHistory,
    handleClearAnchor,
  } = useUserMessageHistoryAnchor({
    sessionId: userMessageSidebarVisible ? currentSession?.id : null,
    messages,
    hasMoreMessages,
    onLoadMore,
  });

  return (
    <div className="flex-1 min-h-0 flex overflow-hidden">
      {userMessageSidebarVisible ? (
        <ConversationUserMessagesSidebar
          sessionId={currentSession?.id || null}
          className="w-[360px]"
          summaryActive={sessionSummaryPaneVisible}
          runtimeContextActive={Boolean(
            runtimeContextOpen
            && runtimeContextSessionId
            && currentSession?.id === runtimeContextSessionId,
          )}
          onOpenSummary={onToggleSessionSummary && currentSession?.id ? () => {
            onToggleSessionSummary(currentSession.id);
          } : undefined}
          onOpenRuntimeContext={onOpenSessionRuntimeContext && currentSession?.id ? () => {
            onOpenSessionRuntimeContext(currentSession.id);
          } : undefined}
          onSelectMessage={handleSelectUserMessage}
          onLoadMoreHistory={handleLoadMoreUserMessagesHistory}
          onOpenTasks={setTaskMessage}
        />
      ) : null}
      <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
        <div className="flex-1 overflow-hidden">
          <ChatMessagesPane
            currentSession={currentSession}
            sessionSummaryPaneVisible={sessionSummaryPaneVisible}
            currentContactName={currentContactName}
            userMessageSidebarVisible={userMessageSidebarVisible}
            currentProjectNameForMemory={currentProjectNameForMemory}
            currentProjectIdForMemory={currentProjectIdForMemory}
            messages={messages}
            hasMoreMessages={hasMoreMessages}
            onLoadMore={onLoadMore}
            anchorMessageId={anchorMessageId}
            anchorRequestKey={anchorRequestKey}
            onAnchorClear={handleClearAnchor}
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
            onSend={onSend}
            inputDisabled={inputDisabled}
            supportedFileTypes={supportedFileTypes}
            reasoningSupported={supportsReasoning}
            reasoningEnabled={reasoningEnabled}
            onReasoningToggle={onReasoningToggle}
            selectedModelId={selectedModelId}
            selectedModelName={selectedModelName}
            selectedThinkingLevel={selectedThinkingLevel}
            availableModels={availableModels}
            onModelChange={onModelChange}
            onModelNameChange={onModelNameChange}
            onThinkingLevelChange={onThinkingLevelChange}
            onModelRuntimeChange={onModelRuntimeChange}
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
          />
        )}
      </div>

      {taskMessage ? (
        <MessageTaskDrawer
          open
          message={taskMessage}
          onClose={() => setTaskMessage(null)}
        />
      ) : null}
    </div>
  );
};

export default ChatConversationPane;
