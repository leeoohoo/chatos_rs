import type { Session } from '../../../types';
import { readSessionRuntimeFromMetadata } from '../helpers/sessionRuntime';

const SESSION_MESSAGES_CACHE_MAX_ENTRIES = 16;
type SessionMessagesCacheEntry = {
  fetchedAt: number;
  messages: any[];
};

const sessionMessagesPageCache = new Map<string, SessionMessagesCacheEntry>();

export const createPerfMeasureStopper = (measureName: string): (() => number | null) => {
  if (typeof performance === 'undefined' || typeof performance.mark !== 'function' || typeof performance.measure !== 'function') {
    return () => null;
  }

  const startMark = `${measureName}:start`;
  const endMark = `${measureName}:end`;
  performance.mark(startMark);

  return () => {
    performance.mark(endMark);
    performance.measure(measureName, startMark, endMark);
    const entries = performance.getEntriesByName(measureName);
    const duration = entries.length > 0 ? entries[entries.length - 1].duration : null;
    performance.clearMarks(startMark);
    performance.clearMarks(endMark);
    performance.clearMeasures(measureName);
    return duration;
  };
};

export const cloneStreamingMessageDraft = <T,>(value: T): T => {
  try {
    if (typeof structuredClone === 'function') {
      return structuredClone(value);
    }
  } catch {
    // ignore and fallback to JSON clone
  }

  try {
    return JSON.parse(JSON.stringify(value));
  } catch {
    return value;
  }
};

export const writeSessionMessagesCache = (sessionId: string, messages: any[]) => {
  sessionMessagesPageCache.set(sessionId, {
    fetchedAt: Date.now(),
    messages: cloneStreamingMessageDraft(messages),
  });

  while (sessionMessagesPageCache.size > SESSION_MESSAGES_CACHE_MAX_ENTRIES) {
    const oldestKey = sessionMessagesPageCache.keys().next().value;
    if (!oldestKey) {
      break;
    }
    sessionMessagesPageCache.delete(oldestKey);
  }
};

export const deleteSessionMessagesCacheEntry = (sessionId: string) => {
  sessionMessagesPageCache.delete(sessionId);
};

export const ensureSessionTurnMaps = (state: any, sessionId: string) => {
  if (!state.sessionTurnProcessState) {
    state.sessionTurnProcessState = {};
  }
  if (!state.sessionTurnProcessState[sessionId]) {
    state.sessionTurnProcessState[sessionId] = {};
  }

  if (!state.sessionTurnProcessCache) {
    state.sessionTurnProcessCache = {};
  }
  if (!state.sessionTurnProcessCache[sessionId]) {
    state.sessionTurnProcessCache[sessionId] = {};
  }
};

export const normalizeDate = (value: any): Date => {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? new Date() : parsed;
};

export const normalizeTurnId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const resolveSessionTimestamp = (session: Session): number => {
  const updated = new Date((session as any).updatedAt ?? (session as any).createdAt ?? Date.now());
  const ts = updated.getTime();
  return Number.isFinite(ts) ? ts : 0;
};

export const normalizeProjectScopeId = (projectId: string | null | undefined): string => {
  const trimmed = typeof projectId === 'string' ? projectId.trim() : '';
  return trimmed.length > 0 ? trimmed : '0';
};

