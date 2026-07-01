// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
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
  | 'anchorMessageId'
  | 'anchorRequestKey'
  | 'onAnchorClear'
  | 'onLoadMore'
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
  anchorMessageId,
  anchorRequestKey,
  onAnchorClear,
  onLoadMore,
  onClearSummaries,
  onRefreshSummaries,
  onCloseSummary,
  onDeleteSummary,
}) => {
  const { t } = useI18n();

  if (!selectedContact) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
        {t('teamMembers.selectPrompt')}
      </div>
    );
  }
  if (!selectedProjectSession) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
        {t('teamMembers.preparingSession')}
      </div>
    );
  }
  if (!isSelectedSessionActive) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
        {t('teamMembers.switchingSession', { name: selectedContact.name })}
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
        onLoadMore={onLoadMore}
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
      isLoading={false}
      isStreaming={false}
      isStopping={false}
      assistantContactName={selectedContact.name}
      anchorMessageId={anchorMessageId}
      anchorRequestKey={anchorRequestKey}
      autoScrollToLatest={false}
      onAnchorClear={onAnchorClear}
    />
  );
};
