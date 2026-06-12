import { useCallback, useState, type Dispatch, type SetStateAction } from 'react';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type { AgentConfig, Session } from '../../types';
import { mergeSessionRuntimeIntoMetadata } from '../../lib/store/helpers/sessionRuntime';
import { CONTACT_CHAT_PROJECT_ID } from './useContactSessionListState';
import { translateSessionListMessage } from './helpers';
import type { ContactItem } from './types';
import type { SessionSelectOptions } from '../../lib/store/types';

interface UseContactSessionCreatorOptions {
  t?: TranslateFn;
  agents: AgentConfig[];
  currentSessionId: string | null;
  loadContacts: () => Promise<unknown>;
  createContact: (agentId: string, name?: string) => Promise<ContactItem>;
  ensureSessionForContact: (contact: ContactItem) => Promise<string | null>;
  updateSession: (sessionId: string, patch: Partial<Session>) => Promise<unknown>;
  selectSession: (sessionId: string, options?: SessionSelectOptions) => Promise<unknown>;
}

interface UseContactSessionCreatorResult {
  createContactModalOpen: boolean;
  selectedContactAgentId: string | null;
  contactError: string | null;
  setSelectedContactAgentId: Dispatch<SetStateAction<string | null>>;
  setContactError: Dispatch<SetStateAction<string | null>>;
  openCreateSessionModal: () => Promise<void>;
  closeCreateSessionModal: () => void;
  handleCreateContactSession: () => Promise<void>;
}

export const useContactSessionCreator = ({
  t,
  agents,
  currentSessionId,
  loadContacts,
  createContact,
  ensureSessionForContact,
  updateSession,
  selectSession,
}: UseContactSessionCreatorOptions): UseContactSessionCreatorResult => {
  const [createContactModalOpen, setCreateContactModalOpen] = useState(false);
  const [selectedContactAgentId, setSelectedContactAgentId] = useState<string | null>(null);
  const [contactError, setContactError] = useState<string | null>(null);

  const openCreateSessionModal = useCallback(async () => {
    setContactError(null);
    setSelectedContactAgentId(null);
    try {
      await loadContacts();
    } catch (error) {
      setContactError(error instanceof Error ? error.message : translateSessionListMessage(t, 'contactModal.error.loadFailed'));
    }
    setCreateContactModalOpen(true);
  }, [loadContacts, t]);

  const closeCreateSessionModal = useCallback(() => {
    setCreateContactModalOpen(false);
    setSelectedContactAgentId(null);
    setContactError(null);
  }, []);

  const handleCreateContactSession = useCallback(async () => {
    const agentId = selectedContactAgentId?.trim();
    if (!agentId) {
      setContactError(translateSessionListMessage(t, 'contactModal.error.selectFirst'));
      return;
    }
    const selectedAgent = (agents || []).find((agent) => agent.id === agentId);
    if (!selectedAgent) {
      setContactError(translateSessionListMessage(t, 'contactModal.error.unavailable'));
      return;
    }
    try {
      const createdContact = await createContact(
        selectedAgent.id,
        selectedAgent.name || undefined,
      );
      const matchedContact: ContactItem = {
        id: createdContact.id,
        agentId: createdContact.agentId,
        name: createdContact.name,
        status: createdContact.status,
        createdAt: createdContact.createdAt,
        updatedAt: createdContact.updatedAt,
      };
      const ensuredSessionId = await ensureSessionForContact(matchedContact);
      if (ensuredSessionId) {
        const metadata = mergeSessionRuntimeIntoMetadata(null, {
          contactAgentId: selectedAgent.id,
          contactId: createdContact.id || null,
          selectedModelId: null,
          projectId: CONTACT_CHAT_PROJECT_ID,
          projectRoot: null,
        });
        await updateSession(ensuredSessionId, { metadata } as Partial<Session>);
        if (currentSessionId !== ensuredSessionId) {
          await selectSession(ensuredSessionId, {
            initialPageSize: 1,
            skipBackgroundSync: true,
          });
        }
      }

      await loadContacts();
      closeCreateSessionModal();
    } catch (error) {
      console.error('Failed to create session:', error);
      setContactError(error instanceof Error ? error.message : translateSessionListMessage(t, 'contactModal.error.addFailed'));
    }
  }, [
    agents,
    closeCreateSessionModal,
    createContact,
    currentSessionId,
    ensureSessionForContact,
    loadContacts,
    selectedContactAgentId,
    selectSession,
    t,
    updateSession,
  ]);

  return {
    createContactModalOpen,
    selectedContactAgentId,
    contactError,
    setSelectedContactAgentId,
    setContactError,
    openCreateSessionModal,
    closeCreateSessionModal,
    handleCreateContactSession,
  };
};