export const resolveProjectScopeIdFromSessionRecord = (session: Session | null | undefined): string => {
  if (!session) {
    return '0';
  }
  const rawProjectId = typeof (session as any).projectId === 'string'
    ? (session as any).projectId.trim()
    : (typeof (session as any).project_id === 'string'
      ? (session as any).project_id.trim()
      : '');
  if (rawProjectId.length > 0) {
    return normalizeProjectScopeId(rawProjectId);
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  const runtimeProjectId = typeof runtime?.projectId === 'string'
    ? runtime.projectId.trim()
    : '';
  if (runtimeProjectId.length > 0) {
    return runtimeProjectId;
  }
  return '0';
};

export const resolveContactScopeIdentityFromSessionRecord = (session: Session | null | undefined): {
  contactId: string | null;
  contactAgentId: string | null;
} => {
  if (!session) {
    return { contactId: null, contactAgentId: null };
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  const contactId = typeof runtime?.contactId === 'string' ? runtime.contactId.trim() : '';
  const contactAgentId = typeof runtime?.contactAgentId === 'string' ? runtime.contactAgentId.trim() : '';
  return {
    contactId: contactId.length > 0 ? contactId : null,
    contactAgentId: contactAgentId.length > 0 ? contactAgentId : null,
  };
};

export const isSessionActive = (session: Session | null | undefined): boolean => {
  if (!session) {
    return false;
  }
  const status = typeof (session as any).status === 'string'
    ? (session as any).status.toLowerCase()
    : '';
  return !((session as any).archived || status === 'archived' || status === 'archiving');
};

export const matchContactProjectScopeSessionRecord = (
  session: Session | null | undefined,
  target: {
    contactId?: string | null;
    contactAgentId?: string | null;
    projectId: string;
  },
): boolean => {
  if (!isSessionActive(session)) {
    return false;
  }

  const contactId = typeof target.contactId === 'string' ? target.contactId.trim() : '';
  const contactAgentId = typeof target.contactAgentId === 'string' ? target.contactAgentId.trim() : '';
  const identity = resolveContactScopeIdentityFromSessionRecord(session);

  let sameContact = false;
  if (contactId) {
    sameContact = identity.contactId === contactId;
    if (!sameContact && contactAgentId) {
      sameContact = identity.contactAgentId === contactAgentId;
    }
  } else if (contactAgentId) {
    sameContact = identity.contactAgentId === contactAgentId;
  }

  if (!sameContact) {
    return false;
  }

  return resolveProjectScopeIdFromSessionRecord(session) === normalizeProjectScopeId(target.projectId);
};

export const splitSessionsByMappedContacts = (
  sessions: Session[],
  contacts: MemoryContact[],
): {
  matchedSessions: Session[];
  missingContacts: MemoryContact[];
} => {
  const contactsById = new Map(contacts.map((item) => [item.id, item]));
  const contactsByAgentId = new Map(contacts.map((item) => [item.agent_id, item]));

  const mappedContactIds = new Set<string>();
  const mappedContactAgentIds = new Set<string>();
  const matchedSessions = sessions.filter((session) => {
    if (!isSessionActive(session)) {
      return false;
    }
    const identity = resolveContactScopeIdentityFromSessionRecord(session);
    if (identity.contactId && contactsById.has(identity.contactId)) {
      mappedContactIds.add(identity.contactId);
      const mappedContact = contactsById.get(identity.contactId);
      if (mappedContact) {
        mappedContactAgentIds.add(mappedContact.agent_id);
      }
      return true;
    }
    if (identity.contactAgentId && contactsByAgentId.has(identity.contactAgentId)) {
      mappedContactAgentIds.add(identity.contactAgentId);
      const mappedContact = contactsByAgentId.get(identity.contactAgentId);
      if (mappedContact) {
        mappedContactIds.add(mappedContact.id);
      }
      return true;
    }
    return false;
  });

  const missingContacts = contacts.filter((contact) => {
    if (mappedContactIds.has(contact.id)) {
      return false;
    }
    return !mappedContactAgentIds.has(contact.agent_id);
  });

  return {
    matchedSessions,
    missingContacts,
  };
};

export const normalizeContactProjectScopeSessions = (sessions: Session[]): Session[] => {
  const byContactProject = new Map<string, Session>();
  for (const session of sessions) {
    const identity = resolveContactScopeIdentityFromSessionRecord(session);
    const contactKey = identity.contactId || identity.contactAgentId;
    if (!contactKey) {
      continue;
    }
    const key = `${contactKey}::${resolveProjectScopeIdFromSessionRecord(session)}`;
    const existing = byContactProject.get(key);
    if (!existing || resolveSessionTimestamp(session) >= resolveSessionTimestamp(existing)) {
      byContactProject.set(key, session);
    }
  }
  return Array.from(byContactProject.values()).sort(
    (a, b) => resolveSessionTimestamp(b) - resolveSessionTimestamp(a),
  );
};

export const resolveSessionProjectScopeId = resolveProjectScopeIdFromSessionRecord;
export const resolveSessionContactIdentity = resolveContactScopeIdentityFromSessionRecord;
export const matchSessionContactProjectScope = matchContactProjectScopeSessionRecord;
export const normalizeContactSessions = normalizeContactProjectScopeSessions;

export const resolveUserByTurnId = (messages: any[], turnId: string) => {
  if (!turnId) {
    return null;
  }

  return messages.find((message: any) => {
    if (message?.role !== 'user') {
      return false;
    }
    const messageTurnId = normalizeTurnId(
      message?.metadata?.conversation_turn_id || message?.metadata?.historyProcess?.turnId,
    );
    return messageTurnId === turnId;
  }) || null;
};

export const buildDraftUserMessageForStreaming = (
  sessionId: string,
  draftMessage: any,
  finalAssistantMessageId: string,
) => {
  const linkedUserMessageId = normalizeTurnId(
    typeof draftMessage?.metadata?.historyFinalForUserMessageId === 'string'
      ? draftMessage.metadata.historyFinalForUserMessageId
      : (
        typeof draftMessage?.metadata?.historyDraftUserMessage?.id === 'string'
          ? draftMessage.metadata.historyDraftUserMessage.id
          : ''
      )
  );
  const turnId = typeof draftMessage?.metadata?.conversation_turn_id === 'string'
    ? draftMessage.metadata.conversation_turn_id
    : '';
  const effectiveUserMessageId = linkedUserMessageId || (turnId ? `temp_user_turn_${turnId}` : '');
  if (!effectiveUserMessageId) {
    return null;
  }

  const draftUser = draftMessage?.metadata?.historyDraftUserMessage || {};

  return {
    id: effectiveUserMessageId,
    sessionId,
    role: 'user' as const,
    content: typeof draftUser.content === 'string' ? draftUser.content : '',
    status: 'completed' as const,
    createdAt: normalizeDate(draftUser.createdAt || draftMessage?.createdAt || Date.now()),
    metadata: {
      ...(turnId ? { conversation_turn_id: turnId } : {}),
      historyProcess: {
        hasProcess: false,
        toolCallCount: 0,
        thinkingCount: 0,
        processMessageCount: 0,
        userMessageId: effectiveUserMessageId,
        ...(turnId ? { turnId } : {}),
        finalAssistantMessageId: finalAssistantMessageId || null,
        expanded: false,
        loaded: false,
        loading: false,
      },
    },
  };
};

export type MemoryContact = {
  id: string;
  user_id: string;
  agent_id: string;
  agent_name_snapshot?: string | null;
  status?: string | null;
  created_at?: string;
  updated_at?: string;
};

export const normalizeContact = (value: any): MemoryContact | null => {
  if (!value || typeof value !== 'object') {
    return null;
  }
  const id = typeof value.id === 'string' ? value.id.trim() : '';
  const agentId = typeof value.agent_id === 'string' ? value.agent_id.trim() : '';
  const userId = typeof value.user_id === 'string' ? value.user_id.trim() : '';
  if (!id || !agentId || !userId) {
    return null;
  }
  return {
    id,
    user_id: userId,
    agent_id: agentId,
    agent_name_snapshot: typeof value.agent_name_snapshot === 'string'
      ? value.agent_name_snapshot.trim()
      : null,
    status: typeof value.status === 'string' ? value.status.trim() : null,
    created_at: typeof value.created_at === 'string' ? value.created_at : undefined,
    updated_at: typeof value.updated_at === 'string' ? value.updated_at : undefined,
  };
};
