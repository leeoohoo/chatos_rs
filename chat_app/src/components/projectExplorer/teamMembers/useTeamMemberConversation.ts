import { useCallback, useEffect, useMemo, useState } from 'react';

import type { Session } from '../../../types';
import type { ContactItem, ProjectContactRow } from './types';

interface UseTeamMemberConversationParams {
  projectId: string;
  projectRootPath: string | null;
  currentSession: Session | null;
  projectContacts: ProjectContactRow[];
  normalizedContacts: ContactItem[];
  selectedModelId: string | null;
  aiModelConfigs: any[];
  sessionChatState: Record<string, any>;
  summaryPaneSessionId: string | null;
  setSummaryPaneSessionId: (sessionId: string | null) => void;
  setSummaryError: (error: string | null) => void;
  resetSummaryState: () => void;
  openSummaryForSession: (sessionId: string) => Promise<any>;
  deleteSummary: (sessionId: string, summaryId: string) => Promise<any>;
  clearSummaries: (sessionId: string, options: { confirmMessage?: string }) => Promise<any>;
  loadSessionSummaries: (sessionId: string, options?: { silent?: boolean }) => Promise<any>;
  ensureContactSession: (contact: ContactItem) => Promise<string | null>;
  sendMessage: (content: string, attachments?: File[], runtimeOptions?: any) => Promise<any>;
  toggleTurnProcess: (userMessageId: string) => Promise<any>;
  loadMoreMessages: (sessionId: string) => void | Promise<any>;
}

export const useTeamMemberConversation = ({
  projectId,
  projectRootPath,
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
  loadSessionSummaries,
  ensureContactSession,
  sendMessage,
  toggleTurnProcess,
  loadMoreMessages,
}: UseTeamMemberConversationParams) => {
  const [selectedContactId, setSelectedContactId] = useState<string | null>(null);
  const [switchingContactId, setSwitchingContactId] = useState<string | null>(null);
  const [openingSummaryContactId, setOpeningSummaryContactId] = useState<string | null>(null);

  const selectedContact = useMemo(() => {
    if (!selectedContactId) {
      return null;
    }
    const matched = projectContacts.find((item) => item.contact.id === selectedContactId);
    return matched?.contact || null;
  }, [projectContacts, selectedContactId]);

  const selectedProjectSession = useMemo(() => {
    if (!selectedContactId) {
      return null;
    }
    const matched = projectContacts.find((item) => item.contact.id === selectedContactId);
    return matched?.session || null;
  }, [projectContacts, selectedContactId]);

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

  const selectedSessionChatState = useMemo(() => {
    if (!selectedProjectSession?.id) {
      return undefined;
    }
    return sessionChatState[selectedProjectSession.id];
  }, [selectedProjectSession?.id, sessionChatState]);

  const chatIsLoading = selectedSessionChatState?.isLoading ?? false;
  const chatIsStreaming = selectedSessionChatState?.isStreaming ?? false;
  const chatIsStopping = selectedSessionChatState?.isStopping ?? false;

  const supportsReasoning = useMemo(() => {
    if (!selectedModelId) {
      return false;
    }
    const matched = (aiModelConfigs || []).find((item) => item.id === selectedModelId);
    return matched?.supports_reasoning === true;
  }, [aiModelConfigs, selectedModelId]);

  const handleSelectContact = useCallback(async (contactId: string) => {
    const contact = projectContacts.find((item) => item.contact.id === contactId)?.contact
      || normalizedContacts.find((item) => item.id === contactId)
      || null;
    if (!contact) {
      return;
    }
    setSelectedContactId(contactId);
    setSwitchingContactId(contactId);
    try {
      const sessionId = await ensureContactSession(contact);
      if (summaryPaneSessionId && sessionId && sessionId !== summaryPaneSessionId) {
        setSummaryPaneSessionId(null);
        resetSummaryState();
      }
    } finally {
      setSwitchingContactId((prev) => (prev === contactId ? null : prev));
    }
  }, [
    ensureContactSession,
    normalizedContacts,
    projectContacts,
    resetSummaryState,
    setSummaryPaneSessionId,
    summaryPaneSessionId,
  ]);

  useEffect(() => {
    if (projectContacts.length === 0) {
      setSelectedContactId(null);
      return;
    }
    if (selectedContactId && projectContacts.some((item) => item.contact.id === selectedContactId)) {
      return;
    }
    const firstId = projectContacts[0].contact.id;
    void handleSelectContact(firstId);
  }, [handleSelectContact, projectContacts, selectedContactId]);

  const handleLoadMore = useCallback(() => {
    if (selectedProjectSession?.id) {
      loadMoreMessages(selectedProjectSession.id);
    }
  }, [loadMoreMessages, selectedProjectSession?.id]);

  const handleToggleTurnProcess = useCallback((userMessageId: string) => {
    if (!userMessageId) {
      return;
    }
    void toggleTurnProcess(userMessageId).catch((error) => {
      console.error('Failed to toggle turn process messages in team pane:', error);
    });
  }, [toggleTurnProcess]);

  const handleSendMessage = useCallback(async (
    content: string,
    attachments?: File[],
    runtimeOptions?: {
      mcpEnabled?: boolean;
      projectId?: string | null;
      projectRoot?: string | null;
      workspaceRoot?: string | null;
      enabledMcpIds?: string[];
    },
  ) => {
    if (!selectedContact) {
      return;
    }
    try {
      const sessionId = await ensureContactSession(selectedContact);
      if (!sessionId) {
        return;
      }
      await sendMessage(content, attachments, {
        mcpEnabled: runtimeOptions?.mcpEnabled,
        enabledMcpIds: runtimeOptions?.enabledMcpIds,
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
    projectId,
    projectRootPath,
    selectedContact,
    sendMessage,
  ]);

  const handleOpenSummary = useCallback(async (contact: ContactItem) => {
    setOpeningSummaryContactId(contact.id);
    setSelectedContactId(contact.id);
    setSwitchingContactId(contact.id);
    setSummaryError(null);
    try {
      const sessionId = await ensureContactSession(contact);
      if (!sessionId) {
        return;
      }
      await openSummaryForSession(sessionId);
    } finally {
      setSwitchingContactId((prev) => (prev === contact.id ? null : prev));
      setOpeningSummaryContactId((prev) => (prev === contact.id ? null : prev));
    }
  }, [ensureContactSession, openSummaryForSession, setSummaryError]);

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
      confirmMessage: '确定清空当前会话的所有总结吗？',
    });
  }, [clearSummaries, selectedProjectSession?.id]);

  useEffect(() => {
    if (!sessionSummaryPaneVisible || !selectedProjectSession?.id) {
      return;
    }
    void loadSessionSummaries(selectedProjectSession.id, { silent: true });
  }, [loadSessionSummaries, selectedProjectSession?.id, sessionSummaryPaneVisible]);

  return {
    selectedContactId,
    switchingContactId,
    openingSummaryContactId,
    selectedContact,
    selectedProjectSession,
    isSelectedSessionActive,
    sessionSummaryPaneVisible,
    chatIsLoading,
    chatIsStreaming,
    chatIsStopping,
    supportsReasoning,
    setSelectedContactId,
    handleSelectContact,
    handleLoadMore,
    handleToggleTurnProcess,
    handleSendMessage,
    handleOpenSummary,
    handleDeleteSummary,
    handleClearSummaries,
  };
};
