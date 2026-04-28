import React from 'react';

import { MessageList } from '../../MessageList';
import TeamMemberSummaryView from './TeamMemberSummaryView';
import type { TeamMemberWorkspaceProps } from './TeamMemberWorkspaceTypes';

type TeamMemberWorkspaceContentProps = Pick<
  TeamMemberWorkspaceProps,
  | 'selectedContact'
  | 'selectedProjectSession'
  | 'isSelectedSessionActive'
  | 'sessionSummaryPaneVisible'
  | 'summaryItems'
  | 'summaryLoading'
  | 'summaryError'
  | 'clearingSummaries'
  | 'deletingSummaryId'
  | 'messages'
  | 'hasMoreMessages'
  | 'chatIsLoading'
  | 'chatIsStreaming'
  | 'chatIsStopping'
  | 'onLoadMore'
  | 'onToggleTurnProcess'
  | 'onClearSummaries'
  | 'onRefreshSummaries'
  | 'onCloseSummary'
  | 'onDeleteSummary'
>;

export const TeamMemberWorkspaceContent: React.FC<TeamMemberWorkspaceContentProps> = ({
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
  onLoadMore,
  onToggleTurnProcess,
  onClearSummaries,
  onRefreshSummaries,
  onCloseSummary,
  onDeleteSummary,
}) => {
  if (!selectedContact) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
        请选择一个团队成员开始对话
      </div>
    );
  }
  if (!selectedProjectSession) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
        正在准备会话...
      </div>
    );
  }
  if (!isSelectedSessionActive) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
        正在切换到 {selectedContact.name} 的会话...
      </div>
    );
  }
  if (sessionSummaryPaneVisible) {
    return (
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
    );
  }

  return (
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
  );
};
