import { useCallback, useState, type Dispatch, type SetStateAction } from 'react';
import type { Session } from '../../types';
import { mergeSessionRuntimeIntoMetadata } from '../../lib/store/helpers/sessionRuntime';
import { CONTACT_CHAT_PROJECT_ID } from './useContactSessionListState';
import type { ContactItem } from './types';

interface UseContactScopeCreatorOptions {
  agents: any[];
  currentSessionId: string | null;
  loadContacts: () => Promise<unknown>;
  createContact: (agentId: string, name?: string) => Promise<any>;
  ensureBackingSessionForContactScope: (contact: ContactItem) => Promise<string | null>;
  updateSession: (sessionId: string, patch: Partial<Session>) => Promise<unknown>;
  selectSession: (sessionId: string) => Promise<unknown>;
}

interface UseContactScopeCreatorResult {
  createContactModalOpen: boolean;
  selectedContactAgentId: string | null;
  contactError: string | null;
  setSelectedContactAgentId: Dispatch<SetStateAction<string | null>>;
  setContactError: Dispatch<SetStateAction<string | null>>;
  openCreateSessionModal: () => Promise<void>;
  closeCreateSessionModal: () => void;
  handleCreateContactSession: () => Promise<void>;
}

export const useContactScopeCreator = ({
  agents,
  currentSessionId,
  loadContacts,
  createContact,
  ensureBackingSessionForContactScope,
  updateSession,
  selectSession,
}: UseContactScopeCreatorOptions): UseContactScopeCreatorResult => {
  const [createContactModalOpen, setCreateContactModalOpen] = useState(false);
  const [selectedContactAgentId, setSelectedContactAgentId] = useState<string | null>(null);
  const [contactError, setContactError] = useState<string | null>(null);

  const openCreateSessionModal = useCallback(async () => {
    setContactError(null);
    setSelectedContactAgentId(null);
    try {
      await loadContacts();
    } catch (error) {
      setContactError(error instanceof Error ? error.message : '加载联系人失败');
    }
    setCreateContactModalOpen(true);
  }, [loadContacts]);

  const closeCreateSessionModal = useCallback(() => {
    setCreateContactModalOpen(false);
    setSelectedContactAgentId(null);
    setContactError(null);
  }, []);

  const handleCreateContactSession = useCallback(async () => {
    const agentId = selectedContactAgentId?.trim();
    if (!agentId) {
      setContactError('请先选择一个联系人');
      return;
    }
    const selectedAgent = (agents || []).find((agent: any) => agent.id === agentId);
    if (!selectedAgent) {
      setContactError('联系人不存在或不可用');
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
        authorizedBuiltinMcpIds: createdContact.authorizedBuiltinMcpIds,
        status: createdContact.status,
        createdAt: createdContact.createdAt,
        updatedAt: createdContact.updatedAt,
      };
      const ensuredSessionId = await ensureBackingSessionForContactScope(matchedContact);
      if (ensuredSessionId) {
        const metadata = mergeSessionRuntimeIntoMetadata(null, {
          contactAgentId: selectedAgent.id,
          contactId: createdContact.id || null,
          selectedModelId: null,
          projectId: CONTACT_CHAT_PROJECT_ID,
          projectRoot: null,
          mcpEnabled: true,
          enabledMcpIds: [],
        });
        await updateSession(ensuredSessionId, { metadata } as Partial<Session>);
        if (currentSessionId !== ensuredSessionId) {
          await selectSession(ensuredSessionId);
        }
      }

      await loadContacts();
      closeCreateSessionModal();
    } catch (error) {
      console.error('Failed to create session:', error);
      setContactError(error instanceof Error ? error.message : '添加联系人失败');
    }
  }, [
    agents,
    closeCreateSessionModal,
    createContact,
    currentSessionId,
    ensureBackingSessionForContactScope,
    loadContacts,
    selectedContactAgentId,
    selectSession,
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

export const useContactSessionCreator = useContactScopeCreator;
