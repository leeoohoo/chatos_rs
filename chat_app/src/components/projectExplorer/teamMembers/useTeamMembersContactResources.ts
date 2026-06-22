import { useCallback, useMemo } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { useContactSessionResolver } from '../../../features/contactSession/useContactSessionResolver';
import {
  findBestMatchedSession,
  hasSessionMessages,
  isSessionMatchedContactAndProject,
  normalizeProjectScopeId,
  resolveSessionTimestamp,
} from '../../../features/contactSession/sessionResolver';
import { useSessionSummaryPanel } from '../../../features/sessionSummary/useSessionSummaryPanel';
import { normalizeProjectMemberContactsFromRecords } from '../../../lib/domain/projectMembers';
import type { Project, Session } from '../../../types';
import type {
  ContactItem,
  EnsureProjectContactSessionOptions,
  ProjectContactRow,
} from './types';
import { useProjectMembersManager } from './useProjectMembersManager';
import { useTeamMemberConversation } from './useTeamMemberConversation';
import { useTeamMembersPaneStoreBridge } from './useTeamMembersPaneStoreBridge';

interface UseTeamMembersContactResourcesOptions {
  project: Project;
  store: ReturnType<typeof useTeamMembersPaneStoreBridge>;
}

export const resolveProjectContactSession = ({
  currentSession,
  contact,
  normalizedProjectId,
  preferredSessionId,
  preferredSessionHasMessages,
  findProjectSessionForContact,
}: {
  currentSession: Session | null | undefined;
  contact: ContactItem;
  normalizedProjectId: string;
  preferredSessionId?: string | null;
  preferredSessionHasMessages?: boolean;
  findProjectSessionForContact: (contact: ContactItem, preferredSessionId?: string | null) => Session | null;
}): Session | null => {
  const normalizedPreferredSessionId = typeof preferredSessionId === 'string'
    ? preferredSessionId.trim()
    : '';
  if (
    currentSession
    && isSessionMatchedContactAndProject(currentSession, contact, normalizedProjectId)
    && (
      !normalizedPreferredSessionId
      || currentSession.id === normalizedPreferredSessionId
      || (preferredSessionHasMessages !== true && hasSessionMessages(currentSession))
    )
  ) {
    return currentSession;
  }
  const resolved = findProjectSessionForContact(contact, normalizedPreferredSessionId);
  if (
    preferredSessionHasMessages === true
    && normalizedPreferredSessionId
    && resolved?.id !== normalizedPreferredSessionId
    && !hasSessionMessages(resolved)
  ) {
    return null;
  }
  return resolved;
};

