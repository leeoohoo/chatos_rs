import React from 'react';

import { InputArea } from '../../InputArea';
import { MessageList } from '../../MessageList';
import type { Project, Session } from '../../../types';
import type { ContactItem } from './types';
import TeamMemberSummaryView from './TeamMemberSummaryView';

interface TeamMemberWorkspaceProps {
  project: Project;
  selectedContact: ContactItem | null;
  selectedProjectSession: Session | null;
  isSelectedSessionActive: boolean;
  sessionSummaryPaneVisible: boolean;
  summaryItems: any[];
  summaryLoading: boolean;
  summaryError: string | null;
  clearingSummaries: boolean;
  deletingSummaryId: string | null;
  messages: any[];
  hasMoreMessages: boolean;
  chatIsLoading: boolean;
  chatIsStreaming: boolean;
  chatIsStopping: boolean;
  selectedModelId: string | null;
  aiModelConfigs: any[];
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  onLoadMore: () => void;
  onToggleTurnProcess: (userMessageId: string) => void;
  onClearSummaries: () => void;
  onRefreshSummaries: () => void;
  onCloseSummary: () => void;
  onDeleteSummary: (summaryId: string) => void;
  onSend: (
    content: string,
    attachments?: File[],
    runtimeOptions?: {
      mcpEnabled?: boolean;
      projectId?: string | null;
      projectRoot?: string | null;
      workspaceRoot?: string | null;
      enabledMcpIds?: string[];
    },
  ) => void | Promise<void>;
  onGuide: (content: string) => void | Promise<void>;
  onStop: () => void;
  onModelChange: (modelId: string | null) => void;
  onReasoningToggle: (enabled: boolean) => void;
  onMcpEnabledChange: (enabled: boolean) => void;
  onEnabledMcpIdsChange: (ids: string[]) => void;
}

const TeamMemberWorkspace: React.FC<TeamMemberWorkspaceProps> = ({
  project,
  selectedContact,
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
}) => (
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

    {selectedContact && (
      <InputArea
        onSend={onSend}
        onGuide={onGuide}
        onStop={onStop}
        disabled={!isSelectedSessionActive || chatIsStopping}
        isStreaming={chatIsStreaming}
        isStopping={chatIsStopping}
        placeholder={`给 ${selectedContact.name} 发送消息...`}
        allowAttachments={true}
        showModelSelector={true}
        selectedModelId={selectedModelId}
        availableModels={aiModelConfigs}
        onModelChange={onModelChange}
        reasoningSupported={supportsReasoning}
        reasoningEnabled={reasoningEnabled}
        onReasoningToggle={onReasoningToggle}
        availableProjects={[project]}
        currentProject={project}
        selectedProjectId={project.id}
        onProjectChange={() => {}}
        showProjectSelector={false}
        showProjectFileButton={false}
        showWorkspaceRootPicker={false}
        mcpEnabled={mcpEnabled}
        enabledMcpIds={enabledMcpIds}
        onMcpEnabledChange={onMcpEnabledChange}
        onEnabledMcpIdsChange={onEnabledMcpIdsChange}
      />
    )}
  </div>
);

export default TeamMemberWorkspace;
