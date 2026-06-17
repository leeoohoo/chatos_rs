import React, { useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { cn } from '../../lib/utils';
import type { Message, Project } from '../../types';
import TurnRuntimeContextDrawer from '../chatInterface/TurnRuntimeContextDrawer';
import { MessageTaskDrawer } from '../messageTasks/MessageTaskDrawer';
import ConversationUserMessagesSidebar from '../userMessages/ConversationUserMessagesSidebar';
import { useUserMessageHistoryAnchor } from '../userMessages/useUserMessageHistoryAnchor';
import TeamMemberWorkspace from './teamMembers/TeamMemberWorkspace';
import { useTeamMembersPaneModel } from './teamMembers/useTeamMembersPaneModel';

interface TeamMembersPaneProps {
  project: Project;
  className?: string;
}

const TeamMembersPane: React.FC<TeamMembersPaneProps> = ({ project, className }) => {
  const { t } = useI18n();
  const [taskMessage, setTaskMessage] = useState<Message | null>(null);
  const {
    workspaceProps,
    runtimeContextDrawerProps,
    userMessageSidebarActions,
  } = useTeamMembersPaneModel({ project });
  const activeSessionId = project.latestSessionId || null;
  const {
    anchorMessageId,
    anchorRequestKey,
    handleSelectUserMessage,
    handleLoadMoreUserMessagesHistory,
    handleClearAnchor,
  } = useUserMessageHistoryAnchor({
    sessionId: activeSessionId,
    messages: workspaceProps.messages,
    hasMoreMessages: workspaceProps.hasMoreMessages,
    onLoadMore: workspaceProps.onLoadMore,
  });

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        {t('runSettings.selectProject')}
      </div>
    );
  }

  return (
    <div className={cn('flex h-full overflow-hidden', className)}>
      <ConversationUserMessagesSidebar
        sessionId={activeSessionId}
        className="w-[400px]"
        summaryActive={userMessageSidebarActions.summaryActive}
        runtimeContextActive={userMessageSidebarActions.runtimeContextActive}
        summaryLoading={userMessageSidebarActions.summaryLoading}
        runtimeContextLoading={userMessageSidebarActions.runtimeContextLoading}
        summaryDisabled={userMessageSidebarActions.summaryDisabled}
        runtimeContextDisabled={userMessageSidebarActions.runtimeContextDisabled}
        onOpenSummary={userMessageSidebarActions.onOpenSummary}
        onOpenRuntimeContext={userMessageSidebarActions.onOpenRuntimeContext}
        onSelectMessage={handleSelectUserMessage}
        onLoadMoreHistory={handleLoadMoreUserMessagesHistory}
        onOpenTasks={setTaskMessage}
      />
      <TeamMemberWorkspace
        {...workspaceProps}
        anchorMessageId={anchorMessageId}
        anchorRequestKey={anchorRequestKey}
        onAnchorClear={handleClearAnchor}
      />
      <TurnRuntimeContextDrawer {...runtimeContextDrawerProps} />
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

export default TeamMembersPane;
