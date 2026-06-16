import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { isSessionMatchedContactAndProject } from '../../../features/contactSession/sessionResolver';
import type { Session } from '../../../types';
import type { SendMessageRuntimeOptions, SessionSelectOptions } from '../../../lib/store/types';
import type { ContactItem, ProjectContactRow } from './types';

interface UseTeamMemberConversationParams {
  projectId: string;
  projectRootPath: string | null;
  currentSession: Session | null;
  projectContacts: ProjectContactRow[];
  normalizedContacts: ContactItem[];
  summaryPaneSessionId: string | null;
  setSummaryPaneSessionId: (sessionId: string | null) => void;
  setSummaryError: (error: string | null) => void;
  resetSummaryState: () => void;
  openSummaryForSession: (sessionId: string) => Promise<void>;
  deleteSummary: (sessionId: string, summaryId: string) => Promise<void>;
  clearSummaries: (
    sessionId: string,
    options: { confirmMessage?: string },
  ) => Promise<void>;
  cancelPendingSessionSummariesLoad: () => void;
  ensureContactSession: (contact: ContactItem) => Promise<string | null>;
  selectSession: (sessionId: string, options?: SessionSelectOptions) => Promise<void>;
  sendMessage: (
    content: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => Promise<void>;
  loadMoreMessages: (sessionId: string) => Promise<void>;
}

export const useTeamMemberConversation = ({
  projectId,
  projectRootPath,
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
}: UseTeamMemberConversationParams) => {
  const { t } = useI18n();
  const [selectedContactId, setSelectedContactId] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [switchingContactId, setSwitchingContactId] = useState<string | null>(null);
  const [openingSummaryContactId, setOpeningSummaryContactId] = useState<string | null>(null);
  const latestContactSwitchSeqRef = useRef(0);
  const latestSummaryOpenSeqRef = useRef(0);

  const currentSessionMatchedContactRow = useMemo(() => {
    if (!currentSession) {
      return null;
    }
    return projectContacts.find((item) => (
      isSessionMatchedContactAndProject(currentSession, item.contact, projectId)
    )) || null;
  }, [currentSession, projectContacts, projectId]);

  const selectedContactRow = useMemo(() => {
    if (!selectedContactId) {
      return null;
    }
    return projectContacts.find((item) => item.contact.id === selectedContactId) || null;
  }, [projectContacts, selectedContactId]);

  const selectedContact = useMemo(() => {
    if (selectedContactRow?.contact) {
      return selectedContactRow.contact;
    }
    if (selectedContactId) {
      return normalizedContacts.find((item) => item.id === selectedContactId) || null;
    }
    return currentSessionMatchedContactRow?.contact || null;
  }, [currentSessionMatchedContactRow?.contact, normalizedContacts, selectedContactId, selectedContactRow?.contact]);

  const selectedProjectSession = useMemo(() => {
    const normalizedSelectedSessionId = typeof selectedSessionId === 'string'
      ? selectedSessionId.trim()
      : '';
    if (normalizedSelectedSessionId) {
      if (currentSession?.id === normalizedSelectedSessionId) {
        return currentSession;
      }
      const bySessionId = projectContacts.find((item) => item.session?.id === normalizedSelectedSessionId);
      if (bySessionId?.session) {
        return bySessionId.session;
      }
    }
    if (selectedContactRow?.session) {
      return selectedContactRow.session;
    }
    if (
      currentSession
      && selectedContact
      && isSessionMatchedContactAndProject(currentSession, selectedContact, projectId)
    ) {
      return currentSession;
    }
    return null;
  }, [currentSession, projectContacts, projectId, selectedContact, selectedContactRow?.session, selectedSessionId]);

  const isSelectedSessionActive = Boolean(
    selectedProjectSession?.id
    && currentSession?.id
    && selectedProjectSession.id === currentSession.id,
  );

  const sessionSummaryPaneVisible = Boolean(
    selectedProjectSession?.id
    && summaryPaneSessionId
    && selectedProjectSession.id === summaryPaneSessionId,
  );

  const handleSelectContact = useCallback(async (contactId: string) => {
    const contact = projectContacts.find((item) => item.contact.id === contactId)?.contact
      || normalizedContacts.find((item) => item.id === contactId)
      || null;
    if (!contact) {
      return;
    }
    const requestSeq = latestContactSwitchSeqRef.current + 1;
    latestContactSwitchSeqRef.current = requestSeq;
    setSelectedContactId(contactId);
    setSwitchingContactId(contactId);
    try {
      const sessionId = await ensureContactSession(contact);
      if (latestContactSwitchSeqRef.current !== requestSeq) {
        return;
      }
      setSelectedSessionId(sessionId);
      if (sessionId && currentSession?.id !== sessionId) {
        await selectSession(sessionId, {
          keepActivePanel: true,
          skipBackgroundSync: true,
        });
      }
      if (latestContactSwitchSeqRef.current !== requestSeq) {
        return;
      }
      if (summaryPaneSessionId && sessionId && sessionId !== summaryPaneSessionId) {
        cancelPendingSessionSummariesLoad();
        setSummaryPaneSessionId(null);
        resetSummaryState();
      }
    } finally {
      setSwitchingContactId((prev) => (prev === contactId ? null : prev));
    }
  }, [
    cancelPendingSessionSummariesLoad,
    ensureContactSession,
    normalizedContacts,
    projectContacts,
    resetSummaryState,
    currentSession?.id,
    selectSession,
    setSummaryPaneSessionId,
    summaryPaneSessionId,
  ]);

  useEffect(() => {
    if (projectContacts.length === 0) {
      setSelectedContactId(null);
      setSelectedSessionId(null);
      return;
    }
    if (currentSessionMatchedContactRow && currentSession?.id) {
      setSelectedContactId(currentSessionMatchedContactRow.contact.id);
      setSelectedSessionId(currentSession.id);
      return;
    }
    if (selectedContactId && projectContacts.some((item) => item.contact.id === selectedContactId)) {
      return;
    }
    const firstContact = projectContacts[0];
    setSelectedContactId(firstContact.contact.id);
    setSelectedSessionId(firstContact.session?.id || null);
  }, [currentSession?.id, currentSessionMatchedContactRow, projectContacts, selectedContactId]);

  useEffect(() => {
    if (!selectedContactId || switchingContactId) {
      return;
    }
    if (selectedProjectSession && isSelectedSessionActive) {
      return;
    }
    void handleSelectContact(selectedContactId);
  }, [
    handleSelectContact,
    isSelectedSessionActive,
    selectedContactId,
    selectedProjectSession,
    switchingContactId,
  ]);

  useEffect(() => {
    if (!selectedContactId) {
      setSelectedSessionId(null);
      return;
    }
    if (
      currentSession
      && selectedContact
      && isSessionMatchedContactAndProject(currentSession, selectedContact, projectId)
    ) {
      if (selectedSessionId !== currentSession.id) {
        setSelectedSessionId(currentSession.id);
      }
      return;
    }
    if (switchingContactId === selectedContactId) {
      return;
    }
    const latestRowSessionId = selectedContactRow?.session?.id || null;
    if (selectedSessionId !== latestRowSessionId) {
      setSelectedSessionId(latestRowSessionId);
    }
  }, [
    currentSession,
    projectId,
    selectedContact,
    selectedContactId,
    selectedContactRow?.session?.id,
    selectedSessionId,
    switchingContactId,
  ]);

  const handleLoadMore = useCallback(async () => {
    if (selectedProjectSession?.id) {
      await loadMoreMessages(selectedProjectSession.id);
    }
  }, [loadMoreMessages, selectedProjectSession?.id]);

  const handleSendMessage = useCallback(async (
    content: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => {
    if (!selectedContact) {
      return;
    }
    try {
      const sessionId = await ensureContactSession(selectedContact);
      if (!sessionId) {
        return;
      }
      setSelectedSessionId(sessionId);
      if (currentSession?.id !== sessionId) {
        await selectSession(sessionId, {
          keepActivePanel: true,
          skipBackgroundSync: true,
        });
      }
      await sendMessage(content, attachments, {
        remoteConnectionId: runtimeOptions?.remoteConnectionId,
        modelConfigId: runtimeOptions?.modelConfigId,
        modelName: runtimeOptions?.modelName,
        thinkingLevel: runtimeOptions?.thinkingLevel,
        contactAgentId: selectedContact.agentId,
        contactId: selectedContact.id,
        projectId,
        projectRoot: projectRootPath || null,
        workspaceRoot: null,
      });
    } catch (error) {
      console.error('Failed to send message in team pane:', error);
    }
  }, [
    ensureContactSession,
    currentSession?.id,
    projectId,
    projectRootPath,
    selectedContact,
    selectSession,
    sendMessage,
  ]);

  const handleOpenSummary = useCallback(async (contact: ContactItem) => {
    const requestSeq = latestSummaryOpenSeqRef.current + 1;
    latestSummaryOpenSeqRef.current = requestSeq;
    setOpeningSummaryContactId(contact.id);
    setSelectedContactId(contact.id);
    setSwitchingContactId(contact.id);
    setSummaryError(null);
    try {
      const sessionId = await ensureContactSession(contact);
      if (latestSummaryOpenSeqRef.current !== requestSeq) {
        return;
      }
      if (!sessionId) {
        return;
      }
      setSelectedSessionId(sessionId);
      if (currentSession?.id !== sessionId) {
        await selectSession(sessionId, {
          keepActivePanel: true,
          skipBackgroundSync: true,
        });
      }
      if (latestSummaryOpenSeqRef.current !== requestSeq) {
        return;
      }
      await openSummaryForSession(sessionId);
    } finally {
      setSwitchingContactId((prev) => (prev === contact.id ? null : prev));
      setOpeningSummaryContactId((prev) => (prev === contact.id ? null : prev));
    }
  }, [ensureContactSession, currentSession?.id, openSummaryForSession, selectSession, setSummaryError]);

  const handleDeleteSummary = useCallback(async (summaryId: string) => {
    if (!selectedProjectSession?.id || !summaryId) {
      return;
    }
    void deleteSummary(selectedProjectSession.id, summaryId);
  }, [deleteSummary, selectedProjectSession?.id]);

  const handleClearSummaries = useCallback(async () => {
    if (!selectedProjectSession?.id) {
      return;
    }
    await clearSummaries(selectedProjectSession.id, {
      confirmMessage: t('teamMembers.summaryClearMessage'),
    });
  }, [clearSummaries, selectedProjectSession?.id, t]);

  return {
    selectedContactId,
    selectedSessionId,
    switchingContactId,
    openingSummaryContactId,
    selectedContact,
    selectedProjectSession,
    isSelectedSessionActive,
    sessionSummaryPaneVisible,
    setSelectedContactId,
    handleSelectContact,
    handleLoadMore,
    handleSendMessage,
    handleOpenSummary,
    handleDeleteSummary,
    handleClearSummaries,
  };
};