export const useTeamMembersContactResources = ({
  project,
  store,
}: UseTeamMembersContactResourcesOptions) => {
  const { t } = useI18n();
  const {
    apiClient,
    currentSession,
    sessions,
    contacts,
    loadContacts,
    sendMessage,
    loadMoreMessages,
    createSession,
    selectSession,
    selectedModelId,
  } = store;

  const normalizedProjectId = normalizeProjectScopeId(project.id || null);
  const normalizedContacts = useMemo<ContactItem[]>(
    () => normalizeProjectMemberContactsFromRecords(contacts),
    [contacts],
  );

  const {
    projectMembers,
    projectMembersLoading,
    projectMembersError,
    projectContactIdSet,
    memberPickerOpen,
    memberPickerSelectedId,
    memberPickerError,
    removingContactId,
    openAddMember: openAddMemberFromManager,
    confirmAddMember: confirmAddMemberFromManager,
    removeMember: removeMemberFromManager,
    closeMemberPicker,
    selectMemberPickerContact,
  } = useProjectMembersManager({
    apiClient,
    projectId: project.id,
    contacts: normalizedContacts,
    loadContacts,
  });

  const findProjectSessionForContact = useCallback((
    contact: ContactItem,
    preferredSessionId?: string | null,
  ): Session | null => {
    return findBestMatchedSession(sessions || [], contact, normalizedProjectId, preferredSessionId);
  }, [normalizedProjectId, sessions]);

  const { ensureContactSession: ensureContactSessionFromResolver } = useContactSessionResolver({
    sessions: sessions || [],
    currentSession,
    createSession,
    apiClient,
    defaultProjectId: normalizedProjectId,
  });

  const projectContacts = useMemo<ProjectContactRow[]>(() => {
    const normalizedContactById = new Map(
      normalizedContacts.map((contact) => [contact.id, contact]),
    );
    const rows = projectMembers.map((member) => {
      const normalizedContact = normalizedContactById.get(member.contactId) || null;
      const contact: ContactItem = {
        id: member.contactId,
        agentId: member.agentId,
        name: member.name,
        taskRunner: normalizedContact?.taskRunner,
      };
      const session = resolveProjectContactSession({
        currentSession,
        contact,
        normalizedProjectId,
        preferredSessionId: member.latestSessionId,
        preferredSessionHasMessages: Boolean(member.lastMessageAt),
        findProjectSessionForContact,
      });
      return {
        contact,
        session,
        latestSessionId: member.latestSessionId,
        lastMessageAt: member.lastMessageAt,
        updatedAt: session ? resolveSessionTimestamp(session) : member.updatedAt,
      };
    });
    rows.sort((a, b) => b.updatedAt - a.updatedAt);
    return rows;
  }, [
    currentSession,
    findProjectSessionForContact,
    normalizedContacts,
    normalizedProjectId,
    projectMembers,
  ]);

  const projectContactsOptions = useMemo(() => normalizedContacts, [normalizedContacts]);

  const {
    summaryPaneSessionId,
    summaryItems,
    summaryLoading,
    summaryError,
    clearingSummaries,
    deletingSummaryId,
    setSummaryPaneSessionId,
    setSummaryError,
    resetSummaryState,
    loadSessionSummaries,
    markSessionSummariesStale,
    hydrateSessionSummariesFromCache,
    cancelPendingSessionSummariesLoad,
    applyRealtimeSessionSummaries,
    openSummaryForSession,
    deleteSummary,
    clearSummaries,
  } = useSessionSummaryPanel(apiClient);

  const ensureContactSession = useCallback(async (
    contact: ContactItem,
    options?: EnsureProjectContactSessionOptions,
  ): Promise<string | null> => {
    const projectMember = projectMembers.find((member) => member.contactId === contact.id) || null;
    return ensureContactSessionFromResolver(contact, {
      projectId: normalizedProjectId,
      title: contact.name || t('teamMembers.contactFallback'),
      selectedModelId: selectedModelId ?? null,
      projectRoot: project.rootPath || null,
      preferredSessionId: projectMember?.latestSessionId || null,
      preferredSessionHasMessages: Boolean(projectMember?.lastMessageAt),
      createIfMissing: options?.createIfMissing,
      createSessionOptions: { keepActivePanel: true, activateSession: false },
    });
  }, [
    ensureContactSessionFromResolver,
    normalizedProjectId,
    projectMembers,
    project.rootPath,
    selectedModelId,
    t,
  ]);

  const conversation = useTeamMemberConversation({
    projectId: normalizedProjectId,
    projectRootPath: project.rootPath || null,
    currentSession,
    projectContacts,
    normalizedContacts,
    summaryPaneSessionId,
    setSummaryPaneSessionId,
    setSummaryError,
    resetSummaryState,
    openSummaryForSession,
    deleteSummary,
    clearSummaries,
    cancelPendingSessionSummariesLoad,
    ensureContactSession,
    selectSession,
    sendMessage,
    loadMoreMessages,
  });

  const handleOpenAddMember = useCallback(async () => {
    await openAddMemberFromManager();
  }, [openAddMemberFromManager]);

  const handleConfirmAddMember = useCallback(async () => {
    const contactId = await confirmAddMemberFromManager();
    if (contactId) {
      await conversation.handleSelectContact(contactId);
    }
  }, [confirmAddMemberFromManager, conversation.handleSelectContact]);

  return {
    normalizedProjectId,
    normalizedContacts,
    ensureContactSession,
    members: {
      projectContacts,
      projectContactsOptions,
      projectMembersLoading,
      projectMembersError,
      projectContactIdSet,
      memberPickerOpen,
      memberPickerSelectedId,
      memberPickerError,
      removingContactId,
      closeMemberPicker,
      selectMemberPickerContact,
      removeMemberFromManager,
      handleOpenAddMember,
      handleConfirmAddMember,
    },
    conversation,
    summary: {
      summaryPaneSessionId,
      summaryItems,
      summaryLoading,
      summaryError,
      clearingSummaries,
      deletingSummaryId,
      setSummaryPaneSessionId,
      resetSummaryState,
      loadSessionSummaries,
      markSessionSummariesStale,
      hydrateSessionSummariesFromCache,
      cancelPendingSessionSummariesLoad,
      applyRealtimeSessionSummaries,
    },
  };
};
