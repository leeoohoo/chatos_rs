import { useCallback, useMemo } from 'react';

import type { Session } from '../../types';
import { mergeSessionRuntimeIntoMetadata } from '../../lib/store/helpers/sessionRuntime';
import {
  resolveContactAgentIdFromSession,
  resolveContactIdFromSession,
} from '../../features/contactSession/sessionResolver';
import { useContactSessionResolver } from '../../features/contactSession/useContactSessionResolver';

export const CONTACT_CHAT_PROJECT_ID = '0';
const CONTACT_PLACEHOLDER_PREFIX = 'contact-placeholder:';

export interface SessionListContact {
  id: string;
  agentId: string;
  name: string;
  createdAt: Date;
  updatedAt: Date;
}

type CreateSessionFn = (
  payload: {
    title: string;
    contactAgentId: string;
    contactId: string;
    selectedModelId: string | null;
    projectId: string;
    projectRoot: string | null;
    mcpEnabled: boolean;
    enabledMcpIds: string[];
  },
  options?: { keepActivePanel?: boolean },
) => Promise<string | undefined | null>;

interface SessionResolverApiClient {
  getSessions: (
    userId?: string,
    projectId?: string,
    paging?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
  getConversationMessages: (
    sessionId: string,
    params?: { limit?: number; offset?: number; compact?: boolean },
  ) => Promise<unknown[]>;
}

interface UseContactSessionListStateOptions {
  contacts: SessionListContact[];
  sessions: Session[];
  currentSession: Session | null | undefined;
  activePanel: string;
  activeSummarySessionId?: string | null;
  createSession: CreateSessionFn;
  apiClient: SessionResolverApiClient;
}

interface UseContactSessionListStateResult {
  ensureSessionForContact: (contact: SessionListContact) => Promise<string | null>;
  displaySessionRuntimeIdMap: Record<string, string>;
  displaySessions: Session[];
  currentDisplaySessionId: string | null;
  activeSummaryDisplaySessionId: string | null;
  clearCachedSessionIdsForContact: (contactId: string, projectId?: string | null) => string[];
}

export const useContactSessionListState = ({
  contacts,
  sessions,
  currentSession,
  activePanel,
  activeSummarySessionId,
  createSession,
  apiClient,
}: UseContactSessionListStateOptions): UseContactSessionListStateResult => {
  const {
    ensureContactSession,
    buildDisplayRuntimeSessionIdMap,
    clearCachedSessionIdsForContact,
  } = useContactSessionResolver({
    sessions: sessions || [],
    currentSession,
    createSession,
    apiClient,
    defaultProjectId: CONTACT_CHAT_PROJECT_ID,
  });

  const ensureSessionForContact = useCallback((contact: SessionListContact): Promise<string | null> => {
    return ensureContactSession(contact, {
      projectId: CONTACT_CHAT_PROJECT_ID,
      title: contact.name || '联系人',
      selectedModelId: null,
      projectRoot: null,
      mcpEnabled: true,
      enabledMcpIds: [],
    });
  }, [ensureContactSession]);

  const displaySessionRuntimeIdMap = useMemo<Record<string, string>>(() => {
    return buildDisplayRuntimeSessionIdMap(contacts || [], {
      projectId: CONTACT_CHAT_PROJECT_ID,
      keyPrefix: CONTACT_PLACEHOLDER_PREFIX,
    });
  }, [buildDisplayRuntimeSessionIdMap, contacts]);

  const displaySessions = useMemo<Session[]>(() => {
    return contacts.map((contact) => {
      const placeholderId = `${CONTACT_PLACEHOLDER_PREFIX}${contact.id}`;
      const runtimeSessionId = displaySessionRuntimeIdMap[placeholderId] || null;
      const runtimeSession = runtimeSessionId
        ? sessions.find((item: Session) => item.id === runtimeSessionId)
        : null;
      return {
        id: placeholderId,
        title: contact.name,
        createdAt: contact.createdAt,
        updatedAt: runtimeSession?.updatedAt || contact.updatedAt,
        messageCount: 0,
        tokenUsage: 0,
        pinned: false,
        archived: false,
        status: 'active',
        metadata: mergeSessionRuntimeIntoMetadata(null, {
          contactAgentId: contact.agentId,
          contactId: contact.id,
          selectedModelId: null,
          projectId: CONTACT_CHAT_PROJECT_ID,
          projectRoot: null,
          mcpEnabled: true,
          enabledMcpIds: [],
        }),
      } as Session;
    });
  }, [contacts, displaySessionRuntimeIdMap, sessions]);

  const currentDisplaySessionId = useMemo(() => {
    if (activePanel !== 'chat') {
      return null;
    }
    const currentContactId = resolveContactIdFromSession(currentSession);
    if (currentContactId) {
      return `${CONTACT_PLACEHOLDER_PREFIX}${currentContactId}`;
    }

    const currentContactAgentId = resolveContactAgentIdFromSession(currentSession);
    if (!currentContactAgentId) {
      return null;
    }
    const matched = contacts.find((item) => item.agentId === currentContactAgentId);
    if (!matched) {
      return null;
    }
    return `${CONTACT_PLACEHOLDER_PREFIX}${matched.id}`;
  }, [activePanel, contacts, currentSession]);

  const activeSummaryDisplaySessionId = useMemo(() => {
    if (!activeSummarySessionId || !currentSession?.id) {
      return null;
    }
    if (activeSummarySessionId !== currentSession.id) {
      return null;
    }
    return currentDisplaySessionId;
  }, [activeSummarySessionId, currentDisplaySessionId, currentSession?.id]);

  return {
    ensureSessionForContact,
    displaySessionRuntimeIdMap,
    displaySessions,
    currentDisplaySessionId,
    activeSummaryDisplaySessionId,
    clearCachedSessionIdsForContact,
  };
};
