import { useCallback, useMemo } from 'react';

import type { Session } from '../../types';
import { mergeSessionRuntimeIntoMetadata } from '../../lib/store/helpers/sessionRuntime';
import type { ImConversationResponse } from '../../lib/api/client/types';
import {
  resolveContactAgentIdFromScopeRecord,
  resolveContactIdFromScopeRecord,
} from '../../features/contactSession/sessionResolver';
import { useContactScopeResolver } from '../../features/contactSession/useContactSessionResolver';

export const CONTACT_CHAT_PROJECT_ID = '0';

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

interface ScopeResolverApiClient {
  getSessions: (
    userId?: string,
    projectId?: string,
    paging?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
  getSessionMessages: (
    sessionId: string,
    params?: { limit?: number; offset?: number; compact?: boolean },
  ) => Promise<unknown[]>;
}

interface UseContactSessionListStateOptions {
  contacts: SessionListContact[];
  sessions: Session[];
  imConversations?: ImConversationResponse[];
  currentSession: Session | null | undefined;
  activePanel: string;
  activeSummarySessionId?: string | null;
  createSession: CreateSessionFn;
  apiClient: ScopeResolverApiClient;
}

interface UseContactSessionListStateResult {
  ensureBackingSessionForContactScope: (contact: SessionListContact) => Promise<string | null>;
  displayBackingSessionIdMap: Record<string, string>;
  displayScopeSessions: Session[];
  currentDisplayScopeSessionId: string | null;
  activeSummaryDisplayScopeSessionId: string | null;
  clearCachedBackingSessionIdsForContact: (contactId: string, projectId?: string | null) => string[];
  ensureSessionForContact: (contact: SessionListContact) => Promise<string | null>;
  displaySessionRuntimeIdMap: Record<string, string>;
  displaySessions: Session[];
  currentDisplaySessionId: string | null;
  activeSummaryDisplaySessionId: string | null;
  clearCachedSessionIdsForContact: (contactId: string, projectId?: string | null) => string[];
}

export const useContactScopeListState = ({
  contacts,
  sessions,
  imConversations = [],
  currentSession,
  activePanel,
  activeSummarySessionId,
  createSession,
  apiClient,
}: UseContactSessionListStateOptions): UseContactSessionListStateResult => {
  const {
    ensureBackingSessionForContactScope,
    buildDisplayBackingSessionIdMap,
    clearCachedBackingSessionIdsForContact,
  } = useContactScopeResolver({
    sessions: sessions || [],
    currentSession,
    createSession,
    apiClient,
    defaultProjectId: CONTACT_CHAT_PROJECT_ID,
  });

  const ensureBackingSession = useCallback((contact: SessionListContact): Promise<string | null> => {
    return ensureBackingSessionForContactScope(contact, {
      projectId: CONTACT_CHAT_PROJECT_ID,
      title: contact.name || '联系人',
      selectedModelId: null,
      projectRoot: null,
      mcpEnabled: true,
      enabledMcpIds: [],
    });
  }, [ensureBackingSessionForContactScope]);

  const displayBackingSessionIdMap = useMemo<Record<string, string>>(() => {
    return buildDisplayBackingSessionIdMap(contacts || [], {
      projectId: CONTACT_CHAT_PROJECT_ID,
    });
  }, [buildDisplayBackingSessionIdMap, contacts]);

  const displayScopeSessions = useMemo<Session[]>(() => {
    const conversationByContactId = new Map<string, ImConversationResponse>();
    for (const conversation of imConversations || []) {
      const contactId = typeof conversation?.contact_id === 'string'
        ? conversation.contact_id.trim()
        : '';
      const projectId = typeof conversation?.project_id === 'string'
        ? conversation.project_id.trim()
        : '';
      if (!contactId || (projectId || CONTACT_CHAT_PROJECT_ID) !== CONTACT_CHAT_PROJECT_ID) {
        continue;
      }
      const previous = conversationByContactId.get(contactId);
      const nextTime = new Date(
        conversation?.last_message_at || conversation?.updated_at || conversation?.created_at || 0,
      ).getTime();
      const prevTime = new Date(
        previous?.last_message_at || previous?.updated_at || previous?.created_at || 0,
      ).getTime();
      if (!previous || nextTime >= prevTime) {
        conversationByContactId.set(contactId, conversation);
      }
    }

    return contacts.map((contact) => {
      const runtimeSessionId = displayBackingSessionIdMap[contact.id] || null;
      const runtimeSession = runtimeSessionId
        ? sessions.find((item: Session) => item.id === runtimeSessionId)
        : null;
      const matchedConversation = conversationByContactId.get(contact.id) || null;
      const nextMetadata = mergeSessionRuntimeIntoMetadata(runtimeSession?.metadata || null, {
        contactAgentId: contact.agentId,
        contactId: contact.id,
        selectedModelId: null,
        projectId: CONTACT_CHAT_PROJECT_ID,
        projectRoot: null,
        mcpEnabled: true,
        enabledMcpIds: [],
      }) as Record<string, unknown>;
      const conversationId = typeof matchedConversation?.id === 'string'
        ? matchedConversation.id.trim()
        : '';
      if (conversationId) {
        nextMetadata.im = {
          conversation_id: conversationId,
          contact_id: contact.id,
        };
      }
      const updatedAt = (() => {
        const rawValue = matchedConversation?.last_message_at || matchedConversation?.updated_at;
        if (!rawValue) {
          return runtimeSession?.updatedAt || contact.updatedAt;
        }
        const parsed = new Date(rawValue);
        return Number.isNaN(parsed.getTime())
          ? (runtimeSession?.updatedAt || contact.updatedAt)
          : parsed;
      })();
      return {
        id: contact.id,
        title: contact.name,
        createdAt: contact.createdAt,
        updatedAt,
        messageCount: 0,
        tokenUsage: 0,
        pinned: false,
        archived: false,
        status: 'active',
        metadata: nextMetadata,
      } as Session;
    });
  }, [contacts, displayBackingSessionIdMap, imConversations, sessions]);

  const currentDisplayScopeSessionId = useMemo(() => {
    if (activePanel !== 'chat') {
      return null;
    }
    const currentContactId = resolveContactIdFromScopeRecord(currentSession);
    if (currentContactId) {
      return currentContactId;
    }

    const currentContactAgentId = resolveContactAgentIdFromScopeRecord(currentSession);
    if (!currentContactAgentId) {
      return null;
    }
    const matched = contacts.find((item) => item.agentId === currentContactAgentId);
    if (!matched) {
      return null;
    }
    return matched.id;
  }, [activePanel, contacts, currentSession]);

  const activeSummaryDisplayScopeSessionId = useMemo(() => {
    if (!activeSummarySessionId || !currentSession?.id) {
      return null;
    }
    if (activeSummarySessionId !== currentSession.id) {
      return null;
    }
    return currentDisplayScopeSessionId;
  }, [activeSummarySessionId, currentDisplayScopeSessionId, currentSession?.id]);

  return {
    ensureBackingSessionForContactScope: ensureBackingSession,
    displayBackingSessionIdMap,
    displayScopeSessions,
    currentDisplayScopeSessionId,
    activeSummaryDisplayScopeSessionId,
    clearCachedBackingSessionIdsForContact,
    ensureSessionForContact: ensureBackingSession,
    displaySessionRuntimeIdMap: displayBackingSessionIdMap,
    displaySessions: displayScopeSessions,
    currentDisplaySessionId: currentDisplayScopeSessionId,
    activeSummaryDisplaySessionId: activeSummaryDisplayScopeSessionId,
    clearCachedSessionIdsForContact: clearCachedBackingSessionIdsForContact,
  };
};

export const useContactSessionListState = useContactScopeListState;
