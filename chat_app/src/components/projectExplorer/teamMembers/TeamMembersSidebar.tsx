import React from 'react';

import { cn } from '../../../lib/utils';
import SessionBusyBadge from '../../chat/SessionBusyBadge';
import type {
  ContactItem,
  ProjectContactRow,
  SessionChatStateMap,
} from './types';

interface TeamMembersSidebarProps {
  projectName: string;
  projectMembersLoading: boolean;
  projectMembersError?: string | null;
  memberPickerError?: string | null;
  projectContacts: ProjectContactRow[];
  selectedContactId: string | null;
  switchingContactId: string | null;
  summaryPaneSessionId: string | null;
  openingSummaryContactId: string | null;
  removingContactId: string | null;
  sessionChatState?: SessionChatStateMap;
  onOpenAddMember: () => void;
  onSelectContact: (contactId: string) => void;
  onOpenSummary: (contact: ContactItem) => void;
  onRemoveMember: (contact: ContactItem) => void;
}

const TeamMembersSidebar: React.FC<TeamMembersSidebarProps> = ({
  projectName,
  projectMembersLoading,
  projectMembersError,
  memberPickerError,
  projectContacts,
  selectedContactId,
  switchingContactId,
  summaryPaneSessionId,
  openingSummaryContactId,
  removingContactId,
  sessionChatState,
  onOpenAddMember,
  onSelectContact,
  onOpenSummary,
  onRemoveMember,
}) => {
  return (
    <div className="w-64 shrink-0 border-r border-border bg-card/40 flex flex-col">
      <div className="px-3 py-2 border-b border-border space-y-2">
        <div className="flex items-center justify-between gap-2">
          <div className="text-xs uppercase tracking-wide text-muted-foreground">团队成员</div>
          <button
            type="button"
            className="px-2 py-1 text-xs rounded border border-border text-muted-foreground hover:text-foreground hover:bg-accent"
            onClick={onOpenAddMember}
          >
            添加
          </button>
        </div>
        <div className="text-sm font-medium text-foreground truncate" title={projectName}>{projectName}</div>
        {projectMembersError && (
          <div className="text-[11px] text-destructive">{projectMembersError}</div>
        )}
        {memberPickerError && (
          <div className="text-[11px] text-destructive">{memberPickerError}</div>
        )}
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-1">
        {projectMembersLoading ? (
          <div className="text-xs text-muted-foreground px-2 py-3">
            正在加载项目成员...
          </div>
        ) : projectContacts.length === 0 ? (
          <div className="text-xs text-muted-foreground px-2 py-3">
            当前项目暂无已添加联系人，请点击上方“添加”按钮。
          </div>
        ) : (
          projectContacts.map(({ contact, session }) => {
            const active = selectedContactId === contact.id;
            const switching = switchingContactId === contact.id;
            const chatState = session?.id ? sessionChatState?.[session.id] : undefined;
            const isBusy = Boolean(chatState?.isLoading || chatState?.isStreaming);
            return (
              <div
                key={contact.id}
                role="button"
                tabIndex={0}
                onClick={() => onSelectContact(contact.id)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    onSelectContact(contact.id);
                  }
                }}
                className={cn(
                  'w-full text-left rounded-md border px-2 py-2 transition-colors cursor-pointer',
                  active
                    ? 'bg-accent border-border'
                    : 'border-transparent hover:bg-accent/60'
                )}
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="text-sm font-medium text-foreground truncate">{contact.name}</div>
                  <div className="flex items-center gap-1 shrink-0">
                    <button
                      type="button"
                      className={cn(
                        'px-1.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground hover:text-foreground hover:bg-accent',
                        summaryPaneSessionId && session?.id === summaryPaneSessionId && 'text-blue-600 border-blue-200',
                      )}
                      onClick={(event) => {
                        event.stopPropagation();
                        onOpenSummary(contact);
                      }}
                      disabled={openingSummaryContactId === contact.id}
                    >
                      {openingSummaryContactId === contact.id
                        ? '加载中'
                        : (summaryPaneSessionId && session?.id === summaryPaneSessionId ? '关闭总结' : '总结')}
                    </button>
                    <button
                      type="button"
                      className="px-1.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground hover:text-destructive hover:border-destructive"
                      onClick={(event) => {
                        event.stopPropagation();
                        onRemoveMember(contact);
                      }}
                      disabled={removingContactId === contact.id}
                    >
                      {removingContactId === contact.id ? '移除中' : '移除'}
                    </button>
                  </div>
                </div>
                <div className="mt-1 text-[11px] text-muted-foreground truncate">
                  {switching ? (
                    '切换中...'
                  ) : (
                    <span className="inline-flex items-center gap-2">
                      <span>{`会话: ${session?.title || '未创建'}`}</span>
                      {session?.id ? (
                        <SessionBusyBadge busy={isBusy} />
                      ) : null}
                    </span>
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};

export default TeamMembersSidebar;
