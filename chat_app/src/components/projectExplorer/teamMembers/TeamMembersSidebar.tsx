import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { cn } from '../../../lib/utils';
import SessionBusyBadge from '../../chat/SessionBusyBadge';
import {
  countPendingSessionPanels,
  resolveSessionBusyPhase,
} from '../../chat/sessionBusyState';
import type {
  ContactItem,
  ProjectContactRow,
  SessionChatStateMap,
  TaskReviewPanelsBySessionMap,
  UiPromptPanelsBySessionMap,
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
  runtimeContextSessionId: string | null;
  openingRuntimeContextContactId: string | null;
  removingContactId: string | null;
  sessionChatState?: SessionChatStateMap;
  taskReviewPanelsBySession?: TaskReviewPanelsBySessionMap;
  uiPromptPanelsBySession?: UiPromptPanelsBySessionMap;
  onOpenAddMember: () => void;
  onSelectContact: (contactId: string) => void;
  onOpenSummary: (contact: ContactItem) => void;
  onOpenRuntimeContext: (contact: ContactItem) => void;
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
  runtimeContextSessionId,
  openingRuntimeContextContactId,
  removingContactId,
  sessionChatState,
  taskReviewPanelsBySession = {},
  uiPromptPanelsBySession = {},
  onOpenAddMember,
  onSelectContact,
  onOpenSummary,
  onOpenRuntimeContext,
  onRemoveMember,
}) => {
  const { t } = useI18n();

  return (
    <div className="w-64 shrink-0 border-r border-border bg-card/40 flex flex-col">
      <div className="px-3 py-2 border-b border-border space-y-2">
        <div className="flex items-center justify-between gap-2">
          <div className="text-xs uppercase tracking-wide text-muted-foreground">{t('teamMembers.title')}</div>
          <button
            type="button"
            className="px-2 py-1 text-xs rounded border border-border text-muted-foreground hover:text-foreground hover:bg-accent"
            onClick={onOpenAddMember}
          >
            {t('teamMembers.add')}
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
            {t('teamMembers.loading')}
          </div>
        ) : projectContacts.length === 0 ? (
          <div className="text-xs text-muted-foreground px-2 py-3">
            {t('teamMembers.empty')}
          </div>
        ) : (
          projectContacts.map(({ contact, session }) => {
            const active = selectedContactId === contact.id;
            const switching = switchingContactId === contact.id;
            const chatState = session?.id ? sessionChatState?.[session.id] : undefined;
            const runtimeSessionId = session?.id || '';
            const {
              taskReviewCount,
              uiPromptCount,
              pendingCount,
            } = countPendingSessionPanels({
              sessionId: runtimeSessionId,
              taskReviewPanelsBySession,
              uiPromptPanelsBySession,
            });
            const streamingPhase = resolveSessionBusyPhase({
              chatState,
              pendingTaskReviewCount: taskReviewCount,
              pendingUiPromptCount: uiPromptCount,
            });
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
                        ? t('teamMembers.loadingShort')
                        : (summaryPaneSessionId && session?.id === summaryPaneSessionId ? t('teamMembers.summaryClose') : t('teamMembers.summary'))}
                    </button>
                    <button
                      type="button"
                      className={cn(
                        'px-1.5 py-0.5 text-[11px] rounded border border-border text-muted-foreground hover:text-foreground hover:bg-accent',
                        runtimeContextSessionId && session?.id === runtimeContextSessionId && 'text-blue-600 border-blue-200',
                      )}
                      onClick={(event) => {
                        event.stopPropagation();
                        onOpenRuntimeContext(contact);
                      }}
                      disabled={openingRuntimeContextContactId === contact.id}
                    >
                      {openingRuntimeContextContactId === contact.id
                        ? t('teamMembers.loadingShort')
                        : (runtimeContextSessionId && session?.id === runtimeContextSessionId ? t('teamMembers.contextClose') : t('teamMembers.context'))}
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
                      {removingContactId === contact.id ? t('teamMembers.removing') : t('teamMembers.remove')}
                    </button>
                  </div>
                </div>
                <div className="mt-1 text-[11px] text-muted-foreground truncate">
                  {switching ? (
                    t('teamMembers.switching')
                  ) : (
                    <span className="inline-flex items-center gap-2">
                      <span>{t('teamMembers.sessionLabel', { title: session?.title || t('teamMembers.sessionNotCreated') })}</span>
                      {session?.id ? (
                        <SessionBusyBadge phase={streamingPhase} />
                      ) : null}
                      {pendingCount > 0 ? (
                        <span className="inline-flex items-center gap-1 text-blue-600">
                          <span className="inline-block w-2 h-2 rounded-full bg-blue-500 animate-pulse" />
                          {t('teamMembers.pending', { count: pendingCount })}
                        </span>
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
