import { useCallback, useMemo } from 'react';

import { useContactSessionResolver } from '../../../features/contactSession/useContactSessionResolver';
import {
  findLatestMatchedSession,
  isSessionMatchedContactAndProject,
  normalizeProjectScopeId,
  resolveSessionTimestamp,
} from '../../../features/contactSession/sessionResolver';
import { useSessionSummaryPanel } from '../../../features/sessionSummary/useSessionSummaryPanel';
import { normalizeProjectMemberContactsFromRecords } from '../../../lib/domain/projectMembers';
import type { AgentConfig, Project, Session } from '../../../types';
import type { ContactItem, ProjectContactRow } from './types';
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
  findProjectSessionForContact,
}: {
  currentSession: Session | null | undefined;
  contact: ContactItem;
  normalizedProjectId: string;
  findProjectSessionForContact: (contact: ContactItem) => Session | null;
}): Session | null => {
  if (
    currentSession
    && isSessionMatchedContactAndProject(currentSession, contact, normalizedProjectId)
  ) {
    return currentSession;
  }
  return findProjectSessionForContact(contact);
};

export const useTeamMembersContactResources = ({
  project,
  store,
}: UseTeamMembersContactResourcesOptions) => {
  const {
    apiClient,
    currentSession,
    sessions,
    contacts,
    agents,
    loadContacts,
    sessionChatState,
    sendMessage,
    loadMoreMessages,
    createSession,
    selectSession,
    aiModelConfigs,
    selectedModelId,
    messages,
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

  const findProjectSessionForContact = useCallback((contact: ContactItem): Session | null => {
    return findLatestMatchedSession(sessions || [], contact, normalizedProjectId);
  }, [normalizedProjectId, sessions]);

  const { ensureContactSession: ensureContactSessionFromResolver } = useContactSessionResolver({
    sessions: sessions || [],
    currentSession,
    createSession,
    apiClient,
    defaultProjectId: normalizedProjectId,
  });

  const projectContacts = useMemo<ProjectContactRow[]>(() => {
    const rows = projectMembers.map((member) => {
      const contact: ContactItem = {
        id: member.contactId,
        agentId: member.agentId,
        name: member.name,
      };
      const session = resolveProjectContactSession({
        currentSession,
        contact,
        normalizedProjectId,
        findProjectSessionForContact,
      });
      return {
        contact,
        session,
        updatedAt: session ? resolveSessionTimestamp(session) : member.updatedAt,
      };
    });
    rows.sort((a, b) => b.updatedAt - a.updatedAt);
    return rows;
  }, [currentSession, findProjectSessionForContact, normalizedProjectId, projectMembers]);

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

  const ensureContactSession = useCallback(async (contact: ContactItem): Promise<string | null> => {
    return ensureContactSessionFromResolver(contact, {
      projectId: normalizedProjectId,
      title: contact.name || '联系人',
      selectedModelId: selectedModelId ?? null,
      projectRoot: project.rootPath || null,
      mcpEnabled: true,
      enabledMcpIds: [],
      createSessionOptions: { keepActivePanel: true, activateSession: false },
    });
  }, [
    ensureContactSessionFromResolver,
    normalizedProjectId,
    project.rootPath,
    selectedModelId,
  ]);

  const conversation = useTeamMemberConversation({
    projectId: normalizedProjectId,
    projectRootPath: project.rootPath || null,
    currentSession,
    projectContacts,
    normalizedContacts,
    selectedModelId,
    aiModelConfigs,
    sessionChatState,
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
    messages,
    openTurnProcessViewer: store.openTurnProcessViewer,
    loadMoreMessages,
  });

  const selectedContactAgent = useMemo<AgentConfig | null>(() => {
    const agentId = typeof conversation.selectedContact?.agentId === 'string'
      ? conversation.selectedContact.agentId.trim()
      : '';
    if (!agentId) {
      return null;
    }
    return (agents || []).find((agent: AgentConfig) => agent?.id === agentId) || null;
  }, [agents, conversation.selectedContact?.agentId]);

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
    conversation: {
      ...conversation,
      selectedContactAgent,
    },
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
